//! One-call suggestions are untrusted. Every accepted patch has an original
//! snapshot precondition and has survived deterministic checks on cumulative scratch.
use crate::governance::{
    changed_claims, claims, conflict::sentences, digest, interpret, propose_claim_repairs,
    rule_relation, Citation, ConflictDetector, EvidenceBundle, FilePatch, GovernanceAuditReport,
    Issue, MaintenanceResult, Relation, RepairCheck, RepairProposal, RuleConflictDetector,
    TextEdit, Verdict,
};
use crate::{knowledge_index::code_ranges, okf};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A sparse attachment to a (candidate, evidence) judgment. Support can
/// differ from the finding evidence when the latter is a retired source.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CitedRepair {
    pub candidate: usize,
    pub evidence: usize,
    pub support: usize,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RepairAttachment {
    Proposal(CitedRepair),
    Malformed(serde_json::Value),
}

type Rejection = &'static str;
type Audit<'a> = &'a dyn Fn(&str) -> Result<GovernanceAuditReport, String>;

fn claim_fault(issue: &Issue) -> bool {
    matches!(
        issue.code.as_str(),
        "contradicts_verified" | "reintroduces_retired" | "revision_regression"
    )
}

fn line_at(text: &str, line: usize) -> Option<(usize, &str)> {
    let mut offset = 0;
    for (idx, raw) in text.split_inclusive('\n').enumerate() {
        if idx + 1 == line {
            return Some((offset + raw.len() - raw.trim_start().len(), raw.trim()));
        }
        offset += raw.len();
    }
    None
}

/// Keep code identifiers, negation, operators, units and case significant.
fn normalized(s: &str) -> Vec<&str> {
    s.trim_end_matches('.').split_whitespace().collect()
}

fn grounded(
    old: &str,
    new: &str,
    source: &str,
    base: &[crate::governance::Claim],
    heading: &str,
) -> bool {
    let (prior, next) = (sentences(old), sentences(new));
    if prior.len() != next.len() {
        return false;
    }
    let mut edits = prior.iter().zip(&next).filter(|(a, b)| a != b);
    let Some((previous, corrected)) = edits.next() else {
        return false;
    };
    if edits.next().is_some() {
        return false;
    }
    if normalized(new) == normalized(source) || normalized(corrected) == normalized(source) {
        return true;
    }
    // Only permit a paraphrased base restoration when the value or qualifier
    // and its immediate lexical context actually occur in the authority.
    let prior_words = normalized(previous);
    let next_words = normalized(corrected);
    let source_words = normalized(source);
    let prefix = prior_words
        .iter()
        .zip(&next_words)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = prior_words[prefix..]
        .iter()
        .rev()
        .zip(next_words[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let restored = &next_words[prefix..next_words.len() - suffix];
    if !restored.iter().any(|word| {
        word.chars().any(|c| c.is_ascii_digit())
            || matches!(
                word.to_ascii_lowercase().as_str(),
                "only"
                    | "since"
                    | "before"
                    | "after"
                    | "unless"
                    | "except"
                    | "when"
                    | "if"
                    | "default"
                    | "true"
                    | "false"
                    | "on"
                    | "off"
            )
    }) {
        return false;
    }
    let context = &next_words
        [prefix.saturating_sub(1)..(next_words.len() - suffix + 1).min(next_words.len())];
    if !source_words
        .windows(context.len())
        .any(|words| words == context)
    {
        return false;
    }
    // A locally worded restoration needs both a matching base sentence and
    // independent corroboration of its claim, including qualifiers and values.
    base.iter().filter(|c| c.heading == heading).any(|c| {
        sentences(&c.text)
            .iter()
            .any(|s| normalized(s) == normalized(corrected))
    }) && rule_relation(corrected, source) == Relation::Entailment
}

// Models may quote an entire line as the preimage. Record only changed bytes
// so adjacent links, inline code, independent sentences and whitespace survive.
fn minimal_edit(start: usize, before: &str, after: &str) -> TextEdit {
    let prefix: usize = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let suffix: usize = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    TextEdit {
        start: start + prefix,
        end: start + before.len() - suffix,
        before: before[prefix..before.len() - suffix].into(),
        after: after[prefix..after.len() - suffix].into(),
    }
}

fn coordinates(c: &Citation) -> (&str, usize, usize) {
    (&c.path, c.first, c.last)
}

fn new_structure(before: &GovernanceAuditReport, after: &GovernanceAuditReport) -> bool {
    fn keys(report: &GovernanceAuditReport) -> BTreeMap<impl Ord + '_, usize> {
        let mut counts = BTreeMap::new();
        for i in &report.issues {
            let key = (
                &i.path,
                &i.code,
                i.severity,
                &i.subject,
                &i.message,
                i.location.as_ref().map(coordinates),
                i.evidence.as_ref().map(coordinates),
            );
            *counts.entry(key).or_default() += 1;
        }
        counts
    }
    let old = keys(before);
    keys(after)
        .iter()
        .any(|(key, count)| old.get(key).copied().unwrap_or_default() < *count)
}

fn rule_issues(p: &EvidenceBundle, text: &str) -> Result<Vec<Issue>, Rejection> {
    let mut scratch = p.clone();
    scratch.input.candidate = text.into();
    scratch.changed = changed_claims(&scratch.input).map_err(|_| "rules_unavailable")?;
    let verdicts = RuleConflictDetector
        .classify(&scratch)
        .map_err(|_| "rules_unavailable")?;
    interpret(&scratch, &verdicts).map_err(|_| "rules_unavailable")
}

fn same_issue(a: &Issue, b: &Issue) -> bool {
    a.code == b.code
        && coordinates(&a.location) == coordinates(&b.location)
        && a.evidence == b.evidence
}

struct Validation<'a> {
    packet: &'a EvidenceBundle,
    candidate_digest: &'a str,
    model: &'a [Issue],
    rules: &'a [Verdict],
    audit: Audit<'a>,
}

