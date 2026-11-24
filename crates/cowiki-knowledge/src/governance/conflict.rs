use super::{claims, Citation, Evidence, EvidenceBundle};
use crate::okf;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Contradiction,
    Entailment,
    Neutral,
    ScopeDifference,
    Insufficient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub candidate: usize,
    pub evidence: usize,
    pub relation: Relation,
}

pub trait ConflictDetector {
    /// Remote verification consumes the shared call allowance on its first batch.
    fn allows_scratch_recheck(&self) -> bool {
        true
    }
    fn classify(&self, bundle: &EvidenceBundle) -> Result<Vec<Verdict>, String>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    pub location: Citation,
    pub evidence: Option<Citation>,
    pub message: String,
}

#[derive(PartialEq, Eq, Debug, Default)]
struct Statement {
    subject: BTreeSet<String>,
    values: Vec<(String, String)>,
    qualifiers: Vec<String>,
    negative: bool,
    comparison: Vec<String>,
    statuses: Vec<String>,
}

fn normalized_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('≥', " at least ")
        .replace('≤', " at most ")
        .replace(">=", " at least ")
        .replace("<=", " at most ")
        .replace('>', " more than ")
        .replace('<', " less than ")
        .replace("at most", " maximum ")
        .replace("no more than", " maximum ")
        .replace("at least", " minimum ")
        .replace("no less than", " minimum ")
        .replace("more than", " above ")
        .replace("less than", " below ")
        .split(|c: char| !c.is_alphanumeric() && !matches!(c, '-' | '.' | '%'))
        .map(|s| s.trim_matches(['.', '-']))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn unit(word: &str) -> (&str, f64) {
    match word.trim_end_matches('s') {
        "second" => ("seconds", 1.),
        "minute" => ("seconds", 60.),
        "hour" => ("seconds", 3600.),
        "day" => ("seconds", 86400.),
        "week" => ("seconds", 604800.),
        w => (w, 1.),
    }
}

fn statement(text: &str) -> Statement {
    let words = normalized_words(text);
    let mut s = Statement::default();
    let mut i = 0;
    while i < words.len() {
        let w = words[i].as_str();
        i += 1;
        if okf::valid_iso_date(w) {
            s.values.push(("date".into(), w.into()));
            continue;
        }
        if matches!(
            w,
            "version" | "scope" | "region" | "since" | "until" | "before" | "after"
        ) && i < words.len()
        {
            // A duration after a verb (e.g. "expires after 14 days") is a value.
            if !matches!(w, "before" | "after") || okf::valid_iso_date(&words[i]) {
                if matches!(w, "before" | "after") {
                    s.comparison.push(w.into());
                } else {
                    s.qualifiers.push(format!("{w} {}", words[i]));
                    i += 1;
                }
                continue;
            }
        }
        if w.strip_prefix('v')
            .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.'))
        {
            s.qualifiers.push(w.into());
            continue;
        }
        let numeric = match w.trim_end_matches('%') {
            "one" => Some(1.),
            "two" => Some(2.),
            "three" => Some(3.),
            "five" => Some(5.),
            "seven" => Some(7.),
            other => other.parse::<f64>().ok().filter(|n| n.is_finite()),
        };
        if let Some(n) = numeric {
            let next = words.get(i).map(String::as_str).unwrap_or("");
            let (u, scale) = if w.ends_with('%') {
                ("%", 1.)
            } else {
                unit(next)
            };
            if !w.ends_with('%') && !next.is_empty() {
                i += 1;
            }
            s.values.push((u.into(), (n * scale).to_string()));
            continue;
        }
        match w {
            "not" | "never" | "cannot" | "forbidden" | "prohibited" => s.negative = true,
            "minimum" | "maximum" | "above" | "below" => s.comparison.push(w.into()),
            "active" | "retired" | "enabled" | "disabled" | "deprecated" | "draft" => {
                s.statuses.push(w.into())
            }
            "a" | "an" | "the" | "to" | "of" | "is" | "are" | "be" | "must" | "may" | "can"
            | "every" | "at" | "in" | "for" | "after" | "automatically" => {}
            _ => {
                s.subject.insert(w.trim_end_matches('s').to_owned());
            }
        }
    }
    s
}

pub fn historical_context(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    text.starts_with('>')
        || [
            "under the retired policy",
            "historically,",
            "formerly,",
            "previously,",
            "in the retired version",
        ]
        .iter()
        .any(|p| text.starts_with(p))
}

