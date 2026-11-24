//! Review immutable base/candidate trees. Only the explicit stage command creates a worktree.
use super::{
    base_ref, branch_ref, change_status, reference_commit, snapshot_worktree, tree_bytes,
    validate_change_id, AgentChange, LocalEngine,
};
use crate::agent_execution_gate::AgentExecutionGate;
use crate::local_engine::{text_file_diff, FileDiff};
use cowiki_knowledge::{
    embeddings::LocalHashProjectionEmbedder, governance::*, knowledge_index as index, okf,
};
use git2::{ObjectType, Oid, Repository, Tree};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceReview {
    as_of: String,
    findings: Vec<ReviewFinding>,
    proposals: Vec<ReviewRepair>,
    warnings: Vec<String>,
    can_stage: bool,
}

#[derive(Serialize)]
struct ReviewFinding {
    #[serde(flatten)]
    issue: AuditIssue,
    verdict: Option<Relation>,
    candidate_excerpt: Option<String>,
    evidence_excerpt: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewRepair {
    id: String,
    rationale: String,
    finding_ids: Vec<String>,
    diff: FileDiff,
    #[serde(skip)]
    selection: ReviewSelection,
}
fn materialize(repo: &Repository, tree: &Tree<'_>, out: &Path) -> Result<(), String> {
    copy_documents(repo, tree, out)?;
    // Raw sources are the one hidden subtree admitted by the knowledge index.
    if let Ok(sources) = tree.get_path(Path::new(okf::RAW_SOURCES_DIR)) {
        if sources.kind() == Some(ObjectType::Tree) {
            let dst = out.join(okf::RAW_SOURCES_DIR);
            fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
            let subtree = repo.find_tree(sources.id()).map_err(|e| e.to_string())?;
            copy_documents(repo, &subtree, &dst)?;
        }
    }
    Ok(())
}

fn copy_documents(repo: &Repository, tree: &Tree<'_>, out: &Path) -> Result<(), String> {
    for entry in tree.iter() {
        let Some(name) = entry.name() else { continue };
        if name.starts_with('.') || name.contains(['/', '\\', ':']) {
            continue;
        }
        let dst = out.join(name);
        match (entry.kind(), entry.filemode()) {
            (Some(ObjectType::Tree), _) => {
                fs::create_dir(&dst).map_err(|e| e.to_string())?;
                copy_documents(
                    repo,
                    &repo.find_tree(entry.id()).map_err(|e| e.to_string())?,
                    &dst,
                )?;
            }
            (Some(ObjectType::Blob), 0o100644 | 0o100755) => {
                if dst
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                {
                    let blob = repo.find_blob(entry.id()).map_err(|e| e.to_string())?;
                    fs::write(dst, blob.content()).map_err(|e| e.to_string())?;
                } else {
                    fs::File::create(dst).map_err(|e| e.to_string())?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Compare Draft to the latest checkpoint (or HEAD), or an Agent Change to its immutable creation checkpoint.
/// Snapshotting an open worktree writes only unreachable Git objects, never an index or ref.
struct ReviewTrees {
    base_commit: Oid,
    candidate: Oid,
    can_stage: bool,
}

impl LocalEngine {
    fn review_trees(
        &self,
        repo: &Repository,
        slug: &str,
        id: Option<&str>,
    ) -> Result<ReviewTrees, String> {
        let Some(id) = id else {
            let base_commit = match crate::local_engine::checkpoint_tip(repo)? {
                Some(id) => id,
                None => repo
                    .head()
                    .and_then(|r| r.peel_to_commit())
                    .map_err(|e| e.to_string())?
                    .id(),
            };
            return Ok(ReviewTrees {
                base_commit,
                candidate: snapshot_worktree(repo)?,
                can_stage: true,
            });
        };
        validate_change_id(id)?;
        let base_commit = reference_commit(repo, &base_ref(id))?.id();
        let can_stage = matches!(change_status(repo, id), "open" | "needsResolution");
        let candidate = if can_stage {
            let worktree = Repository::open(self.agent_change_worktree(slug, id)?)
                .map_err(|e| e.to_string())?;
            if worktree.head().ok().as_ref().and_then(|h| h.name()) != Some(branch_ref(id).as_str())
            {
                return Err("Agent Change left its managed Git branch".into());
            }
            snapshot_worktree(&worktree)?
        } else {
            reference_commit(repo, &branch_ref(id))?.tree_id()
        };
        Ok(ReviewTrees {
            base_commit,
            candidate,
            can_stage,
        })
    }

    pub fn review_governance(
        &self,
        slug: &str,
        id: Option<&str>,
        day: &str,
    ) -> Result<GovernanceReview, String> {
        let repo = self.repo(slug)?;
        let trees = self.review_trees(&repo, slug, id)?;
        build_review(&repo, &trees, slug, id, day)
    }

    pub fn stage_governance_as_new_change(
        &self,
        gate: &AgentExecutionGate,
        slug: &str,
        id: Option<&str>,
        day: &str,
        proposal_ids: &[String],
    ) -> Result<AgentChange, String> {
        if proposal_ids.is_empty() {
            return Err("Select at least one repair.".into());
        }
        let _source = gate.begin_close(slug, id.unwrap_or("current-draft"))?;
        let repo = self.repo(slug)?;
        let trees = self.review_trees(&repo, slug, id)?;
        if !trees.can_stage {
            return Err("This Agent Change is closed.".into());
        }
        let review = build_review(&repo, &trees, slug, id, day)?;
        let mut available: BTreeMap<_, _> = review
            .proposals
            .into_iter()
            .map(|p| (p.id, p.selection))
            .collect();
        let mut selections = Vec::new();
        for key in proposal_ids {
            selections.push(
                available
                    .remove(key)
                    .ok_or("Repair preview changed or selection is invalid. Refresh the audit.")?,
            );
        }
        let selected = combine_selections(selections)?;
        let base = repo
            .find_commit(trees.base_commit)
            .map_err(|e| e.to_string())?;
        let new = {
            let _mutation = self.lock_mutations()?;
            let current = self.review_trees(&repo, slug, id)?;
            if !current.can_stage
                || current.base_commit != trees.base_commit
                || current.candidate != trees.candidate
            {
                return Err("Review source changed. Refresh the audit.".into());
            }
            self.create_change_at_tree(
                slug,
                &repo,
                &base,
                trees.candidate,
                "Governance repair",
                "CoWiki Governance",
            )?
        };
        self.stage_governance_in_change(gate, slug, &new.id, &selected, day, base.tree_id())
            .map_err(|e| format!("Governance change {} remains open for review: {e}", new.id))
    }
}

/// Combine independent edits to the same candidate before the worktree writer
/// checks each file's preimage and resets its trust metadata once.
fn combine_selections(selections: Vec<ReviewSelection>) -> Result<Vec<ReviewSelection>, String> {
    let mut files = BTreeMap::<String, ReviewSelection>::new();
    for selected in selections {
        let path = &selected.proposal.patch.path;
        match files.entry(path.to_lowercase()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(selected);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                if existing.proposal.patch.path != *path {
                    return Err(format!("Selected paths differ only by case: {path}."));
                }
                existing
                    .proposal
                    .patch
                    .edits
                    .extend(selected.proposal.patch.edits);
                existing
                    .proposal
                    .evidence
                    .extend(selected.proposal.evidence);
            }
        }
    }
    for selected in files.values_mut() {
        let patch = &mut selected.proposal.patch;
        patch.edits.sort_by_key(|e| (e.start, e.end));
        patch.edits.dedup();
        if patch
            .edits
            .windows(2)
            .any(|w| w[0].end > w[1].start || w[0].start == w[1].start)
        {
            return Err(format!(
                "Selected repairs overlap in {}. Select one of the alternatives.",
                patch.path
            ));
        }
        patch.preview(&patch.path, &selected.candidate)?;
    }
    Ok(files.into_values().collect())
}

// Only actual source links in a checkpoint can ground a revision correction.
fn history(repo: &Repository, base: Oid, path: &Path) -> Result<Vec<Revision>, String> {
    let mut commit = repo.find_commit(base).map_err(|e| e.to_string())?;
    let mut revisions = Vec::<Revision>::new();
    let mut previous = None;
    for _ in 0..32 {
        let tree = commit.tree().map_err(|e| e.to_string())?;
        let Ok(entry) = tree.get_path(path) else {
            break;
        };
        if !matches!(entry.filemode(), 0o100644 | 0o100755) {
            break;
        }
        if previous != Some(entry.id()) {
            let blob = repo.find_blob(entry.id()).map_err(|e| e.to_string())?;
            let Ok(text) = std::str::from_utf8(blob.content()) else {
                break;
            };
            let sources = okf::parse_metadata(text).sources;
            revisions.push(Revision {
                id: commit.id().to_string(),
                path: path.to_string_lossy().into_owned(),
                text: text.into(),
                source_path: sources.first().cloned().unwrap_or_default(),
            });
            previous = Some(entry.id());
        }
        if commit.parent_count() == 0 {
            break;
        }
        commit = commit.parent(0).map_err(|e| e.to_string())?;
    }
    revisions.reverse();
    Ok(revisions)
}

fn build_review(
    repo: &Repository,
    trees: &ReviewTrees,
    slug: &str,
    id: Option<&str>,
    day: &str,
) -> Result<GovernanceReview, String> {
    if !okf::valid_iso_date(day) {
        return Err("Invalid review date.".into());
    }
    let base = repo
        .find_commit(trees.base_commit)
        .and_then(|c| c.tree())
        .map_err(|e| e.to_string())?;
    let candidate = repo.find_tree(trees.candidate).map_err(|e| e.to_string())?;
    let old = tempfile::tempdir().map_err(|e| e.to_string())?;
    let new = tempfile::tempdir().map_err(|e| e.to_string())?;
    materialize(repo, &base, old.path())?;
    materialize(repo, &candidate, new.path())?;
    let embedder = LocalHashProjectionEmbedder::default();
    let mut db = Connection::open_in_memory().map_err(|e| e.to_string())?;
    index::initialize(&mut db)?;
    index::rebuild_space_with_embedder(&mut db, "base", old.path(), &embedder)?;
    index::rebuild_space_with_embedder(&mut db, "candidate", new.path(), &embedder)?;
    let mut audit = audit_space(new.path(), &db, "candidate", day)?;
    let lookup = SnapshotIndex {
        connection: &db,
        space_id: "base",
        root: old.path(),
        embedder: &embedder,
    };
    let mut verdicts = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut semantic = Vec::new();
    let mut repairs = Vec::new();
    for delta in repo
        .diff_tree_to_tree(Some(&base), Some(&candidate), None)
        .map_err(|e| e.to_string())?
        .deltas()
    {
        let Some(path) = delta
            .new_file()
            .path()
            .and_then(Path::to_str)
            .filter(|p| eligible_concept(p))
        else {
            continue;
        };
        if !new.path().join(path).is_file() {
            continue;
        }
        let read = || -> Result<_, String> {
            let input = GovernanceInput {
                path: path.into(),
                request: "Review this local edit.".into(),
                base: base_text(repo, &base, path)?,
                candidate: fs::read_to_string(new.path().join(path)).map_err(|e| e.to_string())?,
                history: history(repo, trees.base_commit, Path::new(path))?,
            };
            lookup.locate(input, day).map(protocol::pack)
        };
        let packet = match read() {
            Ok(packet) => packet,
            Err(e) => {
                warnings.push(format!("Could not verify {path}: {e}"));
                continue;
            }
        };
        let judgments = RuleConflictDetector.classify(&packet)?;
        let issues = interpret(&packet, &judgments)?;
        let classified: BTreeMap<_, _> = judgments
            .iter()
            .map(|v| {
                (
                    (
                        &packet.changed[v.candidate].location,
                        &packet.evidence[v.evidence].claim.location,
                    ),
                    v.relation,
                )
            })
            .collect();
        for issue in &issues {
            if let Some((evidence, relation)) = issue
                .evidence
                .as_ref()
                .and_then(|e| classified.get(&(&issue.location, e)).map(|v| (e, v)))
            {
                verdicts.insert(
                    (issue.code.clone(), issue.location.clone(), evidence.clone()),
                    *relation,
                );
            }
        }
        if let Some(fix) = propose_claim_repairs(&packet, &issues) {
            repairs.push(fix);
        }
        semantic.extend(issues);
    }
    audit.add_semantic(&semantic, &repairs);
    let mut proposals = Vec::new();
    for proposal in audit.proposals {
        let path = &proposal.patch.path;
        if !eligible_concept(path) {
            continue;
        }
        let original = base_text(repo, &base, path)?;
        let current = fs::read_to_string(new.path().join(path)).map_err(|e| e.to_string())?;
        let transition = match prepare_review(&proposal.patch, &original, &current, day) {
            Ok(transition) => transition,
            Err(e) => {
                warnings.push(format!("Repair for {path} needs human review: {e}"));
                continue;
            }
        };
        // Link proposals to the exact lines and sources they repair. Two
        // unrelated findings on the same page must stay independently reviewable.
        let affected: BTreeSet<_> = proposal
            .patch
            .edits
            .iter()
            .map(|e| current[..e.start].bytes().filter(|b| *b == b'\n').count() + 1)
            .collect();
        let finding_ids = audit
            .issues
            .iter()
            .filter(|f| {
                f.path == *path
                    && f.location
                        .as_ref()
                        .is_some_and(|l| affected.range(l.first..=l.last).next().is_some())
                    && f.evidence
                        .as_ref()
                        .is_some_and(|e| proposal.evidence.contains(e))
            })
            .map(|f| f.id.clone())
            .collect();
        let key = digest(
            &serde_json::json!([
                slug,
                id,
                trees.base_commit.to_string(),
                trees.candidate.to_string(),
                day,
                proposal
            ])
            .to_string(),
        );
        proposals.push(ReviewRepair {
            id: key,
            rationale: proposal.rationale.clone(),
            finding_ids,
            diff: text_file_diff(path, Some(transition.before), Some(transition.after)),
            selection: ReviewSelection {
                base_digest: digest(&original),
                candidate: current,
                proposal,
            },
        });
    }
    let findings = audit
        .issues
        .into_iter()
        .map(|issue| {
            let verdict = issue
                .location
                .as_ref()
                .zip(issue.evidence.as_ref())
                .and_then(|(a, b)| verdicts.remove(&(issue.code.clone(), a.clone(), b.clone())));
            let evidence_root = if verdict.is_some() {
                old.path()
            } else {
                new.path()
            };
            ReviewFinding {
                candidate_excerpt: issue.location.as_ref().and_then(|c| excerpt(new.path(), c)),
                evidence_excerpt: issue
                    .evidence
                    .as_ref()
                    .and_then(|c| excerpt(evidence_root, c)),
                issue,
                verdict,
            }
        })
        .collect();
    Ok(GovernanceReview {
        as_of: day.into(),
        findings,
        proposals,
        warnings,
        can_stage: trees.can_stage,
    })
}

// Coordinates always refer to the full snapshot. Keep the display bounded for long source lines.
fn excerpt(root: &Path, citation: &Citation) -> Option<String> {
    let (_, path) = okf::safe_resolve_under_space(root, &citation.path).ok()?;
    let text = fs::read_to_string(path).ok()?;
    if digest(&text) != citation.snapshot {
        return None;
    }
    let start = citation.first.checked_sub(1)?;
    let lines = text
        .lines()
        .skip(start)
        .take(citation.last.checked_sub(start)?)
        .collect::<Vec<_>>()
        .join("\n");
    let mut chars = lines.chars();
    let mut display: String = chars.by_ref().take(320).collect();
    if chars.next().is_some() {
        display.push('…');
    }
    Some(display)
}

fn base_text(repo: &Repository, tree: &Tree<'_>, path: &str) -> Result<String, String> {
    String::from_utf8(tree_bytes(repo, tree, Path::new(path))?.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    const DAY: &str = "2026-11-24";
    const LINK: &str = "# Build guide\nSee [[old#Options|options]].\n";
    struct Fixture {
        _temp: tempfile::TempDir,
        root: std::path::PathBuf,
        engine: LocalEngine,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("wiki");
            fs::create_dir(&root).unwrap();
            let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
            engine.add_space("Notes", "notes", &root).unwrap();
            fs::write(root.join("guide.md"), "# Guide\n").unwrap();
            fs::write(
                root.join("old.md"),
                "---\ntype: Redirect\ntitle: Old guide\nredirect_to: guide.md\n---\n",
            )
            .unwrap();
            Self {
                _temp: temp,
                root,
                engine,
            }
        }
        fn review(&self, id: Option<&str>) -> GovernanceReview {
            self.engine.review_governance("notes", id, DAY).unwrap()
        }
    }
    #[test]
    fn review_and_stage_selected_repairs_from_unmerged_worktree() {
        let f = Fixture::new();
        let change = f.engine.create_agent_change("notes", "Build docs").unwrap();
        for path in ["a.md", "b.md", "c.md"] {
            fs::write(change.worktree_path.join(path), LINK).unwrap();
        }
        let repo = Repository::open(&f.root).unwrap();
        let source_head = || repo.refname_to_id(&branch_ref(&change.id)).unwrap();
        let head = source_head();
        let index = fs::read(f.root.join(".git/index")).unwrap();
        let read = || f.review(Some(&change.id));
        let review = read();
        let repairs: Vec<_> = review
            .proposals
            .iter()
            .filter(|p| matches!(p.diff.path.as_str(), "a.md" | "b.md"))
            .collect();
        assert_eq!(repairs.len(), 2);
        for p in &repairs {
            let issue = review
                .findings
                .iter()
                .find(|f| p.finding_ids.contains(&f.issue.id))
                .unwrap();
            assert_eq!(issue.issue.code, "inbound_redirect");
            assert_eq!(issue.issue.location.as_ref().unwrap().first, 2);
            assert_eq!(issue.issue.evidence.as_ref().unwrap().path, "old.md");
            assert!(issue.verdict.is_none());
        }
        assert_eq!(
            serde_json::to_value(&review).unwrap(),
            serde_json::to_value(read()).unwrap()
        );
        let ids: Vec<_> = repairs.iter().map(|p| p.id.clone()).collect();
        let gate = AgentExecutionGate::default();
        let stage = |ids: &[String]| {
            f.engine
                .stage_governance_as_new_change(&gate, "notes", Some(&change.id), DAY, ids)
        };
        assert!(stage(&[]).is_err());
        assert!(stage(&[ids[0].clone(), ids[0].clone()]).is_err());
        fs::write(change.worktree_path.join("guide.md"), "# New evidence\n").unwrap();
        assert!(stage(&ids).is_err());
        fs::write(change.worktree_path.join("guide.md"), "# Guide\n").unwrap();
        assert_eq!(
            repo.references_glob("refs/cowiki/agent-bases/*")
                .unwrap()
                .count(),
            1
        );
        let new = stage(&ids).unwrap();
        assert_ne!(change.id, new.id);
        assert_eq!(new.diffs.len(), 2);
        for p in repairs {
            assert_eq!(
                new.diffs.iter().find(|d| d.path == p.diff.path),
                Some(&p.diff)
            );
            assert_eq!(
                fs::read_to_string(change.worktree_path.join(&p.diff.path)).unwrap(),
                LINK
            );
            assert!(!f.root.join(&p.diff.path).exists());
        }
        assert_eq!(
            fs::read_to_string(new.worktree_path.join("c.md")).unwrap(),
            LINK
        );
        assert_eq!(source_head(), head);
        assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
        f.engine.discard_agent_change("notes", &change.id).unwrap();
        assert!(!read().can_stage);
        assert!(stage(&ids).is_err());
    }

    #[test]
    fn draft_audit_includes_unchanged_files_and_classifies_uncheckpointed_edits() {
        let f = Fixture::new();
        fs::write(f.root.join("limits.md"), "---\ntype: Policy\ntitle: Limits\ntrust_tier: human_verified\nverified: true\n---\n# Limits\nThe cache timeout is 30 seconds.\n").unwrap();
        fs::write(f.root.join("links.md"), LINK).unwrap();
        f.engine
            .create_checkpoint("notes", Some("Source policy"))
            .unwrap();
        fs::write(
            f.root.join("cache.md"),
            "# Cache\nThe cache timeout is 90 seconds.\nSee [[old]].\n",
        )
        .unwrap();
        let review = f.review(None);
        let conflict = review
            .findings
            .iter()
            .find(|f| f.issue.code == "contradicts_verified")
            .unwrap();
        assert_eq!(conflict.verdict, Some(Relation::Contradiction));
        assert_eq!(
            conflict.candidate_excerpt.as_deref(),
            Some("The cache timeout is 90 seconds.")
        );
        assert_eq!(
            conflict.evidence_excerpt.as_deref(),
            Some("The cache timeout is 30 seconds.")
        );
        assert_eq!(conflict.issue.location.as_ref().unwrap().path, "cache.md");
        assert_eq!(conflict.issue.evidence.as_ref().unwrap().path, "limits.md");
        assert!(review.proposals.iter().any(|p| p.diff.path == "links.md"));
        let repairs: Vec<_> = review
            .proposals
            .iter()
            .filter(|p| p.diff.path == "cache.md")
            .collect();
        assert_eq!(repairs.len(), 2);
        let ids: Vec<_> = repairs.iter().map(|p| p.id.clone()).collect();
        let gate = AgentExecutionGate::default();
        let staged = f
            .engine
            .stage_governance_as_new_change(&gate, "notes", None, DAY, &ids)
            .unwrap();
        assert_eq!(staged.diffs.len(), 1);
        assert_eq!(staged.diffs[0].path, "cache.md");
        assert_eq!(
            staged.diffs[0].new_content.as_deref(),
            Some("# Cache\nThe cache timeout is 30 seconds.\nSee [[guide]].\n")
        );
        assert_eq!(
            fs::read_to_string(f.root.join("cache.md")).unwrap(),
            "# Cache\nThe cache timeout is 90 seconds.\nSee [[old]].\n"
        );
        let first = repairs[0].selection.clone();
        let mut other = first.clone();
        other.proposal.patch.edits[0]
            .after
            .push_str(" conflicting alternative");
        assert!(combine_selections(vec![first, other])
            .unwrap_err()
            .contains("overlap"));
    }
}
