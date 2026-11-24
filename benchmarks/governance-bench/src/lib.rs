//! Governance evaluation inputs and independent scoring. Methods receive a
//! Case and its Family, while only the evaluator receives the matching Gold.
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

#[derive(Debug, Deserialize)]
pub struct Family {
    pub id: String,
    pub split: String,
    pub source_paths: Vec<String>,
    pub pages: BTreeMap<String, String>,
}

pub use cowiki_knowledge::governance::Revision;

#[derive(Debug, Deserialize, Serialize)]
pub struct Case {
    pub id: String,
    pub family: String,
    pub request: String,
    pub path: String,
    pub base: String,
    pub candidate: String,
    pub history: Vec<Revision>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub path: String,
    pub first: usize,
    pub last: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub code: String,
    pub candidate: Span,
    pub evidence: Span,
}

#[derive(Debug, Deserialize)]
pub struct Gold {
    pub id: String,
    pub primary_type: String,
    pub issues: Vec<Finding>,
    // Canonical acceptable outcomes for the controlled pilot edits.
    pub acceptable: Vec<String>,
    pub protected: Vec<String>,
}

pub struct Dataset {
    pub families: BTreeMap<String, Family>,
    pub cases: Vec<Case>,
    pub gold: BTreeMap<String, Gold>,
}

/// Internal family-level development partition, fixed before this tuning round.
/// Pilot selection uses case IDs only and is shared by all model arms.
pub fn select_dev_slice(dataset: &mut Dataset, split: &str, slice: &str) -> Result<(), String> {
    use cowiki_knowledge::governance::digest;
    if split != "dev" {
        return Err("--dev-slice requires --split dev".into());
    }
    let mut families: Vec<_> = dataset.families.keys().collect();
    families.sort_by_cached_key(|id| digest(&format!("issue169-dev-validation-v1/{id}")));
    match slice {
        "tune" | "validation" => {
            if families.len() != 8 {
                return Err("development partition expects eight families".into());
            }
            let validation = &families[..2];
            dataset
                .cases
                .retain(|case| validation.contains(&&case.family) == (slice == "validation"));
        }
        "pilot" => {
            let mut selected = BTreeSet::new();
            for family in families {
                let mut cases: Vec<_> = dataset
                    .cases
                    .iter()
                    .filter(|c| &c.family == family)
                    .collect();
                cases.sort_by_cached_key(|c| digest(&format!("issue169-model-pilot-v1/{}", c.id)));
                selected.extend(cases.into_iter().take(2).map(|c| c.id.clone()));
            }
            if selected.len() > 16 {
                return Err("pilot exceeds 16 cases".into());
            }
            dataset.cases.retain(|c| selected.contains(&c.id));
        }
        _ => return Err(format!("unknown development slice {slice}")),
    }
    Ok(())
}

fn records<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, String> {
    let contents = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    contents
        .lines()
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line).map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))
        })
        .collect()
}

fn validate_span(span: &Span, text: &str) -> Result<(), String> {
    if span.first == 0 || span.first > span.last || span.last > text.lines().count() {
        return Err(format!("invalid span {span:?}"));
    }
    Ok(())
}

fn page_path(path: &str) -> bool {
    path.ends_with(".md")
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.'))
        && Path::new(path)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
}