fn sentence_relation(candidate: &str, evidence: &str) -> Relation {
    if historical_context(candidate) {
        return Relation::ScopeDifference;
    }
    let (a, b) = (statement(candidate), statement(evidence));
    if a.qualifiers != b.qualifiers && !a.qualifiers.is_empty() {
        return Relation::ScopeDifference;
    }
    if let Some(r) = super::alignment::facet_relation(candidate, evidence) {
        return r;
    }
    let intersection = a.subject.intersection(&b.subject).count();
    let union = a.subject.union(&b.subject).count();
    if intersection < 1 || 4 * intersection < 3 * union {
        return Relation::Neutral;
    }
    if a.qualifiers.is_empty() && !b.qualifiers.is_empty() {
        return Relation::Contradiction;
    }
    let comparable =
        a.values.len() == b.values.len() && a.values.iter().zip(&b.values).all(|(x, y)| x.0 == y.0);
    if !comparable {
        return Relation::Insufficient;
    }
    if a.negative != b.negative
        || a.values != b.values
        || a.comparison != b.comparison
        || (!a.statuses.is_empty() && !b.statuses.is_empty() && a.statuses != b.statuses)
    {
        return Relation::Contradiction;
    }
    if a == b {
        Relation::Entailment
    } else {
        Relation::Insufficient
    }
}

// Split only at sentence boundaries. A decimal, a version number or an
// identifier containing punctuation must remain a single claim.
pub(super) fn sentences(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        let end = i + ch.len_utf8();
        if matches!(ch, '.' | '!' | '?') && text[end..].starts_with(char::is_whitespace) {
            parts.push(text[start..end].trim());
            start = end;
        }
    }
    if !text[start..].trim().is_empty() {
        parts.push(text[start..].trim())
    }
    parts
}

pub fn rule_relation(candidate: &str, evidence: &str) -> Relation {
    if historical_context(candidate) {
        return Relation::ScopeDifference;
    }
    let source = sentences(evidence);
    let parts = sentences(candidate);
    if parts.len() == 1 && source.len() == 1 {
        return sentence_relation(candidate, evidence);
    }
    let mut all_entailed = !parts.is_empty();
    for claim in parts {
        let relations = source
            .iter()
            .enumerate()
            .map(|(i, s)| {
                super::alignment::scope_relation(claim, s, i.checked_sub(1).map(|n| source[n]))
                    .unwrap_or_else(|| sentence_relation(claim, s))
            })
            .collect::<Vec<_>>();
        if relations.contains(&Relation::Contradiction)
            && !relations.contains(&Relation::Entailment)
        {
            return Relation::Contradiction;
        }
        all_entailed &= relations.contains(&Relation::Entailment);
    }
    if all_entailed {
        Relation::Entailment
    } else {
        Relation::Neutral
    }
}

pub struct RuleConflictDetector;
impl ConflictDetector for RuleConflictDetector {
    fn classify(&self, bundle: &EvidenceBundle) -> Result<Vec<Verdict>, String> {
        let base_meta = okf::parse_metadata(&bundle.input.base);
        let new_meta = okf::parse_metadata(&bundle.input.candidate);
        let base = claims(&bundle.input.path, &bundle.input.base)?;
        let current = claims(&bundle.input.path, &bundle.input.candidate)?;
        let surviving: BTreeSet<_> = current
            .iter()
            .flat_map(|c| sentences(&c.text).into_iter().map(|s| (&c.heading, s)))
            .collect();
        let removed: Vec<_> = base
            .iter()
            .flat_map(|c| sentences(&c.text).into_iter().map(|s| (&c.heading, s)))
            .filter(|pair| !surviving.contains(pair))
            .collect();
        let mut results = Vec::new();
        for (candidate, claim) in bundle.changed.iter().enumerate() {
            let mut aligned = Vec::new();
            if base_meta.version == new_meta.version && base_meta.scope == new_meta.scope {
                for new in sentences(&claim.text) {
                    if let Some(old) = super::alignment::align_removed(
                        new,
                        removed
                            .iter()
                            .filter(|(h, _)| *h == &claim.heading)
                            .map(|(_, s)| *s),
                    ) {
                        aligned.push((new, old));
                    }
                }
            }
            for (evidence, item) in bundle.evidence.iter().enumerate() {
                let mut relation = rule_relation(&claim.text, &item.claim.text);
                if matches!(relation, Relation::Neutral | Relation::Insufficient)
                    && item.status == "active"
                    && item.human_verified
                {
                    let source = sentences(&item.claim.text);
                    if aligned.iter().any(|(new, old)| {
                        source.iter().any(|s| {
                            super::alignment::changed_facet(new, old, s)
                                == Some(Relation::Contradiction)
                        })
                    }) {
                        relation = Relation::Contradiction;
                    }
                }
                results.push(Verdict {
                    candidate,
                    evidence,
                    relation,
                });
            }
        }
        Ok(results)
    }
}

