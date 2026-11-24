//! Read-only Space audit. The caller supplies the derived index for the same snapshot.
use super::{digest, Citation, Issue, RepairProposal, Severity};
use crate::{
    knowledge_index::{self as index, FileIndex, LinkKind},
    okf::{self, DocumentKind},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    ops::Range,
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditIssue {
    pub id: String,
    pub code: String,
    pub severity: Severity,
    pub path: String,
    /// Absent when a file has no readable source span.
    pub location: Option<Citation>,
    pub evidence: Option<Citation>,
    pub subject: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceAuditReport {
    pub schema_version: u32,
    pub snapshot: String,
    pub as_of: String,
    pub issues: Vec<AuditIssue>,
    pub by_code: BTreeMap<String, usize>,
    pub by_severity: BTreeMap<Severity, usize>,
    pub proposals: Vec<RepairProposal>,
}

struct Page {
    text: String,
    snapshot: String,
    meta: okf::OkfMetadata,
}

impl Page {
    fn span(&self, path: &str, first: usize, last: usize) -> Citation {
        Citation {
            path: path.into(),
            snapshot: self.snapshot.clone(),
            first,
            last,
        }
    }
    fn field(&self, path: &str, name: &str) -> Option<Citation> {
        let (yaml, _) = okf::split_frontmatter(&self.text).ok()?;
        let line = yaml?.lines().position(|line| {
            line.split_once(':')
                .is_some_and(|(key, _)| key.trim_end() == name)
        })? + 2;
        Some(self.span(path, line, line))
    }
    fn offsets(&self, path: &str, r: Range<usize>) -> Citation {
        let first = 1 + self.text[..r.start].bytes().filter(|b| *b == b'\n').count();
        let last = first
            + self.text[r]
                .trim_end_matches(['\r', '\n'])
                .bytes()
                .filter(|b| *b == b'\n')
                .count();
        self.span(path, first, last)
    }
}

fn issue(
    code: &str,
    severity: Severity,
    path: &str,
    location: Option<Citation>,
    evidence: Option<Citation>,
    subject: Option<String>,
    message: String,
) -> AuditIssue {
    let key = serde_json::json!([code, path, location, evidence, subject, message]);
    AuditIssue {
        id: digest(&key.to_string()),
        code: code.into(),
        severity,
        path: path.into(),
        location,
        evidence,
        subject,
        message,
    }
}

fn field_for(code: &str) -> Option<&str> {
    match code {
        "invalid_status" => Some("status"),
        "invalid_trust_tier" => Some("trust_tier"),
        "trust_reset_violation" => Some("verified"),
        "invalid_stale_after" | "expired_stale_after" => Some("stale_after"),
        "broken_redirect" | "redirect_cycle" => Some("redirect_to"),
        _ => None,
    }
}

pub fn audit_space(
    root: &Path,
    connection: &Connection,
    space_id: &str,
    as_of: &str,
) -> Result<GovernanceAuditReport, String> {
    if !okf::valid_iso_date(as_of) {
        return Err("audit requires an explicit ISO date".into());
    }
    let validation = okf::validate_bundle(root, Some(as_of), false)?;
    let broken: BTreeSet<_> = index::broken_links(connection, space_id, root)?
        .into_iter()
        .map(|l| (l.source_path, l.target))
        .collect();
    let files = FileIndex::scan(root)?;
    let mut pages = BTreeMap::new();
    for path in files.files().iter().filter(|p| p.ends_with(".md")) {
        let (_, full) = okf::safe_resolve_under_space(root, path)?;
        // The validator retains unreadable-file diagnostics.
        if let Ok(text) = fs::read_to_string(full) {
            pages.insert(
                path.as_str(),
                Page {
                    meta: okf::parse_metadata(&text),
                    snapshot: digest(&text),
                    text,
                },
            );
        }
    }
    let mut issues = Vec::new();
    for v in validation.issues {
        let location = pages
            .get(v.path.as_str())
            .and_then(|page| field_for(&v.code).and_then(|key| page.field(&v.path, key)));
        let severity = if v.severity == "error" {
            Severity::Critical
        } else {
            Severity::Warning
        };
        let evidence = if v.code == "redirect_cycle" {
            pages
                .get(v.path.as_str())
                .and_then(|source| {
                    index::resolve_markdown_target(&v.path, source.meta.redirect_to.as_ref()?)
                })
                .and_then(|target| pages.get(target.as_str())?.field(&target, "redirect_to"))
        } else {
            None
        };
        issues.push(issue(
            &v.code, severity, &v.path, location, evidence, None, v.message,
        ));
    }
    for (&path, page) in &pages {
        if DocumentKind::from_path(Path::new(path)) != DocumentKind::Concept
            || path.split('/').any(|s| s.starts_with('.'))
        {
            continue;
        }
        let Ok((_, body)) = okf::split_frontmatter(&page.text) else {
            continue;
        };
        if page.meta.trust_tier == "agent_draft" && !page.meta.is_human_verified() {
            issues.push(issue(
                "unverified_agent_draft",
                Severity::Info,
                path,
                page.field(path, "trust_tier"),
                None,
                None,
                "Awaiting human review.".into(),
            ));
        }
        if page.meta.generated && page.meta.sources.is_empty() {
            issues.push(issue(
                "missing_generated_sources",
                Severity::Warning,
                path,
                page.field(path, "generated"),
                None,
                None,
                "Generated page has no cited source.".into(),
            ));
        }
        let prefix = page.text.len() - body.len();
        for link in index::link_occurrences(body, path) {
            let location =
                Some(page.offsets(path, prefix + link.range.start..prefix + link.range.end));
            if broken.contains(&(path.to_owned(), link.target.clone())) {
                issues.push(issue(
                    "broken_link",
                    Severity::Warning,
                    path,
                    location,
                    None,
                    Some(link.target.clone()),
                    format!("Unresolved internal link to {}.", link.target),
                ));
            } else {
                let target = match link.kind {
                    LinkKind::Markdown => Some(link.target),
                    LinkKind::Wiki => files.resolve_wikilink(&link.target, path),
                };
                if let Some((target, stub)) = target
                    .as_ref()
                    .and_then(|p| Some((p, pages.get(p.as_str())?)))
                    .filter(|(_, p)| p.meta.redirect_to.is_some())
                {
                    issues.push(issue(
                        "inbound_redirect",
                        Severity::Warning,
                        path,
                        location,
                        stub.field(target, "redirect_to"),
                        Some(target.clone()),
                        format!("Link points to redirect stub {target}."),
                    ));
                }
            }
        }
    }
    let fields: super::RedirectFields = pages
        .iter()
        .filter_map(|(&path, p)| {
            Some((
                path.to_owned(),
                (p.meta.redirect_to.clone()?, p.field(path, "redirect_to")?),
            ))
        })
        .collect();
    let proposals = pages
        .iter()
        .filter_map(|(&path, p)| super::propose_redirects(path, &p.text, &files, &fields))
        .collect();
    let snapshot = digest(
        &serde_json::json!(pages
            .iter()
            .map(|(p, d)| (p, &d.snapshot))
            .collect::<Vec<_>>())
        .to_string(),
    );
    let mut report = GovernanceAuditReport {
        schema_version: 1,
        snapshot,
        as_of: as_of.into(),
        issues,
        by_code: BTreeMap::new(),
        by_severity: BTreeMap::new(),
        proposals,
    };
    report.summarize();
    Ok(report)
}

impl GovernanceAuditReport {
    pub fn add_semantic(&mut self, findings: &[Issue], proposals: &[RepairProposal]) {
        self.issues.extend(findings.iter().map(|i| {
            issue(
                &i.code,
                i.severity,
                &i.location.path,
                Some(i.location.clone()),
                i.evidence.clone(),
                None,
                i.message.clone(),
            )
        }));
        self.proposals.extend_from_slice(proposals);
        self.summarize();
    }
    fn summarize(&mut self) {
        self.issues.sort_by(|a, b| {
            let key = |i: &AuditIssue| (i.severity, i.location.as_ref().map_or(0, |c| c.first));
            key(a)
                .0
                .cmp(&key(b).0)
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| key(a).1.cmp(&key(b).1))
                .then_with(|| a.code.cmp(&b.code))
                .then_with(|| a.id.cmp(&b.id))
        });
        self.issues.dedup_by(|a, b| a.id == b.id);
        self.by_severity.clear();
        self.by_code.clear();
        for i in &self.issues {
            *self.by_code.entry(i.code.clone()).or_default() += 1;
            *self.by_severity.entry(i.severity).or_default() += 1;
        }
    }
}

/// Cancel matched occurrences across a revision, including when unrelated text
/// moved one of them down the page. A candidate is accountable for newly
/// introduced issues on its own path only.
pub fn introduced(
    before: &GovernanceAuditReport,
    after: &GovernanceAuditReport,
    path: &str,
) -> Vec<Issue> {
    let key = |i: &AuditIssue| (i.code.clone(), i.subject.clone(), i.message.clone());
    let mut existing = BTreeMap::new();
    for i in before.issues.iter().filter(|i| i.path == path) {
        *existing.entry(key(i)).or_insert(0usize) += 1;
    }
    let mut result = vec![];
    for i in after.issues.iter().filter(|i| i.path == path) {
        let count = existing.entry(key(i)).or_default();
        if *count > 0 {
            *count -= 1;
            continue;
        }
        if let Some(location) = &i.location {
            result.push(Issue {
                severity: i.severity,
                code: i.code.clone(),
                location: location.clone(),
                evidence: i.evidence.clone().or_else(|| Some(location.clone())),
                message: i.message.clone(),
            });
        }
    }
    result
}

/// Raw link inventory visible to every method, with page-relative source line
/// numbers. Missing entries are recorded explicitly. No audit verdicts enter it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkRecord {
    pub target: Option<String>,
    pub redirect: Option<(usize, String)>,
    pub snapshot: Option<String>,
}