pub fn load_dataset(root: &Path, split: &str) -> Result<Dataset, String> {
    if !matches!(split, "dev" | "held-out") {
        return Err(format!("unknown split {split}"));
    }
    let mut families = BTreeMap::new();
    let mut source_owners = BTreeMap::new();
    for family in records::<Family>(&root.join(split).join("families.jsonl"))? {
        if !matches!(family.split.as_str(), "dev" | "held-out")
            || family.source_paths.is_empty()
            || family.pages.is_empty()
            || family.pages.keys().any(|path| !page_path(path))
        {
            return Err(format!("invalid family {}", family.id));
        }
        for path in &family.source_paths {
            if let Some(owner) = source_owners.insert(path.clone(), family.id.clone()) {
                return Err(format!(
                    "source {path} shared by families {owner} and {}",
                    family.id
                ));
            }
        }
        if families.insert(family.id.clone(), family).is_some() {
            return Err("duplicate family id".into());
        }
    }
    if families.values().any(|family| family.split != split) {
        return Err("family manifest contains an entry from another split".into());
    }
    let cases: Vec<Case> = records(&root.join(split).join("cases.jsonl"))?;
    let mut gold = BTreeMap::new();
    for item in records::<Gold>(&root.join(split).join("gold.jsonl"))? {
        if gold.insert(item.id.clone(), item).is_some() {
            return Err("duplicate gold id".into());
        }
    }
    let mut ids = BTreeSet::new();
    for case in &cases {
        let family = families
            .get(&case.family)
            .ok_or_else(|| format!("{}: unknown family", case.id))?;
        let label = gold
            .get(&case.id)
            .ok_or_else(|| format!("{}: missing gold", case.id))?;
        if !ids.insert(&case.id)
            || family.split != split
            || !page_path(&case.path)
            || case.candidate == case.base
            || label.acceptable.is_empty()
        {
            return Err(format!("{}: invalid or misplaced case", case.id));
        }
        if label.acceptable.contains(&case.candidate) != label.issues.is_empty() {
            return Err(format!("{}: labels disagree with accepted state", case.id));
        }
        if label.issues.iter().collect::<BTreeSet<_>>().len() != label.issues.len() {
            return Err(format!("{}: duplicate gold issue", case.id));
        }
        for issue in &label.issues {
            if issue.candidate.path != case.path {
                return Err(format!("{}: citation outside candidate", case.id));
            }
            validate_span(&issue.candidate, &case.candidate)?;
            let structural = matches!(
                issue.code.as_str(),
                "broken_link"
                    | "inbound_redirect"
                    | "broken_redirect"
                    | "redirect_cycle"
                    | "missing_generated_sources"
                    | "expired_stale_after"
            );
            let text = if structural && issue.evidence.path == case.path {
                &case.candidate
            } else {
                family
                    .pages
                    .get(&issue.evidence.path)
                    .ok_or_else(|| format!("{}: unknown evidence path", case.id))?
            };
            validate_span(&issue.evidence, text)?;
        }
        if label.protected.iter().any(|s| {
            s.is_empty()
                || !case.candidate.contains(s)
                || label.acceptable.iter().any(|text| !text.contains(s))
        }) {
            return Err(format!("{}: invalid protected content", case.id));
        }
        for rev in &case.history {
            if rev.path != case.path || !family.pages.contains_key(&rev.source_path) {
                return Err(format!("{}: invalid revision evidence", case.id));
            }
        }
    }
    if ids.is_empty() || ids.len() != gold.len() {
        return Err("empty split or unmatched gold".into());
    }
    Ok(Dataset {
        families,
        cases,
        gold,
    })
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Counts {
    pub cases: usize,
    pub tp: usize,
    pub fp: usize,
    #[serde(rename = "fn")]
    pub fn_: usize,
    pub valid_cases: usize,
    pub valid_false_alarms: usize,
    pub faulty_cases: usize,
    pub repaired_cases: usize,
    pub residual_cases: usize,
    pub unintended_changes: usize,
}

impl Counts {
    pub fn observe(&mut self, case: &Case, label: &Gold, findings: &[Finding], final_text: &str) {
        let mut remaining: BTreeSet<_> = label.issues.iter().collect();
        for finding in findings {
            if remaining.remove(finding) {
                self.tp += 1;
            } else {
                self.fp += 1;
            }
        }
        self.fn_ += remaining.len();
        self.cases += 1;
        let accepted = label.acceptable.iter().any(|s| s == final_text);
        if label.issues.is_empty() {
            self.valid_cases += 1;
            self.valid_false_alarms += usize::from(!findings.is_empty());
        } else {
            self.faulty_cases += 1;
            self.repaired_cases += usize::from(accepted);
            self.residual_cases += usize::from(!accepted);
        }
        self.unintended_changes += usize::from(
            (label.issues.is_empty() && final_text != case.candidate && !accepted)
                || label.protected.iter().any(|s| !final_text.contains(s)),
        );
    }

    pub fn metrics(&self) -> serde_json::Value {
        let ratio = |n: usize, d: usize| (d != 0).then(|| n as f64 / d as f64);
        serde_json::json!({
            "counts": self,
            "precision": ratio(self.tp, self.tp + self.fp),
            "recall": ratio(self.tp, self.tp + self.fn_),
            "f1": ratio(2*self.tp, 2*self.tp + self.fp + self.fn_),
            "repair_success": ratio(self.repaired_cases, self.faulty_cases),
            "false_alarm_rate": ratio(self.valid_false_alarms, self.valid_cases),
            "unintended_change_rate": ratio(self.unintended_changes, self.cases),
        })
    }
}

pub fn evidence_packet(
    family: &Family,
    case: &Case,
) -> Result<cowiki_knowledge::governance::EvidenceBundle, String> {
    Ok(prepare_case(family, case, false)?.0)
}

/// Audit the edit on disposable snapshots only for arms that include symbolic
/// checks. Both snapshots share the immutable family pages and the same date.
pub fn prepare_case(
    family: &Family,
    case: &Case,
    check_structure: bool,
) -> Result<
    (
        cowiki_knowledge::governance::EvidenceBundle,
        Vec<cowiki_knowledge::governance::Issue>,
    ),
    String,
> {
    use cowiki_knowledge::{
        embeddings::LocalHashProjectionEmbedder,
        governance::{audit_space, introduced, EvidenceLookup, GovernanceInput, SnapshotIndex},
        knowledge_index, okf,
    };
    let space = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut pages = family.pages.clone();
    if !case.base.is_empty() {
        pages.insert(case.path.clone(), case.base.clone());
    }
    for (path, text) in pages {
        let (_, file) = okf::safe_resolve_under_space(space.path(), &path)?;
        fs::create_dir_all(file.parent().ok_or("invalid page path")?).map_err(|e| e.to_string())?;
        fs::write(file, text).map_err(|e| e.to_string())?;
    }
    let mut db = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;
    let embedder = LocalHashProjectionEmbedder::default();
    knowledge_index::initialize(&mut db)?;
    knowledge_index::rebuild_space_with_embedder(&mut db, "base", space.path(), &embedder)?;
    let packet = SnapshotIndex {
        connection: &db,
        space_id: "base",
        root: space.path(),
        embedder: &embedder,
    }
    .locate(
        GovernanceInput {
            path: case.path.clone(),
            request: case.request.clone(),
            base: case.base.clone(),
            candidate: case.candidate.clone(),
            history: case.history.clone(),
        },
        "2026-11-23",
    )?;
    let mut findings = Vec::new();
    if check_structure {
        let before = audit_space(space.path(), &db, "base", &packet.as_of)?;
        let (_, file) = okf::safe_resolve_under_space(space.path(), &case.path)?;
        fs::create_dir_all(file.parent().ok_or("invalid candidate path")?)
            .map_err(|e| e.to_string())?;
        fs::write(file, &case.candidate).map_err(|e| e.to_string())?;
        knowledge_index::rebuild_space_with_embedder(
            &mut db,
            "candidate",
            space.path(),
            &embedder,
        )?;
        let after = audit_space(space.path(), &db, "candidate", &packet.as_of)?;
        findings = introduced(&before, &after, &case.path);
        findings.retain(|i| {
            matches!(
                i.code.as_str(),
                "broken_link"
                    | "inbound_redirect"
                    | "broken_redirect"
                    | "redirect_cycle"
                    | "missing_generated_sources"
                    | "expired_stale_after"
            )
        });
    }
    Ok((packet, findings))
}

pub fn semantic_findings(issues: &[cowiki_knowledge::governance::Issue]) -> Vec<Finding> {
    let span = |c: &cowiki_knowledge::governance::Citation| Span {
        path: c.path.clone(),
        first: c.first,
        last: c.last,
    };
    issues
        .iter()
        .filter_map(|i| {
            Some(Finding {
                code: i.code.clone(),
                candidate: span(&i.location),
                evidence: span(i.evidence.as_ref()?),
            })
        })
        .collect()
}

pub mod evaluation;
