//! Method execution is deliberately independent of the gold-label scorer.
use cowiki_knowledge::governance::{
    maintain, protocol::Usage, EvidenceBundle, MaintenanceResult, RuleConflictDetector,
};
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    None,
    Rules,
    SinglePass,
    Remote,
    Both,
}
impl std::str::FromStr for Mode {
    type Err = String;
    fn from_str(name: &str) -> Result<Self, String> {
        match name {
            "none" => Ok(Self::None),
            "rules" => Ok(Self::Rules),
            "single-pass" => Ok(Self::SinglePass),
            "remote" => Ok(Self::Remote),
            "both" => Ok(Self::Both),
            _ => Err(format!("unknown verifier {name}")),
        }
    }
}
impl Mode {
    pub fn uses_model(self) -> bool {
        matches!(self, Self::SinglePass | Self::Remote | Self::Both)
    }
}
#[derive(Default)]
pub struct Runner {
    #[cfg(feature = "remote-verifier")]
    pub provider: Option<cowiki_knowledge::governance::remote::RemoteVerifier>,
}
#[derive(Default, Serialize)]
pub struct Outcome {
    pub maintenance: Option<MaintenanceResult>,
    pub usage: Usage,
    pub cache_key: Option<String>,
    pub error: Option<String>,
    pub verifier_ns: u64,
    pub repair_and_check_ns: u64,
}
impl Runner {
    pub fn run(&self, mode: Mode, packet: &EvidenceBundle) -> Outcome {
        self.run_with_audit(mode, packet, &|_| Err("scratch audit unavailable".into()))
    }

    pub fn run_with_audit(
        &self,
        mode: Mode,
        packet: &EvidenceBundle,
        _audit: &dyn Fn(
            &str,
        )
            -> Result<cowiki_knowledge::governance::GovernanceAuditReport, String>,
    ) -> Outcome {
        let start = Instant::now();
        let mut outcome = Outcome::default();
        let result = match mode {
            Mode::None => Ok(None),
            Mode::Rules => maintain(packet, &RuleConflictDetector).map(Some),
            #[cfg(feature = "remote-verifier")]
            _ => (|| {
                use cowiki_knowledge::governance::remote::*;
                let provider = self
                    .provider
                    .as_ref()
                    .ok_or("missing verifier configuration")?;
                let task = if mode == Mode::SinglePass {
                    Task::SinglePass
                } else {
                    Task::Verify
                };
                let mut attempt = Attempt::default();
                let response = provider.request_accounted(packet, task, &mut attempt);
                outcome.verifier_ns = start.elapsed().as_nanos() as u64;
                outcome.cache_key = attempt.key;
                outcome.usage = attempt.usage;
                let response = response?;
                outcome.usage = response.usage.clone();
                if mode == Mode::SinglePass {
                    single_pass(packet, &response).map(Some)
                } else {
                    verified_maintenance_with_audit(packet, &response, mode == Mode::Both, _audit)
                        .map(Some)
                }
            })(),
            #[cfg(not(feature = "remote-verifier"))]
            _ => Err("remote mode requires --features remote-verifier".into()),
        };
        outcome.repair_and_check_ns =
            (start.elapsed().as_nanos() as u64).saturating_sub(outcome.verifier_ns);
        match result {
            Ok(result) => outcome.maintenance = result,
            Err(reason) => outcome.error = Some(reason),
        }
        outcome
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Cost {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub logical_calls: u64,
    pub http_calls: u64,
    pub estimated_cases: u64,
    pub cache_hits: u64,
    pub retrieval_calls: usize,
    pub retrieval_tokens: usize,
    pub lookup_ns: u64,
    pub verifier_ns: u64,
    pub repair_and_check_ns: u64,
    pub elapsed_ns: u64,
    pub failures: usize,
    pub unresolved: usize,
}
impl Cost {
    pub fn observe(
        &mut self,
        packet: &EvidenceBundle,
        out: &Outcome,
        lookup_ns: u64,
        elapsed_ns: u64,
    ) {
        let usage = &out.usage;
        self.prompt_tokens += usage.prompt_tokens;
        self.completion_tokens += usage.completion_tokens;
        self.total_tokens += usage.prompt_tokens + usage.completion_tokens;
        self.logical_calls += usage.calls;
        self.http_calls += usage.http_calls;
        self.cache_hits += usage.cache_hits;
        self.estimated_cases += u64::from(usage.estimated);
        self.retrieval_calls += packet.retrieval_calls;
        self.retrieval_tokens += packet.retrieval_tokens;
        self.lookup_ns += lookup_ns;
        self.verifier_ns += out.verifier_ns;
        self.repair_and_check_ns += out.repair_and_check_ns;
        self.elapsed_ns += elapsed_ns;
        self.failures += usize::from(out.error.is_some());
        self.unresolved += out.maintenance.as_ref().map_or(0, |r| r.unresolved.len());
    }
}

/// Apply deterministic destination spelling only after a detector selected a
/// particular candidate occurrence and source stub. File names enforce path
/// safety. Every redirect field used for content comes from the common packet.
pub fn repair_redirects(
    packet: &EvidenceBundle,
    paths: Vec<String>,
    result: &mut MaintenanceResult,
    approved: &std::collections::BTreeSet<(usize, String)>,
) -> Result<(), String> {
    use cowiki_knowledge::{
        governance::{redirect_patch, Citation, RedirectFields},
        knowledge_index::{self as index, FileIndex, LinkKind},
    };
    if approved.is_empty() || result.final_text != packet.input.candidate {
        return Ok(());
    }
    let files = FileIndex::new(paths);
    let fields: RedirectFields = packet
        .links
        .values()
        .filter_map(|r| {
            let (line, raw) = r.redirect.as_ref()?;
            let path = r.target.as_ref()?;
            Some((
                path.clone(),
                (
                    raw.clone(),
                    Citation {
                        path: path.clone(),
                        snapshot: r.snapshot.clone()?,
                        first: *line,
                        last: *line,
                    },
                ),
            ))
        })
        .collect();
    let Some(proposal) = redirect_patch(
        &packet.input.path,
        &result.final_text,
        &files,
        &fields,
        Some(approved),
    ) else {
        return Ok(());
    };
    let next = proposal
        .patch
        .preview(&packet.input.path, &result.final_text)?;
    let (_, body) = cowiki_knowledge::okf::split_frontmatter(&next)?;
    let prefix = next.len() - body.len();
    let remaining: std::collections::BTreeSet<_> =
        index::link_occurrences(body, &packet.input.path)
            .into_iter()
            .filter_map(|link| {
                let line = 1 + next[..prefix + link.range.start]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count();
                let path = match link.kind {
                    LinkKind::Markdown => link.target,
                    LinkKind::Wiki => files.resolve_wikilink(&link.target, &packet.input.path)?,
                };
                Some((line, path))
            })
            .collect();
    result.unresolved.retain(|i| {
        let Some(e) = &i.evidence else { return true };
        let key = (i.location.first, e.path.clone());
        i.code != "inbound_redirect" || !approved.contains(&key) || remaining.contains(&key)
    });
    result.final_text = next;
    result.proposals.push(proposal);
    result.rounds += 1;
    Ok(())
}