pub fn link_inventory(
    root: &Path,
    input: &super::GovernanceInput,
) -> Result<BTreeMap<String, LinkRecord>, String> {
    let mut todo = Vec::new();
    fn follow_redirect(todo: &mut Vec<(String, String, LinkKind)>, path: &str, raw: &str) {
        if let Some(target) = index::resolve_markdown_target(path, raw) {
            todo.push((path.into(), target, LinkKind::Markdown))
        }
    }
    for doc in [&input.base, &input.candidate] {
        if let Ok((_, body)) = okf::split_frontmatter(doc) {
            todo.extend(
                index::extract_links(body, &input.path)
                    .into_iter()
                    .map(|(t, k)| (input.path.clone(), t, k)),
            );
        }
        if let Some(raw) = okf::parse_metadata(doc).redirect_to {
            follow_redirect(&mut todo, &input.path, &raw);
        }
    }
    let mut inventory = BTreeMap::new();
    if todo.is_empty() {
        return Ok(inventory);
    }
    let files = FileIndex::scan(root)?;
    while let Some((source, stored, kind)) = todo.pop() {
        let key = format!(
            "{} {} {}",
            source,
            if kind == LinkKind::Wiki { "wiki" } else { "md" },
            stored
        );
        if inventory.contains_key(&key) {
            continue;
        }
        let target = match kind {
            LinkKind::Markdown => okf::safe_resolve_under_space(root, &stored)
                .ok()
                .filter(|(_, p)| p.exists())
                .map(|_| stored),
            LinkKind::Wiki => files.resolve_wikilink(&stored, &source),
        };
        let mut record = LinkRecord {
            target: target.clone(),
            redirect: None,
            snapshot: None,
        };
        if let Some(path) = target.filter(|p| p.ends_with(".md")) {
            if let Ok((_, file)) = okf::safe_resolve_under_space(root, &path) {
                if let Ok(text) = fs::read_to_string(file) {
                    let p = Page {
                        snapshot: digest(&text),
                        meta: okf::parse_metadata(&text),
                        text,
                    };
                    if let (Some(raw), Some(line)) =
                        (&p.meta.redirect_to, p.field(&path, "redirect_to"))
                    {
                        record.redirect = Some((line.first, raw.clone()));
                        follow_redirect(&mut todo, &path, raw);
                    }
                    record.snapshot = Some(p.snapshot);
                }
            }
        }
        inventory.insert(key, record);
    }
    Ok(inventory)
}