impl Validation<'_> {
    fn check(
        &self,
        r: &CitedRepair,
        current: &str,
    ) -> Result<(RepairProposal, String, Vec<Issue>), Rejection> {
        let p = self.packet;
        let c = p.changed.get(r.candidate).ok_or("outside_finding_span")?;
        let finding_source = p.evidence.get(r.evidence).ok_or("finding_citation")?;
        let issues: Vec<_> = self
            .model
            .iter()
            .filter(|i| {
                claim_fault(i)
                    && i.location == c.location
                    && i.evidence.as_ref() == Some(&finding_source.claim.location)
            })
            .cloned()
            .collect();
        if issues.is_empty() {
            return Err("no_matching_conflict");
        }
        if c.location.snapshot != self.candidate_digest
            || c.location.snapshot != digest(&p.input.candidate)
        {
            return Err("preimage_digest");
        }
        if c.location.path != p.input.path || c.location.first != c.location.last {
            return Err("outside_finding_span");
        }
        let (offset, line) =
            line_at(&p.input.candidate, c.location.first).ok_or("outside_finding_span")?;
        let (now_offset, now_line) =
            line_at(current, c.location.first).ok_or("outside_finding_span")?;
        if c.text != line || line != now_line {
            return Err("preimage_digest");
        }
        if r.before.is_empty() || r.before.contains(['\n', '\r']) || r.after.contains(['\n', '\r'])
        {
            return Err("outside_finding_span");
        }
        let at = line.find(&r.before).ok_or("preimage_span")?;
        if line.rfind(&r.before) != Some(at) {
            return Err("ambiguous_span");
        }
        let s = p.evidence.get(r.support).ok_or("support_citation")?;
        let meta = okf::parse_metadata(&p.input.candidate);
        if !s.human_verified
            || s.status != "active"
            || s.version != meta.version
            || (s.scope != "default" && meta.scope != "default" && s.scope != meta.scope)
        {
            return Err("inapplicable_source");
        }
        if r.support != r.evidence
            && !(finding_source.status == "retired"
                && (s.supersedes.as_deref() == Some(&finding_source.claim.location.path)
                    || self.model.iter().any(|i| {
                        i.code == "contradicts_verified"
                            && i.location == c.location
                            && i.evidence.as_ref() == Some(&s.claim.location)
                    })))
        {
            return Err("unrelated_source");
        }
        // Lexical silence is not a veto. An explicit opposing assessment from
        // the rules on this very citation still blocks combined-arm repairs.
        if self.rules.iter().any(|v| {
            v.candidate == r.candidate
                && v.evidence == r.evidence
                && (v.relation == Relation::ScopeDifference
                    || (finding_source.status == "active" && v.relation == Relation::Entailment)
                    || (finding_source.status == "retired"
                        && v.relation == Relation::Contradiction))
        }) {
            return Err("detector_disagreement");
        }
        let edit = minimal_edit(at, &r.before, &r.after);
        if edit.before == edit.after {
            return Err("no_change");
        }
        if edit
            .before
            .split_whitespace()
            .any(|w| matches!(w, "and" | "but" | "whereas"))
            || edit.before.contains(';')
            || edit.after.contains(';')
            || edit.after.contains('`')
            || code_ranges(line)
                .iter()
                .any(|range| range.start < edit.end && edit.start < range.end)
        {
            return Err("protected_span");
        }
        let mut repaired = line.to_owned();
        repaired.replace_range(edit.start..edit.end, &edit.after);
        let base = claims(&p.input.path, &p.input.base).map_err(|_| "preimage_digest")?;
        if !grounded(line, &repaired, &s.claim.text, &base, &c.heading) {
            return Err("unsupported_replacement");
        }
        let mut scratch = current.to_owned();
        scratch.replace_range(now_offset + edit.start..now_offset + edit.end, &edit.after);
        let before = rule_issues(p, current)?;
        for issue in rule_issues(p, &scratch)? {
            if !before.iter().any(|old| same_issue(&issue, old)) {
                return Err("new_rule_issue");
            }
            if issues.iter().any(|old| same_issue(&issue, old)) {
                return Err("conflict_persists");
            }
        }
        let a = (self.audit)(current).map_err(|_| "audit_unavailable")?;
        let b = (self.audit)(&scratch).map_err(|_| "audit_unavailable")?;
        if new_structure(&a, &b) {
            return Err("new_structural_issue");
        }
        let patch = FilePatch {
            path: p.input.path.clone(),
            preimage: c.location.snapshot.clone(),
            edits: vec![TextEdit {
                start: offset + edit.start,
                end: offset + edit.end,
                ..edit
            }],
        };
        patch
            .preview(&p.input.path, &p.input.candidate)
            .map_err(|_| "preimage_digest")?;
        Ok((
            RepairProposal {
                rationale: "Restore a cited claim after a scratch rules and Space audit.".into(),
                evidence: vec![s.claim.location.clone()],
                patch,
            },
            scratch,
            issues,
        ))
    }
}

