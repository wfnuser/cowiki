//! Pluggable post-retrieval evidence scoring for multi-hop chain assembly.
//!
//! Callers can run `knowledge_index::hybrid_search_with_scorer` with either the
//! built-in zero-dependency `MarginalNoveltyScorer` or a custom cross-encoder /
//! evidence-necessity scorer that evaluates each candidate conditioned on the
//! query and already-selected shared evidence.

use std::collections::HashSet;

/// Query plus already-selected seed evidence supplied to an `EvidenceScorer`.
#[derive(Debug, Clone)]
pub struct ScoringContext<'a> {
    pub query: &'a str,
    pub shared_evidence: &'a [String],
}

/// Candidate passage submitted for scoring.
#[derive(Debug, Clone)]
pub struct CandidatePassage<'a> {
    pub unit_id: &'a str,
    pub page_title: &'a str,
    pub section_heading: &'a str,
    pub body: &'a str,
}

/// Pluggable scorer returning normalized `[0.0, 1.0]` relevance/necessity scores
/// aligned 1-to-1 with `candidates`.
pub trait EvidenceScorer: Send + Sync {
    fn scorer_name(&self) -> &str;

    fn score_candidates(
        &self,
        ctx: &ScoringContext<'_>,
        candidates: &[CandidatePassage<'_>],
    ) -> Result<Vec<f64>, String>;
}

/// Zero-dependency deterministic scorer combining direct query coverage with
/// marginal novel-term coverage relative to `shared_evidence`.
///
/// Rewards candidates that supply *uncovered* query terms or bridge-adjacent
/// terminology rather than duplicating terms already captured by earlier seeds.
#[derive(Debug, Clone, Default)]
pub struct MarginalNoveltyScorer;

impl EvidenceScorer for MarginalNoveltyScorer {
    fn scorer_name(&self) -> &str {
        "local-marginal-novelty-v1"
    }

    fn score_candidates(
        &self,
        ctx: &ScoringContext<'_>,
        candidates: &[CandidatePassage<'_>],
    ) -> Result<Vec<f64>, String> {
        let query_terms = extract_terms(ctx.query);
        let mut shared_terms: HashSet<String> = HashSet::new();
        for ev in ctx.shared_evidence {
            shared_terms.extend(extract_terms(ev));
        }
        let uncovered_query: HashSet<String> = query_terms
            .iter()
            .filter(|t| !shared_terms.contains(*t))
            .cloned()
            .collect();

        let mut scores = Vec::with_capacity(candidates.len());
        for cand in candidates {
            let text = format!("{} {} {}", cand.page_title, cand.section_heading, cand.body);
            let cand_terms = extract_terms(&text);
            if cand_terms.is_empty() {
                scores.push(0.0);
                continue;
            }
            let direct_cov = if query_terms.is_empty() {
                0.0
            } else {
                query_terms.intersection(&cand_terms).count() as f64 / query_terms.len() as f64
            };
            let uncovered_cov = if uncovered_query.is_empty() {
                0.0
            } else {
                uncovered_query.intersection(&cand_terms).count() as f64
                    / uncovered_query.len() as f64
            };
            let novel_ratio = if shared_terms.is_empty() {
                1.0
            } else {
                let novel = cand_terms.difference(&shared_terms).count() as f64;
                (novel / cand_terms.len() as f64).clamp(0.0, 1.0)
            };
            let score =
                (0.45 * direct_cov + 0.35 * uncovered_cov + 0.20 * novel_ratio).clamp(0.0, 1.0);
            scores.push(score);
        }
        Ok(scores)
    }
}

fn extract_terms(text: &str) -> HashSet<String> {
    const STOPS: &[&str] = &[
        "the", "and", "for", "with", "that", "this", "from", "are", "was", "were", "have", "has",
        "had", "not", "but", "how", "what", "who", "when", "where", "which", "does", "did", "can",
        "will", "into", "about", "after", "before", "under", "over", "between",
    ];
    text.split(|c: char| !c.is_alphanumeric())
        .filter_map(|tok| {
            let low = tok.to_ascii_lowercase();
            if low.len() >= 3 && !STOPS.contains(&low.as_str()) {
                Some(low)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marginal_novelty_scorer_prefers_candidate_filling_uncovered_hop() {
        let scorer = MarginalNoveltyScorer;
        let shared = vec!["Billing Service sends posted invoices to Ledger Pipeline.".to_string()];
        let ctx = ScoringContext {
            query:
                "How long does Billing Service retain financial ledger journals in WORM storage?",
            shared_evidence: &shared,
        };
        let cands = vec![
            CandidatePassage {
                unit_id: "u-dup",
                page_title: "Billing Service",
                section_heading: "Overview",
                body: "Billing Service sends posted invoices to Ledger Pipeline every day.",
            },
            CandidatePassage {
                unit_id: "u-hop2",
                page_title: "Retention Policy",
                section_heading: "Financial Records",
                body: "Financial ledger journals remain immutable in WORM storage for seven years.",
            },
        ];
        let scores = scorer.score_candidates(&ctx, &cands).unwrap();
        assert_eq!(scores.len(), 2);
        assert!(
            scores[1] > scores[0],
            "expected uncovered hop candidate ({}) to outscore duplicate seed ({})",
            scores[1],
            scores[0]
        );
    }
}
