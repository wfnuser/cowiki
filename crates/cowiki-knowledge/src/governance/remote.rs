//! One bounded OpenAI-compatible chat call. Only this feature can issue HTTP.
use super::{digest, ConflictDetector, EvidenceBundle, FilePatch, TextEdit, Verdict};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    env, fs,
    io::{Read, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

const BYTE_LIMIT: u64 = 512 * 1024;
use super::protocol::{admission, prompt, OUTPUT_LIMIT, TOKEN_LIMIT};
pub use super::protocol::{model_input, Task, Usage};
mod cited_repair;
pub use cited_repair::{CitedRepair, RepairAttachment};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    Record,
    Replay,
}

/// Provider accounting can include session overhead. The evidence packet stays
/// identical across arms regardless of these accounting and transport limits.
#[derive(Debug, Clone, Copy)]
pub struct RequestLimits {
    pub total_tokens: u64,
    pub output_tokens: u64,
    pub timeout_secs: u64,
}

impl Default for RequestLimits {
    fn default() -> Self {
        Self {
            total_tokens: TOKEN_LIMIT,
            output_tokens: OUTPUT_LIMIT,
            timeout_secs: 30,
        }
    }
}

impl RequestLimits {
    fn from_env() -> Result<Self, String> {
        fn setting(name: &str, default: u64, max: u64) -> Result<u64, String> {
            match env::var(name) {
                Err(env::VarError::NotPresent) => Ok(default),
                Ok(s) => s
                    .parse::<u64>()
                    .ok()
                    .filter(|v| (1..=max).contains(v))
                    .ok_or_else(|| format!("{name} must be between 1 and {max}")),
                Err(_) => Err(format!("invalid {name}")),
            }
        }
        let limits = Self {
            total_tokens: setting("COWIKI_VERIFIER_TOTAL_TOKENS", TOKEN_LIMIT, 1_000_000)?,
            output_tokens: setting("COWIKI_VERIFIER_OUTPUT_TOKENS", OUTPUT_LIMIT, 128_000)?,
            timeout_secs: setting("COWIKI_VERIFIER_TIMEOUT_SECS", 30, 600)?,
        };
        if limits.output_tokens >= limits.total_tokens {
            return Err("output budget must be smaller than total budget".into());
        }
        Ok(limits)
    }
}