fn later_decision(
    bundle: &EvidenceBundle,
    original: &Evidence,
    claim: &str,
    sources: &[String],
) -> bool {
    bundle.evidence.iter().any(|decision| {
        decision.status == "active"
            && decision.human_verified
            && decision.supersedes.as_deref() == Some(original.claim.location.path.as_str())
            && sources.contains(&decision.claim.location.path)
            && decision.scope == original.scope
            && decision.updated_at.as_deref().is_some_and(|date| {
                okf::valid_iso_date(date)
                    && date <= bundle.as_of.as_str()
                    && original
                        .updated_at
                        .as_deref()
                        .is_some_and(|old| okf::valid_iso_date(old) && old < date)
            })
            && decision.claim.text.eq_ignore_ascii_case(claim)
    })
}

/// Convert entailment/contradiction judgments into issues using the immutable
/// evidence's lifecycle, never a candidate's self-asserted verification.
pub fn interpret(bundle: &EvidenceBundle, verdicts: &[Verdict]) -> Result<Vec<Issue>, String> {
    let meta = okf::parse_metadata(&bundle.input.candidate);
    let mut issues = BTreeSet::new();
    for v in verdicts {
        let claim = bundle
            .changed
            .get(v.candidate)
            .ok_or("unknown candidate citation")?;
        let e = bundle
            .evidence
            .get(v.evidence)
            .ok_or("unknown evidence citation")?;
        if (meta.scope != "default" && e.scope != "default" && meta.scope != e.scope)
            || meta.version != e.version && e.status != "retired"
            || historical_context(&claim.text)
            || later_decision(bundle, e, &claim.text, &meta.sources)
        {
            continue;
        }
        let result = match (v.relation, e.status.as_str()) {
            (Relation::Contradiction, "active") if e.human_verified => Some((
                Severity::Critical,
                "contradicts_verified",
                "Conflicts with an active human-verified claim.",
            )),
            (Relation::Entailment, "retired") => Some((
                Severity::Critical,
                "reintroduces_retired",
                "Repeats a withdrawn claim as current policy.",
            )),
            (Relation::Entailment, "active")
                if claim.location.path != e.claim.location.path
                    && !claim.heading.is_empty()
                    && claim.heading.eq_ignore_ascii_case(&e.claim.heading)
                    && claim.text.split_whitespace().count() >= 6 =>
            {
                Some((
                    Severity::Info,
                    "duplicate_overlap",
                    "Consider consolidating overlapping pages.",
                ))
            }
            _ => None,
        };
        if let Some((severity, code, message)) = result {
            issues.insert(Issue {
                severity,
                code: code.into(),
                location: claim.location.clone(),
                evidence: Some(e.claim.location.clone()),
                message: message.into(),
            });
        }
    }
    // A withdrawn version can also survive only in the supplied revision
    // history. Require an intervening correction grounded in retrieved evidence.
    for pair in bundle.input.history.windows(2) {
        let (old, new) = (&pair[0], &pair[1]);
        if old.path != bundle.input.path || new.path != old.path {
            continue;
        }
        let prior = claims(&old.path, &old.text)?;
        let corrected = claims(&new.path, &new.text)?;
        for finding in issues
            .clone()
            .iter()
            .filter(|x| x.code == "contradicts_verified")
        {
            let Some(e) = finding
                .evidence
                .as_ref()
                .and_then(|c| bundle.evidence.iter().find(|e| &e.claim.location == c))
            else {
                continue;
            };
            if new.source_path != e.claim.location.path
                || !corrected
                    .iter()
                    .any(|c| rule_relation(&c.text, &e.claim.text) == Relation::Entailment)
            {
                continue;
            }
            let Some(c) = bundle
                .changed
                .iter()
                .find(|c| c.location == finding.location)
            else {
                continue;
            };
            if issues
                .iter()
                .any(|i| i.code == "reintroduces_retired" && i.location == c.location)
            {
                continue;
            }
            if prior
                .iter()
                .any(|p| rule_relation(&c.text, &p.text) == Relation::Entailment)
            {
                issues.insert(Issue {
                    severity: Severity::Warning,
                    code: "revision_regression".into(),
                    location: c.location.clone(),
                    evidence: Some(e.claim.location.clone()),
                    message: "Restores a claim replaced by a source-backed correction.".into(),
                });
            }
        }
    }
    Ok(issues.into_iter().collect())
}

pub(super) fn safe_to_replace_line(candidate: &str, supported: &str) -> bool {
    let a = statement(candidate);
    let b = statement(supported);
    a.subject == b.subject
        && (a.qualifiers == b.qualifiers || a.qualifiers.is_empty())
        && !candidate.contains(['[', '`', ';'])
        && !supported.contains(['[', '`', ';'])
}
