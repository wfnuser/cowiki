//! Read-only governance of immutable Markdown snapshots.
use crate::{embeddings::EmbeddingProvider, knowledge_index, okf};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Citation {
    pub path: String,
    /// SHA-256 of the original Markdown, including frontmatter.
    pub snapshot: String,
    pub first: usize,
    pub last: usize,
}

impl std::fmt::Display for Citation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:L{}-L{}]", self.path, self.first, self.last)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claim {
    pub location: Citation,
    pub text: String,
    pub heading: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub claim: Claim,
    pub status: String,
    pub human_verified: bool,
    pub scope: String,
    pub version: String,
    pub sources: Vec<String>,
    pub updated_at: Option<String>,
    pub supersedes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revision {
    pub id: String,
    pub path: String,
    pub text: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceInput {
    pub path: String,
    pub request: String,
    pub base: String,
    pub candidate: String,
    pub history: Vec<Revision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBundle {
    pub input: GovernanceInput,
    pub as_of: String,
    pub changed: Vec<Claim>,
    pub evidence: Vec<Evidence>,
    pub retrieval_calls: usize,
    pub retrieval_tokens: usize,
    #[serde(default)]
    pub links: BTreeMap<String, LinkRecord>,
}

pub fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn claims(path: &str, text: &str) -> Result<Vec<Claim>, String> {
    let (_, body) = okf::split_frontmatter(text)?;
    let prefix = text.len() - body.len();
    let first = 1 + text[..prefix].bytes().filter(|&c| c == b'\n').count();
    let code = knowledge_index::code_ranges(body);
    let snapshot = digest(text);
    let mut heading = String::new();
    let mut claims = Vec::new();
    let mut offset = 0;
    for (i, raw) in body.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += raw.len();
        let value = raw.trim();
        if value.is_empty()
            || code.iter().any(|r| {
                r.start <= start + raw.len() - raw.trim_start().len()
                    && r.end >= start + raw.trim_end().len()
            })
        {
            continue;
        }
        if value.starts_with('#') {
            heading = value.trim_start_matches('#').trim().to_owned();
        } else {
            claims.push(Claim {
                location: Citation {
                    path: path.into(),
                    snapshot: snapshot.clone(),
                    first: first + i,
                    last: first + i,
                },
                text: value.into(),
                heading: heading.clone(),
            });
        }
    }
    Ok(claims)
}

fn withdrawal_notice(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let predicate = ["this rule was ", "this policy was ", "this claim was "]
        .iter()
        .find_map(|prefix| lower.strip_prefix(prefix))
        .unwrap_or(&lower);
    let end = predicate
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(predicate.len());
    let (verb, rest) = predicate.split_at(end);
    matches!(
        verb,
        "retired" | "withdrawn" | "superseded" | "retracted" | "replaced"
    ) && (rest.is_empty()
        || rest.starts_with(['.', ':', ';', '—'])
        || matches!(
            rest.split_whitespace().next(),
            Some("by" | "on" | "in" | "at" | "as" | "with" | "effective" | "because")
        ))
}

/// Keep the assertion text separate for classification. On a retired page, a
/// directly attached withdrawal notice also belongs in the reviewer's citation.
fn source_claims(path: &str, text: &str, retired: bool) -> Result<Vec<Claim>, String> {
    let mut result = claims(path, text)?;
    if retired {
        let lines: Vec<_> = text.lines().collect();
        for i in 1..result.len() {
            let (prior, rest) = result.split_at_mut(i);
            let previous = &mut prior[i - 1];
            let next = &rest[0];
            if withdrawal_notice(&next.text)
                && previous.heading == next.heading
                && next.location.first <= previous.location.last + 2
                && lines[previous.location.last..next.location.first - 1]
                    .iter()
                    .all(|s| s.trim().is_empty())
            {
                previous.location.last = next.location.last;
            }
        }
    }
    Ok(result)
}

pub fn changed_claims(input: &GovernanceInput) -> Result<Vec<Claim>, String> {
    let before = okf::parse_metadata(&input.base);
    let after = okf::parse_metadata(&input.candidate);
    let mut old = BTreeMap::new();
    if (before.scope, before.version) == (after.scope, after.version) {
        for claim in claims(&input.path, &input.base)? {
            *old.entry((claim.heading, claim.text)).or_insert(0usize) += 1;
        }
    }
    let mut changed = Vec::new();
    for claim in claims(&input.path, &input.candidate)? {
        let count = old
            .entry((claim.heading.clone(), claim.text.clone()))
            .or_default();
        if *count == 0 {
            changed.push(claim);
        } else {
            *count -= 1;
        }
    }
    Ok(changed)
}

pub trait EvidenceLookup {
    fn locate(&self, input: GovernanceInput, as_of: &str) -> Result<EvidenceBundle, String>;
}

/// The caller supplies the base Space and its matching derived index. A
/// candidate may name an existing base page, but is never indexed as evidence.
pub struct SnapshotIndex<'a> {
    pub connection: &'a Connection,
    pub space_id: &'a str,
    pub root: &'a Path,
    pub embedder: &'a dyn EmbeddingProvider,
}