pub(super) fn apply(
    p: &EvidenceBundle,
    response: &super::Response,
    model: &[Issue],
    rules: &[Verdict],
    disputed: &BTreeSet<Citation>,
    audit: Audit<'_>,
    result: &mut MaintenanceResult,
) {
    let validator = Validation {
        packet: p,
        candidate_digest: &response.candidate_digest,
        model,
        rules,
        audit,
    };
    let mut counts = BTreeMap::new();
    let mut attempts = Vec::new();
    let mut unknown_target = false;
    for raw in &response.answer.repairs {
        match raw {
            RepairAttachment::Proposal(r) => {
                *counts.entry(r.candidate).or_insert(0usize) += 1;
                attempts.push(("model", r.clone()));
            }
            RepairAttachment::Malformed(value) => {
                let index = |key: &str| value.get(key)?.as_u64()?.try_into().ok();
                let candidate = index("candidate");
                if let Some(n) = candidate {
                    *counts.entry(n).or_insert(0usize) += 1;
                } else {
                    unknown_target = true;
                }
                result.repair_checks.push(RepairCheck {
                    candidate,
                    evidence: index("evidence"),
                    support: index("support"),
                    origin: "model".into(),
                    outcome: "malformed_repair".into(),
                });
            }
        }
    }
    // Keep the existing narrow fallback only for claims the model did not try
    // to repair. A rejected attachment must remain unresolved.
    let eligible: Vec<_> = model
        .iter()
        .filter(|i| {
            !unknown_target
                && !disputed.contains(&i.location)
                && !counts
                    .keys()
                    .any(|&n| p.changed.get(n).is_some_and(|c| c.location == i.location))
        })
        .cloned()
        .collect();
    if let Some(rule) = propose_claim_repairs(p, &eligible) {
        for edit in rule.patch.edits {
            let line = 1 + p.input.candidate[..edit.start]
                .bytes()
                .filter(|&b| b == b'\n')
                .count();
            if let Some(candidate) = p.changed.iter().position(|c| c.location.first == line) {
                if let Some(evidence) = p.evidence.iter().position(|e| {
                    e.claim.text == edit.after
                        && eligible.iter().any(|i| {
                            i.location == p.changed[candidate].location
                                && i.evidence.as_ref() == Some(&e.claim.location)
                        })
                }) {
                    attempts.push((
                        "rule",
                        CitedRepair {
                            candidate,
                            evidence,
                            support: evidence,
                            before: edit.before,
                            after: edit.after,
                        },
                    ));
                }
            }
        }
    }
    attempts.sort_by_key(|(_, r)| (r.candidate, r.evidence, r.support));
    for (origin, r) in attempts {
        let decision = if counts.get(&r.candidate).copied().unwrap_or_default() > 1 {
            Err("competing_repairs")
        } else {
            validator.check(&r, &result.final_text)
        };
        let outcome = match decision {
            Ok((proposal, next, resolved)) => {
                result.unresolved.retain(|i| !resolved.contains(i));
                result.proposals.push(proposal);
                result.final_text = next;
                "accepted"
            }
            Err(reason) => reason,
        };
        result.repair_checks.push(RepairCheck {
            candidate: Some(r.candidate),
            evidence: Some(r.evidence),
            support: Some(r.support),
            origin: origin.into(),
            outcome: outcome.into(),
        });
    }
}