#[derive(Clone)]
pub struct RemoteVerifier {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub cache_dir: PathBuf,
    pub mode: CacheMode,
    pub limits: RequestLimits,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFinding {
    pub candidate: usize,
    pub evidence: usize,
    pub code: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEdit {
    pub candidate: usize,
    pub before: String,
    pub after: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralFinding {
    pub code: String,
    pub line: usize,
    pub source: String,
    pub source_line: usize,
}

fn structural_findings(
    p: &EvidenceBundle,
    answer: &ModelAnswer,
) -> Result<Vec<super::Issue>, String> {
    use super::{Citation, Issue, Severity};
    let n = p.input.candidate.lines().count();
    let snapshot = digest(&p.input.candidate);
    answer
        .structural
        .iter()
        .map(|f| {
            if !matches!(
                f.code.as_str(),
                "broken_link"
                    | "inbound_redirect"
                    | "broken_redirect"
                    | "redirect_cycle"
                    | "missing_generated_sources"
                    | "expired_stale_after"
            ) || f.line == 0
                || f.line > n
                || f.source_line == 0
            {
                return Err("invalid structural model finding".into());
            }
            let proof = if f.source == p.input.path && f.source_line <= n {
                Some(&snapshot)
            } else {
                p.links
                    .values()
                    .find(|r| {
                        r.target.as_deref() == Some(&f.source)
                            && r.redirect
                                .as_ref()
                                .is_some_and(|(line, _)| *line == f.source_line)
                    })
                    .and_then(|r| r.snapshot.as_ref())
            }
            .ok_or("structural evidence outside packet")?;
            Ok(Issue {
                code: f.code.clone(),
                severity: Severity::Warning,
                message: "Model structural finding.".into(),
                location: Citation {
                    path: p.input.path.clone(),
                    snapshot: snapshot.clone(),
                    first: f.line,
                    last: f.line,
                },
                evidence: Some(Citation {
                    path: f.source.clone(),
                    snapshot: proof.clone(),
                    first: f.source_line,
                    last: f.source_line,
                }),
            })
        })
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAnswer {
    #[serde(default)]
    pub structural: Vec<StructuralFinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repairs: Vec<RepairAttachment>,
    pub judgments: Vec<Verdict>,
    pub findings: Vec<ModelFinding>,
    pub edits: Vec<ModelEdit>,
}
#[derive(Debug, Serialize, Deserialize)]
struct Record {
    key: String,
    model: String,
    fingerprint: Option<String>,
    prompt_tokens: u64,
    completion_tokens: u64,
    estimated: bool,
    latency_ms: u64,
    answer: ModelAnswer,
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Cached {
    Success(Record),
    Failure {
        key: String,
        reason: String,
        usage: Usage,
    },
}

#[derive(Default, Debug)]
pub struct Attempt {
    pub key: Option<String>,
    pub usage: Usage,
}

pub struct Response {
    pub candidate_digest: String,
    pub answer: ModelAnswer,
    pub usage: Usage,
    pub key: String,
}

/// Exact wire instructions and response format. The evaluation freeze hashes
/// these values before running a new corpus.
pub fn request_contract(task: Task) -> Result<(String, Value), String> {
    let mut system = prompt(task);
    let format = if task == Task::Verify {
        system.push_str(" For a new contradiction or regression, propose at most one minimal repair per changed claim. Link candidate/evidence to the judgment and support to the applicable active verified source. before must occur exactly once within the candidate line. Copy the restoring qualifier, value, or complete sentence verbatim from the cited support line into evidence_quote. after must contain that exact quote and restore the whole assertion, including scope, conditions, and polarity. Prefer the source's own wording for the corrected assertion. Preserve unrelated content. Omit the repair if a minimal quoted correction is unavailable.");
        let schema: Value = serde_json::from_str(include_str!("verifier-reply.schema.json"))
            .map_err(|e| e.to_string())?;
        json!({"type":"json_schema","json_schema":{"name":"m2_quoted_repair_v2","strict":true,"schema":schema}})
    } else {
        json!({"type":"json_object"})
    };
    Ok((system, format))
}

/// A paused provider is an interrupted experiment. Caching a quota error as a
/// terminal answer would preclude continuing the same request after reset.
pub const PROVIDER_PAUSED: &str = "verifier provider quota paused";

impl RemoteVerifier {
    pub fn from_env(mode: CacheMode) -> Result<Self, String> {
        let required = |name: &str| {
            env::var(name)
                .ok()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| format!("missing {name}"))
        };
        Ok(Self {
            endpoint: required("COWIKI_VERIFIER_URL")?,
            model: required("COWIKI_VERIFIER_MODEL")?,
            api_key: env::var("COWIKI_VERIFIER_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            cache_dir: PathBuf::from(required("COWIKI_VERIFIER_CACHE_DIR")?),
            mode,
            limits: RequestLimits::from_env()?,
        })
    }
    pub fn request(&self, packet: &EvidenceBundle, task: Task) -> Result<Response, String> {
        self.request_accounted(packet, task, &mut Attempt::default())
    }
    pub fn request_accounted(
        &self,
        packet: &EvidenceBundle,
        task: Task,
        attempt: &mut Attempt,
    ) -> Result<Response, String> {
        *attempt = Attempt::default();
        let user = model_input(packet).to_string();
        let (system, format) = request_contract(task)?;
        // Keep the paired content admission identical to single-pass. The
        // provider's measured total, including the reply schema and harness,
        // is checked before any judgment or proposal can be consumed.
        let reserved = admission(packet);
        if reserved + OUTPUT_LIMIT > TOKEN_LIMIT
            || reserved + self.limits.output_tokens > self.limits.total_tokens
        {
            return Err("packet exceeds equal per-case token budget before dispatch".into());
        }
        let request = json!({"model":self.model,"temperature":0,"max_tokens":self.limits.output_tokens,"stream":false,
            "response_format":format,"messages":[{"role":"system","content":system},{"role":"user","content":user}]});
        let key = digest(&json!(["cowiki-governance/1", &self.endpoint, &request]).to_string());
        let file = self.cache_dir.join(format!("{key}.json"));
        attempt.key = Some(key.clone());
        let (cached, hit) = match fs::read(&file) {
            Ok(bytes) => (
                serde_json::from_slice::<Cached>(&bytes)
                    .map_err(|e| format!("invalid verifier cache: {e}"))?,
                true,
            ),
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound && self.mode == CacheMode::Record =>
            {
                fs::create_dir_all(&self.cache_dir).map_err(|e| e.to_string())?;
                let started = Instant::now();
                // Keep the actual provider response, including over-budget answers.
                // Replays reapply the caller's acceptance budget without a new call.
                let record = match self.fetch(request, &key, reserved, &mut attempt.usage) {
                    Ok(rec) => Cached::Success(rec),
                    Err(reason) if reason == PROVIDER_PAUSED => return Err(reason),
                    Err(reason) => {
                        attempt.usage.original_latency_ms = started.elapsed().as_millis() as u64;
                        Cached::Failure {
                            key: key.clone(),
                            reason,
                            usage: attempt.usage.clone(),
                        }
                    }
                };
                let mut tmp =
                    tempfile::NamedTempFile::new_in(&self.cache_dir).map_err(|e| e.to_string())?;
                serde_json::to_writer(&mut tmp, &record).map_err(|e| e.to_string())?;
                tmp.flush().map_err(|e| e.to_string())?;
                match tmp.persist_noclobber(&file) {
                    Ok(_)=>(record,false),
                    Err(e) if e.error.kind()==std::io::ErrorKind::AlreadyExists=>return Err("concurrent verifier recording for the same request; replay the recorded run".into()),
                    Err(e)=>return Err(e.to_string()),
                }
            }
            Err(e) => return Err(format!("verifier cache miss or read failure ({key}): {e}")),
        };
        let record = match cached {
            Cached::Success(record) => record,
            Cached::Failure {
                key: stored,
                reason,
                mut usage,
            } => {
                if key != stored {
                    return Err("verifier failure journal identity mismatch".into());
                }
                if hit {
                    usage.http_calls = 0;
                    usage.cache_hits = 1;
                }
                attempt.usage = usage;
                return Err(reason);
            }
        };
        attempt.usage = Usage {
            calls: 1,
            http_calls: u64::from(!hit),
            cache_hits: u64::from(hit),
            prompt_tokens: record.prompt_tokens,
            completion_tokens: record.completion_tokens,
            estimated: record.estimated,
            original_latency_ms: record.latency_ms,
        };
        validate(&record, packet, task, &key, self.limits)?;
        Ok(Response {
            candidate_digest: digest(&packet.input.candidate),
            key,
            answer: record.answer,
            usage: attempt.usage.clone(),
        })
    }

    fn fetch(
        &self,
        request: Value,
        key: &str,
        reserved: u64,
        usage: &mut Usage,
    ) -> Result<Record, String> {
        let start = Instant::now();
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(self.limits.timeout_secs))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        let mut call = client.post(&self.endpoint).json(&request);
        if let Some(token) = &self.api_key {
            call = call.bearer_auth(token);
        }
        usage.calls = 1;
        usage.http_calls = 1;
        // Until a response supplies usage, reserve the full unknown cost.
        usage.estimated = true;
        usage.prompt_tokens = reserved;
        usage.completion_tokens = self.limits.output_tokens;
        let http = call
            .send()
            .map_err(|e| format!("verifier transport failure: {e}"))?;
        if http.status().as_u16() == 429 {
            return Err(PROVIDER_PAUSED.into());
        }
        if !http.status().is_success() {
            return Err(format!("verifier HTTP {}", http.status()));
        }
        let mut bytes = Vec::new();
        http.take(BYTE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > BYTE_LIMIT {
            return Err("verifier response too large".into());
        }
        let parsed: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let input = parsed["usage"]["prompt_tokens"].as_u64();
        let output = parsed["usage"]["completion_tokens"].as_u64();
        usage.prompt_tokens = input.unwrap_or(reserved);
        usage.completion_tokens = output.unwrap_or(self.limits.output_tokens);
        usage.estimated = input.is_none() || output.is_none();
        let choice = &parsed["choices"][0];
        if choice["finish_reason"] == "length" {
            return Err("truncated verifier response".into());
        }
        let content = choice["message"]["content"]
            .as_str()
            .ok_or("missing chat message content")?;
        let answer =
            serde_json::from_str(content).map_err(|e| format!("invalid verifier JSON: {e}"))?;
        Ok(Record {
            key: key.into(),
            model: parsed["model"].as_str().unwrap_or(&self.model).into(),
            fingerprint: parsed["system_fingerprint"].as_str().map(str::to_owned),
            prompt_tokens: input.unwrap_or(reserved),
            completion_tokens: output.unwrap_or(self.limits.output_tokens),
            estimated: input.is_none() || output.is_none(),
            latency_ms: start.elapsed().as_millis() as u64,
            answer,
        })
    }
}
fn validate(
    record: &Record,
    p: &EvidenceBundle,
    task: Task,
    key: &str,
    limits: RequestLimits,
) -> Result<(), String> {
    if key != record.key
        || record.completion_tokens > limits.output_tokens
        || record
            .prompt_tokens
            .saturating_add(record.completion_tokens)
            > limits.total_tokens
    {
        return Err("verifier identity or budget mismatch".into());
    }
    let a = &record.answer;
    let valid = |c: usize, e: usize| c < p.changed.len() && e < p.evidence.len();
    let mut pairs = BTreeSet::new();
    for v in &a.judgments {
        if !valid(v.candidate, v.evidence) || !pairs.insert((v.candidate, v.evidence)) {
            return Err("invalid or duplicate judgment citation".into());
        }
    }
    if task == Task::Verify
        && (pairs.len() != p.changed.len() * p.evidence.len()
            || !a.findings.is_empty()
            || !a.edits.is_empty())
    {
        return Err("incomplete or invalid verifier judgments".into());
    }
    if task == Task::SinglePass && (!pairs.is_empty() || !a.repairs.is_empty()) {
        return Err("unexpected single-pass judgments".into());
    }
    for f in &a.findings {
        if !valid(f.candidate, f.evidence)
            || !matches!(
                f.code.as_str(),
                "contradicts_verified"
                    | "reintroduces_retired"
                    | "revision_regression"
                    | "duplicate_overlap"
            )
        {
            return Err("invalid model finding".into());
        }
    }
    structural_findings(p, a)?;
    model_patch(p, &a.edits)?;
    Ok(())
}
pub fn model_patch(p: &EvidenceBundle, edits: &[ModelEdit]) -> Result<FilePatch, String> {
    let mut items = BTreeSet::new();
    let mut spans = Vec::new();
    for e in edits {
        let c = p
            .changed
            .get(e.candidate)
            .ok_or("unknown model edit citation")?;
        if !items.insert(e.candidate)
            || c.text != e.before
            || c.text.contains('`')
            || e.after.contains('`')
            || e.after.is_empty()
            || e.after.contains(['\r', '\n'])
        {
            return Err("invalid line edit".into());
        }
        let mut lines = p.input.candidate.split_inclusive('\n');
        let offset = lines
            .by_ref()
            .take(c.location.first - 1)
            .map(str::len)
            .sum::<usize>();
        let line = lines.next().ok_or("line missing")?;
        let start = offset + line.len() - line.trim_start().len();
        spans.push(TextEdit {
            start,
            end: start + e.before.len(),
            before: e.before.clone(),
            after: e.after.clone(),
        });
    }
    spans.sort_by_key(|e| e.start);
    let patch = FilePatch {
        path: p.input.path.clone(),
        preimage: digest(&p.input.candidate),
        edits: spans,
    };
    patch.preview(&p.input.path, &p.input.candidate)?;
    Ok(patch)
}
impl ConflictDetector for RemoteVerifier {
    fn allows_scratch_recheck(&self) -> bool {
        false
    }
    fn classify(&self, bundle: &EvidenceBundle) -> Result<Vec<Verdict>, String> {
        Ok(self.request(bundle, Task::Verify)?.answer.judgments)
    }
}

/// The comparison arm accepts the model's cited findings and its one patch.
/// It deliberately does not invoke the M2 classifier or semantic repair loop.
pub fn single_pass(
    packet: &EvidenceBundle,
    response: &Response,
) -> Result<super::MaintenanceResult, String> {
    use super::{Issue, MaintenanceResult, RepairProposal, Severity};
    let mut findings: Vec<_> = response
        .answer
        .findings
        .iter()
        .map(|f| {
            Ok(Issue {
                severity: if f.code == "duplicate_overlap" {
                    Severity::Info
                } else {
                    Severity::Warning
                },
                code: f.code.clone(),
                location: packet
                    .changed
                    .get(f.candidate)
                    .ok_or("unknown finding line")?
                    .location
                    .clone(),
                evidence: Some(
                    packet
                        .evidence
                        .get(f.evidence)
                        .ok_or("unknown finding evidence")?
                        .claim
                        .location
                        .clone(),
                ),
                message: "Single-pass model finding.".into(),
            })
        })
        .collect::<Result<_, String>>()?;
    findings.extend(structural_findings(packet, &response.answer)?);
    let patch = model_patch(packet, &response.answer.edits)?;
    let final_text = patch.preview(&packet.input.path, &packet.input.candidate)?;
    let proposals = if patch.edits.is_empty() {
        vec![]
    } else {
        vec![RepairProposal {
            rationale: "Single-pass model suggestion.".into(),
            evidence: findings.iter().filter_map(|i| i.evidence.clone()).collect(),
            patch,
        }]
    };
    // There is no second verifier call. Leave semantic verification to the
    // independent scorer/reviewer rather than equating a model edit with success.
    Ok(MaintenanceResult {
        repair_checks: Vec::new(),
        rounds: proposals.len(),
        unresolved: findings.clone(),
        initial: findings,
        proposals,
        final_text,
    })
}

/// Apply a single batch of model judgments. An independent positive from either
/// detector reaches the review queue. Without a scratch Space audit, semantic
/// edits remain unresolved. Use `verified_maintenance_with_audit` for repairs.
pub fn verified_maintenance(
    packet: &EvidenceBundle,
    response: &Response,
    combine: bool,
) -> Result<super::MaintenanceResult, String> {
    verified_maintenance_with_audit(packet, response, combine, &|_| {
        Err("scratch audit unavailable".into())
    })
}

/// Validate each attachment against immutable cited evidence and full scratch Space audits.
pub fn verified_maintenance_with_audit(
    packet: &EvidenceBundle,
    response: &Response,
    combine: bool,
    audit: &dyn Fn(&str) -> Result<super::GovernanceAuditReport, String>,
) -> Result<super::MaintenanceResult, String> {
    use super::{interpret, MaintenanceResult, RuleConflictDetector};
    let mut model = interpret(packet, &response.answer.judgments)?;
    model.extend(structural_findings(packet, &response.answer)?);
    let mut initial: BTreeSet<_> = model.iter().cloned().collect();
    let mut disputed = BTreeSet::new();
    let mut rules = Vec::new();
    if combine {
        rules = RuleConflictDetector.classify(packet)?;
        let rule_issues = interpret(packet, &rules)?.into_iter().collect();
        for issue in initial.symmetric_difference(&rule_issues) {
            disputed.insert(issue.location.clone());
        }
        initial.extend(rule_issues);
    }
    let mut result = MaintenanceResult {
        initial: initial.into_iter().collect(),
        unresolved: Vec::new(),
        proposals: Vec::new(),
        final_text: packet.input.candidate.clone(),
        repair_checks: Vec::new(),
        rounds: 0,
    };
    result.unresolved = result.initial.clone();
    cited_repair::apply(
        packet,
        response,
        &model,
        &rules,
        &disputed,
        audit,
        &mut result,
    );
    for i in &mut result.unresolved {
        if disputed.contains(&i.location) {
            i.message.push_str(" Detectors disagree; review required.");
        }
    }
    result.rounds = usize::from(!result.proposals.is_empty());
    Ok(result)
}