impl EvidenceLookup for SnapshotIndex<'_> {
    fn locate(&self, input: GovernanceInput, as_of: &str) -> Result<EvidenceBundle, String> {
        if !okf::valid_iso_date(as_of) {
            return Err("an explicit ISO as-of date is required".into());
        }
        let changed = changed_claims(&input)?;
        let query = changed
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let mut evidence = BTreeMap::new();
        let mut calls = 0;
        let mut tokens = 0;
        if !query.is_empty() {
            for retired in [false, true] {
                let options = knowledge_index::HybridSearchOptions {
                    token_budget: Some(if retired { 400 } else { 800 }),
                    exclude_retired: Some(!retired),
                    follow_redirects: Some(!retired),
                    use_graph_expansion: Some(!retired),
                    as_of_date: Some(as_of.into()),
                    ..Default::default()
                };
                let result = knowledge_index::hybrid_search_subset(
                    self.connection,
                    self.space_id,
                    self.root,
                    &query,
                    &options,
                    self.embedder,
                    retired,
                )?;
                calls += 1;
                tokens += result.tokens_used;
                for item in result.evidence {
                    let (_, path) = okf::safe_resolve_under_space(self.root, &item.page_path)?;
                    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
                    let meta = okf::parse_metadata(&text);
                    let supersedes = okf::split_frontmatter(&text)?
                        .0
                        .and_then(|raw| serde_yaml::from_str::<serde_yaml::Mapping>(raw).ok())
                        .and_then(|map| map.get("supersedes")?.as_str().map(str::to_owned));
                    let (first, last) = item
                        .citation
                        .rsplit_once("#L")
                        .and_then(|(_, s)| s.split_once("-L"))
                        .and_then(|(a, b)| {
                            Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))
                        })
                        .ok_or("invalid hybrid section citation")?;
                    if meta.is_retired() != retired {
                        return Err(format!("stale evidence index for {}", item.page_path));
                    }
                    for claim in source_claims(&item.page_path, &text, retired)? {
                        let at = claim.location.first;
                        if at < first
                            || at > last
                            || !item.text.lines().any(|line| line.trim() == claim.text)
                        {
                            continue;
                        }
                        evidence.insert(
                            claim.location.clone(),
                            Evidence {
                                claim,
                                status: meta.status.clone(),
                                human_verified: meta.is_human_verified()
                                    && !meta.generated
                                    && meta.trust_tier != "agent_draft",
                                scope: meta.scope.clone(),
                                version: meta.version.clone(),
                                sources: meta.sources.clone(),
                                updated_at: meta.updated_at.clone(),
                                supersedes: supersedes.clone(),
                            },
                        );
                    }
                }
            }
        }
        let links = link_inventory(self.root, &input)?;
        Ok(EvidenceBundle {
            links,
            input,
            as_of: as_of.into(),
            changed,
            evidence: evidence.into_values().collect(),
            retrieval_calls: calls,
            retrieval_tokens: tokens,
        })
    }
}

mod alignment;
mod conflict;
pub use conflict::*;
mod repair;
pub use repair::*;
#[cfg(feature = "remote-verifier")]
pub mod remote;

mod audit;
pub mod protocol;
pub use audit::*;
mod staging;
pub use staging::*;
mod redirects;
pub use redirects::*;
