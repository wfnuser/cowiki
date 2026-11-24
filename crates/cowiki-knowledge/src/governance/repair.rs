use super::{changed_claims, digest, interpret, Citation, ConflictDetector, EvidenceBundle, Issue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub before: String,
    pub after: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilePatch {
    pub path: String,
    pub preimage: String,
    pub edits: Vec<TextEdit>,
}

impl FilePatch {
    pub fn preview(&self, path: &str, text: &str) -> Result<String, String> {
        if self.path != path || self.preimage != digest(text) {
            return Err("proposal preimage changed".into());
        }
        let mut cursor = 0;
        let mut result = String::new();
        for edit in &self.edits {
            let prefix = text
                .get(cursor..edit.start)
                .ok_or("overlapping or invalid UTF-8 edits")?;
            if text.get(edit.start..edit.end) != Some(edit.before.as_str()) {
                return Err("edit preimage mismatch".into());
            }
            result.push_str(prefix);
            result.push_str(&edit.after);
            cursor = edit.end;
        }
        result.push_str(&text[cursor..]);
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairProposal {
    pub rationale: String,
    pub evidence: Vec<Citation>,
    pub patch: FilePatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceResult {
    pub initial: Vec<Issue>,
    pub proposals: Vec<RepairProposal>,
    pub unresolved: Vec<Issue>,
    pub final_text: String,
    pub rounds: usize,
}

pub fn propose_claim_repairs(packet: &EvidenceBundle, issues: &[Issue]) -> Option<RepairProposal> {
    let text = &packet.input.candidate;
    let mut replacements: BTreeMap<usize, BTreeMap<&str, Citation>> = BTreeMap::new();
    for issue in issues.iter().filter(|i| i.code == "contradicts_verified") {
        let evidence = packet
            .evidence
            .iter()
            .find(|e| Some(&e.claim.location) == issue.evidence.as_ref())?;
        let claim = packet
            .changed
            .iter()
            .find(|c| c.location == issue.location)?;
        if evidence.human_verified && evidence.status == "active" {
            replacements
                .entry(claim.location.first)
                .or_default()
                .insert(&evidence.claim.text, evidence.claim.location.clone());
        }
    }
    let mut offset = 0;
    let mut edits = Vec::new();
    let mut sources = BTreeSet::new();
    for (idx, line) in text.split_inclusive('\n').enumerate() {
        if let Some(options) = replacements.get(&(idx + 1)) {
            if options.len() == 1 {
                let (&supported, citation) = options.first_key_value()?;
                let original = line.trim();
                if super::conflict::safe_to_replace_line(original, supported)
                    && original != supported
                {
                    let start = offset + (line.len() - line.trim_start().len());
                    edits.push(TextEdit {
                        start,
                        end: start + original.len(),
                        before: original.into(),
                        after: supported.into(),
                    });
                    sources.insert(citation.clone());
                }
            }
        }
        offset += line.len();
    }
    if edits.is_empty() {
        return None;
    }
    Some(RepairProposal {
        rationale: "Restore the conflicting claim from the cited human-verified evidence.".into(),
        evidence: sources.into_iter().collect(),
        patch: FilePatch {
            path: packet.input.path.clone(),
            preimage: digest(text),
            edits,
        },
    })
}

/// All transitions are in memory. Retain a bounded number of repair attempts,
/// and report the verdict on the resulting scratch text.
pub fn maintain(
    bundle: &EvidenceBundle,
    detector: &dyn ConflictDetector,
) -> Result<MaintenanceResult, String> {
    let mut scratch = bundle.clone();
    let mut issues = interpret(&scratch, &detector.classify(&scratch)?)?;
    let initial = issues.clone();
    let mut proposals = Vec::new();
    let mut seen = BTreeSet::from([digest(&scratch.input.candidate)]);
    while proposals.len() < 2 {
        let Some(proposal) = propose_claim_repairs(&scratch, &issues) else {
            break;
        };
        let next = proposal
            .patch
            .preview(&scratch.input.path, &scratch.input.candidate)?;
        if !seen.insert(digest(&next)) {
            break;
        }
        scratch.input.candidate = next;
        scratch.changed = changed_claims(&scratch.input)?;
        proposals.push(proposal);
        issues = interpret(&scratch, &detector.classify(&scratch)?)?;
    }
    Ok(MaintenanceResult {
        initial,
        rounds: proposals.len(),
        proposals,
        unresolved: issues,
        final_text: scratch.input.candidate,
    })
}
