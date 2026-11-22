mod corpus;

use cowiki_knowledge::embeddings::{
    EmbeddingProvider, LocalHashProjectionEmbedder, OpenAiCompatibleEmbedder,
};
use cowiki_knowledge::knowledge_index::{self, HybridSearchOptions};
use cowiki_knowledge::okf;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

const BENCH_AS_OF_DATE: &str = "2026-11-22";

#[derive(Debug, Clone)]
struct CliArgs {
    out_path: Option<PathBuf>,
    embedder_mode: String,
    dataset_path: Option<PathBuf>,
    scale_pages: Vec<usize>,
    threads: Vec<usize>,
    queries_per_thread: usize,
    check_thresholds: bool,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            out_path: None,
            embedder_mode: "local".to_string(),
            dataset_path: None,
            scale_pages: vec![100, 500, 1000],
            threads: vec![1, 4, 8],
            queries_per_thread: 40,
            check_thresholds: true,
        }
    }
}

fn parse_args() -> Result<CliArgs, String> {
    let mut args = CliArgs::default();
    let raw: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--out" => {
                i += 1;
                let value = raw.get(i).ok_or("--out requires a file path")?;
                args.out_path = Some(PathBuf::from(value));
            }
            "--embedder" => {
                i += 1;
                let value = raw
                    .get(i)
                    .ok_or("--embedder requires 'local' or 'remote'")?;
                if value != "local" && value != "remote" {
                    return Err(format!("invalid --embedder value '{value}'"));
                }
                args.embedder_mode = value.clone();
            }
            "--dataset" => {
                i += 1;
                let value = raw.get(i).ok_or("--dataset requires a JSONL file path")?;
                args.dataset_path = Some(PathBuf::from(value));
            }
            "--scale-pages" => {
                i += 1;
                let value = raw
                    .get(i)
                    .ok_or("--scale-pages requires a comma-separated list, e.g. 100,500,1000")?;
                args.scale_pages = parse_usize_csv(value, "--scale-pages")?;
            }
            "--threads" => {
                i += 1;
                let value = raw
                    .get(i)
                    .ok_or("--threads requires a comma-separated list, e.g. 1,4,8")?;
                args.threads = parse_usize_csv(value, "--threads")?;
            }
            "--queries-per-thread" => {
                i += 1;
                let value = raw
                    .get(i)
                    .ok_or("--queries-per-thread requires an integer")?;
                args.queries_per_thread = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --queries-per-thread value '{value}'"))?;
            }
            "--no-check-thresholds" => {
                args.check_thresholds = false;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: cowiki-retrieval-bench [OPTIONS]\n\n\
                     Options:\n\
                       --out <PATH>                  Write full JSON benchmark report to PATH\n\
                       --embedder <local|remote>     Embedding backend (default: local)\n\
                       --dataset <PATH>              Optional external multi-hop JSONL dataset\n\
                       --scale-pages <CSV>           Corpus sizes for scaling suite (default: 100,500,1000)\n\
                       --threads <CSV>               Concurrency levels for read QPS (default: 1,4,8)\n\
                       --queries-per-thread <N>      Queries per worker thread (default: 40)\n\
                       --no-check-thresholds         Do not fail on invariant assertions"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }
    Ok(args)
}

fn parse_usize_csv(value: &str, flag: &str) -> Result<Vec<usize>, String> {
    let parsed: Result<Vec<usize>, _> = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            item.parse::<usize>()
                .map_err(|_| format!("invalid integer '{item}' for {flag}"))
        })
        .collect();
    let list = parsed?;
    if list.is_empty() {
        return Err(format!("{flag} must contain at least one positive integer"));
    }
    Ok(list)
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    benchmark_version: String,
    generated_at_utc: String,
    okf_version: String,
    index_schema_version: i64,
    embedder_mode: String,
    embedding_model: String,
    embedding_dim: usize,
    retrieval_ablations: Vec<AblationBudgetReport>,
    governance_correctness: GovernanceReport,
    scaling_and_incremental: Vec<ScalePointReport>,
    concurrent_reads: Vec<ConcurrencyPointReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_dataset: Option<ExternalDatasetReport>,
    invariants_passed: bool,
    invariant_failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AblationBudgetReport {
    token_budget: usize,
    query_count: usize,
    configs: Vec<AblationConfigMetrics>,
}

#[derive(Debug, Serialize)]
struct CategoryMetrics {
    query_count: usize,
    page_recall: f64,
    full_multihop_coverage: f64,
    mrr: f64,
}

#[derive(Debug, Serialize)]
struct AblationConfigMetrics {
    config: String,
    page_recall: f64,
    full_multihop_coverage: f64,
    mrr: f64,
    avg_tokens_used: f64,
    budget_violations: usize,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    by_category: BTreeMap<String, CategoryMetrics>,
}

#[derive(Debug, Serialize)]
struct GovernanceReport {
    retired_queries_tested: usize,
    retired_leakage_rate: f64,
    redirect_cases_tested: usize,
    redirect_resolution_rate: f64,
    fresh_vs_stale_pairs_tested: usize,
    fresh_over_stale_win_rate: f64,
    trust_reset_cases_tested: usize,
    trust_reset_pass_rate: f64,
    validator_expected_issues: usize,
    validator_detected_issues: usize,
    validator_precision: f64,
    validator_recall: f64,
}

#[derive(Debug, Serialize)]
struct ScalePointReport {
    page_count: usize,
    indexed_pages: usize,
    knowledge_units: usize,
    page_links: usize,
    unit_links: usize,
    cached_embeddings: usize,
    rebuild_ms: f64,
    warm_refresh_ms: f64,
    db_size_kb: f64,
    incremental_add_ms: f64,
    incremental_edit_ms: f64,
    incremental_delete_ms: f64,
    incremental_vs_full_equivalent: bool,
    query_p50_ms: f64,
    query_p95_ms: f64,
}

#[derive(Debug, Serialize)]
struct ConcurrencyPointReport {
    corpus_pages: usize,
    threads: usize,
    total_queries: usize,
    wall_ms: f64,
    qps: f64,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    errors: usize,
}

#[derive(Debug, Deserialize)]
struct ExternalParagraph {
    title: String,
    text: String,
    #[serde(default)]
    links: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExternalQuestion {
    #[serde(default)]
    id: String,
    question: String,
    supporting_titles: Vec<String>,
    paragraphs: Vec<ExternalParagraph>,
}

#[derive(Debug, Serialize)]
struct ExternalDatasetReport {
    dataset_path: String,
    questions_evaluated: usize,
    token_budget: usize,
    configs: Vec<AblationConfigMetrics>,
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted.len().saturating_sub(1) as f64)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn open_wal_db(db_path: &Path) -> Result<Connection, String> {
    let mut conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )
    .map_err(|e| e.to_string())?;
    knowledge_index::initialize(&mut conn)?;
    Ok(conn)
}

fn build_ablation_configs(budget: usize) -> Vec<(&'static str, HybridSearchOptions)> {
    vec![
        (
            "baseline_fts5",
            HybridSearchOptions {
                token_budget: Some(budget),
                max_hops: Some(0),
                seed_top_k: Some(6),
                include_backlinks: Some(false),
                exclude_retired: Some(false),
                follow_redirects: Some(false),
                use_okf_governance: Some(false),
                use_lexical: Some(true),
                use_vector: Some(false),
                use_graph_expansion: Some(false),
                scope_filter: None,
                as_of_date: Some(BENCH_AS_OF_DATE.to_string()),
            },
        ),
        (
            "hybrid_full",
            HybridSearchOptions {
                token_budget: Some(budget),
                max_hops: Some(2),
                seed_top_k: Some(6),
                include_backlinks: Some(true),
                exclude_retired: Some(true),
                follow_redirects: Some(true),
                use_okf_governance: Some(true),
                use_lexical: Some(true),
                use_vector: Some(true),
                use_graph_expansion: Some(true),
                scope_filter: None,
                as_of_date: Some(BENCH_AS_OF_DATE.to_string()),
            },
        ),
        (
            "ablation_no_graph",
            HybridSearchOptions {
                token_budget: Some(budget),
                max_hops: Some(0),
                seed_top_k: Some(6),
                include_backlinks: Some(false),
                exclude_retired: Some(true),
                follow_redirects: Some(true),
                use_okf_governance: Some(true),
                use_lexical: Some(true),
                use_vector: Some(true),
                use_graph_expansion: Some(false),
                scope_filter: None,
                as_of_date: Some(BENCH_AS_OF_DATE.to_string()),
            },
        ),
        (
            "ablation_no_vector",
            HybridSearchOptions {
                token_budget: Some(budget),
                max_hops: Some(2),
                seed_top_k: Some(6),
                include_backlinks: Some(true),
                exclude_retired: Some(true),
                follow_redirects: Some(true),
                use_okf_governance: Some(true),
                use_lexical: Some(true),
                use_vector: Some(false),
                use_graph_expansion: Some(true),
                scope_filter: None,
                as_of_date: Some(BENCH_AS_OF_DATE.to_string()),
            },
        ),
        (
            "ablation_no_bm25",
            HybridSearchOptions {
                token_budget: Some(budget),
                max_hops: Some(2),
                seed_top_k: Some(6),
                include_backlinks: Some(true),
                exclude_retired: Some(true),
                follow_redirects: Some(true),
                use_okf_governance: Some(true),
                use_lexical: Some(false),
                use_vector: Some(true),
                use_graph_expansion: Some(true),
                scope_filter: None,
                as_of_date: Some(BENCH_AS_OF_DATE.to_string()),
            },
        ),
    ]
}

fn run_retrieval_ablations(
    embedder: &dyn EmbeddingProvider,
) -> Result<Vec<AblationBudgetReport>, String> {
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let space_root = temp.path().join("space");
    let db_path = temp.path().join("index.sqlite");
    fs::create_dir_all(&space_root).map_err(|e| e.to_string())?;
    corpus::write_eval_corpus(&space_root)?;

    let mut conn = open_wal_db(&db_path)?;
    knowledge_index::rebuild_space_with_embedder(&mut conn, "eval", &space_root, embedder)?;
    knowledge_index::rebuild_links(&mut conn, "eval")?;

    let mut budget_reports = Vec::new();
    for budget in [400usize, 800usize] {
        let mut config_metrics = Vec::new();
        for (config_name, opts) in build_ablation_configs(budget) {
            let mut recalls = Vec::new();
            let mut full_hits = 0usize;
            let mut mrrs = Vec::new();
            let mut tokens_total = 0usize;
            let mut budget_violations = 0usize;
            let mut latencies_ms = Vec::new();
            let mut per_cat: BTreeMap<String, (Vec<f64>, usize, Vec<f64>)> = BTreeMap::new();

            for eval_q in corpus::EVAL_QUERIES {
                debug_assert!(!eval_q.id.is_empty());
                let start = Instant::now();
                let res = knowledge_index::hybrid_search_with_embedder(
                    &conn,
                    "eval",
                    &space_root,
                    eval_q.query,
                    &opts,
                    embedder,
                )?;
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                latencies_ms.push(elapsed_ms);
                tokens_total += res.tokens_used;
                if res.tokens_used > budget {
                    budget_violations += 1;
                }

                let retrieved_ordered: Vec<&str> = res
                    .evidence
                    .iter()
                    .map(|item| item.page_path.as_str())
                    .collect();
                let retrieved_set: HashSet<&str> = retrieved_ordered.iter().copied().collect();
                let gold_set: HashSet<&str> = eval_q.supporting_pages.iter().copied().collect();

                let matched = gold_set
                    .iter()
                    .filter(|page| retrieved_set.contains(*page))
                    .count();
                let recall = if gold_set.is_empty() {
                    1.0
                } else {
                    matched as f64 / gold_set.len() as f64
                };
                let full_hit = matched == gold_set.len();
                let rr = retrieved_ordered
                    .iter()
                    .enumerate()
                    .find(|(_, page)| gold_set.contains(**page))
                    .map(|(idx, _)| 1.0 / (idx as f64 + 1.0))
                    .unwrap_or(0.0);

                recalls.push(recall);
                if full_hit {
                    full_hits += 1;
                }
                mrrs.push(rr);

                let entry = per_cat
                    .entry(eval_q.category.to_string())
                    .or_insert_with(|| (Vec::new(), 0, Vec::new()));
                entry.0.push(recall);
                if full_hit {
                    entry.1 += 1;
                }
                entry.2.push(rr);
            }

            latencies_ms.sort_by(|a, b| a.total_cmp(b));
            let n = corpus::EVAL_QUERIES.len() as f64;
            let by_category = per_cat
                .into_iter()
                .map(|(cat, (cat_recalls, cat_full, cat_mrrs))| {
                    let cn = cat_recalls.len() as f64;
                    (
                        cat,
                        CategoryMetrics {
                            query_count: cat_recalls.len(),
                            page_recall: round4(cat_recalls.iter().sum::<f64>() / cn),
                            full_multihop_coverage: round4(cat_full as f64 / cn),
                            mrr: round4(cat_mrrs.iter().sum::<f64>() / cn),
                        },
                    )
                })
                .collect();

            config_metrics.push(AblationConfigMetrics {
                config: config_name.to_string(),
                page_recall: round4(recalls.iter().sum::<f64>() / n),
                full_multihop_coverage: round4(full_hits as f64 / n),
                mrr: round4(mrrs.iter().sum::<f64>() / n),
                avg_tokens_used: round2(tokens_total as f64 / n),
                budget_violations,
                p50_latency_ms: round2(percentile(&latencies_ms, 50.0)),
                p95_latency_ms: round2(percentile(&latencies_ms, 95.0)),
                by_category,
            });
        }

        budget_reports.push(AblationBudgetReport {
            token_budget: budget,
            query_count: corpus::EVAL_QUERIES.len(),
            configs: config_metrics,
        });
    }

    Ok(budget_reports)
}

fn run_governance_correctness(
    embedder: &dyn EmbeddingProvider,
) -> Result<GovernanceReport, String> {
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let space_root = temp.path().join("gov-space");
    let db_path = temp.path().join("gov.sqlite");
    fs::create_dir_all(&space_root).map_err(|e| e.to_string())?;

    fs::write(space_root.join("index.md"), okf::root_index()).map_err(|e| e.to_string())?;

    // 1. Retired pages (6 distinct retired docs across topics)
    let retired_topics = [
        (
            "retired-tls10.md",
            "TLS 1.0 Cipher Suite",
            "rc4_legacy_cipher_suite_retired",
        ),
        (
            "retired-xmlrpc.md",
            "XML-RPC Bridge",
            "xmlrpc_soap_gateway_retired",
        ),
        (
            "retired-md5-auth.md",
            "MD5 Password Hash",
            "md5_unsalted_password_retired",
        ),
        (
            "retired-ftp.md",
            "FTP Dropzone",
            "plaintext_ftp_ingest_retired",
        ),
        (
            "retired-v0-api.md",
            "Legacy v0 API",
            "legacy_v0_unauthenticated_api_retired",
        ),
        (
            "retired-cron.md",
            "Legacy Cron Runner",
            "single_node_crontab_scheduler_retired",
        ),
    ];
    for (file, title, sentinel) in &retired_topics {
        fs::write(
            space_root.join(file),
            format!(
                "---\ntype: Policy\ntitle: \"{title}\"\nstatus: retired\n---\n\n# {title}\n\nRetired directive {sentinel}.\n"
            ),
        )
        .map_err(|e| e.to_string())?;
    }

    // 2. Redirect chains (1-hop, 2-hop, frontmatter-only stub)
    fs::write(
        space_root.join("canon-deploy.md"),
        "---\ntype: Guide\ntitle: Canonical Deployment\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Canonical Deployment\n\nBlue-green verified pipeline.\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        space_root.join("redir-1hop.md"),
        "---\ntype: Guide\ntitle: Old Deploy Alpha\nstatus: deprecated\nredirect_to: canon-deploy.md\n---\n\n# Old Deploy Alpha\n\nDeprecated alpha deploy sentinel.\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        space_root.join("redir-2hop-start.md"),
        "---\ntype: Guide\ntitle: Ancient Deploy Entry\nstatus: deprecated\nredirect_to: redir-1hop.md\n---\n\n# Ancient Deploy Entry\n\nAncient deploy hop sentinel.\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        space_root.join("redir-stub-only.md"),
        "---\ntype: Guide\ntitle: Minimal Stub Deploy\nredirect_to: canon-deploy.md\n---\n",
    )
    .map_err(|e| e.to_string())?;

    // 3. Fresh verified vs stale/deprecated pairs
    let pairs = [
        (
            "sla-fresh.md",
            "sla-stale.md",
            "Service SLA Targets",
            "sla_target_9999_sentinel",
        ),
        (
            "oncall-fresh.md",
            "oncall-stale.md",
            "On-Call Escalation",
            "oncall_pager_escalation_sentinel",
        ),
        (
            "key-fresh.md",
            "key-stale.md",
            "SSH Key Policy",
            "ssh_ed25519_hardware_sentinel",
        ),
    ];
    for (fresh, stale, title, sentinel) in &pairs {
        fs::write(
            space_root.join(fresh),
            format!(
                "---\ntype: Policy\ntitle: \"{title}\"\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-01-01\"\n---\n\n# {title}\n\nCurrent authoritative guidance for {sentinel}.\n"
            ),
        )
        .map_err(|e| e.to_string())?;
        fs::write(
            space_root.join(stale),
            format!(
                "---\ntype: Policy\ntitle: \"{title} (Old)\"\nstatus: deprecated\ntrust_tier: agent_draft\nverified: false\ngenerated: true\nstale_after: \"2024-01-01\"\n---\n\n# {title} (Old)\n\nOutdated guidance for {sentinel}.\n"
            ),
        )
        .map_err(|e| e.to_string())?;
    }

    let mut conn = open_wal_db(&db_path)?;
    knowledge_index::rebuild_space_with_embedder(&mut conn, "gov", &space_root, embedder)?;
    knowledge_index::rebuild_links(&mut conn, "gov")?;

    let gov_opts = HybridSearchOptions {
        token_budget: Some(800),
        max_hops: Some(1),
        seed_top_k: Some(6),
        include_backlinks: Some(true),
        exclude_retired: Some(true),
        follow_redirects: Some(true),
        use_okf_governance: Some(true),
        use_lexical: Some(true),
        use_vector: Some(true),
        use_graph_expansion: Some(true),
        scope_filter: None,
        as_of_date: Some(BENCH_AS_OF_DATE.to_string()),
    };

    // Check retired leakage across both hybrid_search and lexical search
    let mut retired_leaks = 0usize;
    for (file, _, sentinel) in &retired_topics {
        let res = knowledge_index::hybrid_search_with_embedder(
            &conn,
            "gov",
            &space_root,
            sentinel,
            &gov_opts,
            embedder,
        )?;
        let leaked_hybrid = res.evidence.iter().any(|e| e.page_path == *file);
        let lex_hits = knowledge_index::search(&conn, "gov", sentinel, 10)?;
        let leaked_lex = lex_hits.iter().any(|h| h.path == *file);
        if leaked_hybrid || leaked_lex {
            retired_leaks += 1;
        }
    }

    // Check redirect resolution (1-hop, 2-hop, frontmatter-only stub)
    let redirect_queries = [
        ("Deprecated alpha deploy sentinel", "canon-deploy.md"),
        ("Ancient deploy hop sentinel", "canon-deploy.md"),
        ("Minimal Stub Deploy", "canon-deploy.md"),
    ];
    let mut redirect_resolved = 0usize;
    for (q, expected_target) in &redirect_queries {
        let res = knowledge_index::hybrid_search_with_embedder(
            &conn,
            "gov",
            &space_root,
            q,
            &gov_opts,
            embedder,
        )?;
        if res
            .evidence
            .iter()
            .any(|item| item.page_path == *expected_target)
        {
            redirect_resolved += 1;
        }
    }

    // Check fresh human_verified outranks stale/deprecated
    let mut fresh_wins = 0usize;
    for (fresh, stale, _, sentinel) in &pairs {
        let res = knowledge_index::hybrid_search_with_embedder(
            &conn,
            "gov",
            &space_root,
            sentinel,
            &gov_opts,
            embedder,
        )?;
        let fresh_idx = res.evidence.iter().position(|e| e.page_path == *fresh);
        let stale_idx = res.evidence.iter().position(|e| e.page_path == *stale);
        if let Some(f_pos) = fresh_idx {
            if stale_idx.is_none_or(|s_pos| f_pos < s_pos) {
                fresh_wins += 1;
            }
        }
    }

    // Check Agent-Edit Trust Reset invariance across 3 human-verified docs with custom YAML
    let trust_docs = [
        "---\ntype: Policy\ntitle: Doc A\nverified: true\ntrust_tier: human_verified\nowner: secops\n---\n\n# Doc A\n\nBody A.\n",
        "---\ntype: Architecture\ntitle: Doc B\nverified: true\ntrust_tier: human_verified\ngenerated: false\npriority: 1\n---\n\n# Doc B\n\nBody B.\n",
        "---\ntype: Guide\ntitle: Doc C\nverified: true\ntrust_tier: human_verified\nsources:\n  - .cowiki/sources/spec.md\n---\n\n# Doc C\n\nBody C.\n",
    ];
    let mut trust_passes = 0usize;
    for (idx, doc) in trust_docs.iter().enumerate() {
        let updated = okf::apply_agent_edit_trust_reset(
            doc,
            &format!("Doc {idx}"),
            &[".cowiki/sources/evidence.md".to_string()],
            Some(BENCH_AS_OF_DATE),
        )?;
        let meta = okf::parse_metadata(&updated);
        let preserves_custom = match idx {
            0 => updated.contains("owner: secops"),
            1 => updated.contains("priority: 1"),
            _ => meta.sources.iter().any(|s| s == ".cowiki/sources/spec.md"),
        };
        if !meta.verified
            && meta.trust_tier == "agent_draft"
            && meta.generated
            && meta.updated_at.as_deref() == Some(BENCH_AS_OF_DATE)
            && meta
                .sources
                .iter()
                .any(|s| s == ".cowiki/sources/evidence.md")
            && preserves_custom
        {
            trust_passes += 1;
        }
    }

    // Check validate_bundle precision/recall on seeded OKF v0.3 defects
    let val_temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let val_root = val_temp.path();
    fs::write(val_root.join("index.md"), okf::root_index()).map_err(|e| e.to_string())?;
    fs::write(
        val_root.join("clean.md"),
        "---\ntype: Note\ntitle: Clean\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-01-01\"\n---\n\n# Clean\n\nAll good.\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        val_root.join("bad-trust.md"),
        "---\ntype: Note\ntitle: Bad Trust\ntrust_tier: agent_draft\nverified: true\ngenerated: true\n---\n\n# Bad Trust\n\nBody.\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        val_root.join("expired.md"),
        "---\ntype: Note\ntitle: Expired\nstale_after: \"2024-01-01\"\n---\n\n# Expired\n\nOld.\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        val_root.join("broken-redir.md"),
        "---\ntype: Note\ntitle: Broken Redir\nredirect_to: missing.md\n---\n\n# Broken Redir\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        val_root.join("cycle-a.md"),
        "---\ntype: Note\ntitle: Cycle A\nredirect_to: cycle-b.md\n---\n\n# Cycle A\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        val_root.join("cycle-b.md"),
        "---\ntype: Note\ntitle: Cycle B\nredirect_to: cycle-a.md\n---\n\n# Cycle B\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        val_root.join("broken-link.md"),
        "---\ntype: Note\ntitle: Broken Link\n---\n\n# Broken Link\n\nSee [Ghost](ghost.md).\n",
    )
    .map_err(|e| e.to_string())?;

    let val_report = okf::validate_bundle(val_root, Some(BENCH_AS_OF_DATE), true)?;
    let expected_pairs: HashSet<(&str, &str)> = [
        ("bad-trust.md", "trust_reset_violation"),
        ("expired.md", "expired_stale_after"),
        ("broken-redir.md", "broken_redirect"),
        ("cycle-a.md", "redirect_cycle"),
        ("cycle-b.md", "redirect_cycle"),
        ("broken-link.md", "broken_link"),
    ]
    .into_iter()
    .collect();
    let detected_pairs: HashSet<(String, String)> = val_report
        .issues
        .iter()
        .map(|issue| (issue.path.clone(), issue.code.clone()))
        .collect();
    let true_positives = detected_pairs
        .iter()
        .filter(|(path, code)| expected_pairs.contains(&(path.as_str(), code.as_str())))
        .count();
    let precision = if detected_pairs.is_empty() {
        0.0
    } else {
        true_positives as f64 / detected_pairs.len() as f64
    };
    let recall = true_positives as f64 / expected_pairs.len() as f64;

    Ok(GovernanceReport {
        retired_queries_tested: retired_topics.len(),
        retired_leakage_rate: round4(retired_leaks as f64 / retired_topics.len() as f64),
        redirect_cases_tested: redirect_queries.len(),
        redirect_resolution_rate: round4(redirect_resolved as f64 / redirect_queries.len() as f64),
        fresh_vs_stale_pairs_tested: pairs.len(),
        fresh_over_stale_win_rate: round4(fresh_wins as f64 / pairs.len() as f64),
        trust_reset_cases_tested: trust_docs.len(),
        trust_reset_pass_rate: round4(trust_passes as f64 / trust_docs.len() as f64),
        validator_expected_issues: expected_pairs.len(),
        validator_detected_issues: detected_pairs.len(),
        validator_precision: round4(precision),
        validator_recall: round4(recall),
    })
}

fn run_scaling_and_incremental(
    embedder: &dyn EmbeddingProvider,
    scale_pages: &[usize],
) -> Result<Vec<ScalePointReport>, String> {
    let mut reports = Vec::new();
    let query_opts = HybridSearchOptions {
        token_budget: Some(800),
        max_hops: Some(2),
        seed_top_k: Some(6),
        include_backlinks: Some(true),
        exclude_retired: Some(true),
        follow_redirects: Some(true),
        use_okf_governance: Some(true),
        use_lexical: Some(true),
        use_vector: Some(true),
        use_graph_expansion: Some(true),
        scope_filter: None,
        as_of_date: Some(BENCH_AS_OF_DATE.to_string()),
    };

    for &page_count in scale_pages {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let space_root = temp.path().join("space");
        let db_path = temp.path().join("scale.sqlite");
        fs::create_dir_all(&space_root).map_err(|e| e.to_string())?;
        let paths = corpus::write_synthetic_space(&space_root, page_count)?;

        let mut conn = open_wal_db(&db_path)?;
        let rebuild_start = Instant::now();
        knowledge_index::rebuild_space_with_embedder(&mut conn, "scale", &space_root, embedder)?;
        knowledge_index::rebuild_links(&mut conn, "scale")?;
        let rebuild_ms = rebuild_start.elapsed().as_secs_f64() * 1000.0;

        let refresh_start = Instant::now();
        knowledge_index::refresh_space_with_embedder(&mut conn, "scale", &space_root, embedder)?;
        let warm_refresh_ms = refresh_start.elapsed().as_secs_f64() * 1000.0;

        // Flush WAL to report accurate disk footprint
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        let db_size_kb = fs::metadata(&db_path).map_err(|e| e.to_string())?.len() as f64 / 1024.0;
        let stats = knowledge_index::index_stats(&conn, "scale")?;

        // Measure query P50 / P95 on the built index
        let sample_queries = [
            "billing deterministic checkpointing backpressure",
            "security sequence numbers SLA threshold",
            "storage workflows operational runbook throttle",
            "sync cross-domain policies worker shard",
            "search module 0012 latency queue depth",
            "runtime module 0045 backpressure flag",
            "telemetry SLA threshold operational runbook",
            "deployment module 0007 consistency model",
            "module 0021 downstream execution",
            "module 0003 backpressure throttle",
        ];
        let mut latencies_ms = Vec::with_capacity(sample_queries.len());
        for q in &sample_queries {
            let q_start = Instant::now();
            let _ = knowledge_index::hybrid_search_with_embedder(
                &conn,
                "scale",
                &space_root,
                q,
                &query_opts,
                embedder,
            )?;
            latencies_ms.push(q_start.elapsed().as_secs_f64() * 1000.0);
        }
        latencies_ms.sort_by(|a, b| a.total_cmp(b));

        // Incremental ADD
        let added_rel = PathBuf::from("billing/incremental-added.md");
        fs::write(
            space_root.join(&added_rel),
            "---\ntype: Architecture\ntitle: \"Incremental Billing Sentinel\"\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Incremental Billing Sentinel\n\n## Delta Update\n\nNewly added sentinel connects to [Module 0001](../security/doc-0001.md).\n",
        )
        .map_err(|e| e.to_string())?;
        let add_start = Instant::now();
        knowledge_index::index_paths_with_embedder(
            &mut conn,
            "scale",
            &space_root,
            std::slice::from_ref(&added_rel),
            embedder,
        )?;
        let incremental_add_ms = add_start.elapsed().as_secs_f64() * 1000.0;

        // Incremental EDIT
        let edited_rel = PathBuf::from(&paths[0]);
        fs::write(
            space_root.join(&edited_rel),
            "---\ntype: Architecture\ntitle: \"Edited Root Module 0000\"\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Edited Root Module 0000\n\n## Updated Contract\n\nRevised incremental edit sentinel linked to [Incremental Billing Sentinel](incremental-added.md).\n",
        )
        .map_err(|e| e.to_string())?;
        let edit_start = Instant::now();
        knowledge_index::index_paths_with_embedder(
            &mut conn,
            "scale",
            &space_root,
            std::slice::from_ref(&edited_rel),
            embedder,
        )?;
        let incremental_edit_ms = edit_start.elapsed().as_secs_f64() * 1000.0;

        // Incremental DELETE
        let deleted_rel = PathBuf::from(&paths[paths.len() / 2]);
        fs::remove_file(space_root.join(&deleted_rel)).map_err(|e| e.to_string())?;
        let del_start = Instant::now();
        knowledge_index::index_paths_with_embedder(
            &mut conn,
            "scale",
            &space_root,
            std::slice::from_ref(&deleted_rel),
            embedder,
        )?;
        let incremental_delete_ms = del_start.elapsed().as_secs_f64() * 1000.0;

        // Strict equivalence check against a fresh full rebuild on the mutated Space
        let fresh_db_path = temp.path().join("fresh.sqlite");
        let mut fresh_conn = open_wal_db(&fresh_db_path)?;
        knowledge_index::rebuild_space_with_embedder(
            &mut fresh_conn,
            "scale",
            &space_root,
            embedder,
        )?;
        knowledge_index::rebuild_links(&mut fresh_conn, "scale")?;

        let equiv_queries = [
            "Incremental Billing Sentinel delta update",
            "Edited Root Module 0000 updated contract",
            "billing deterministic checkpointing backpressure",
            "security sequence numbers SLA threshold",
            "deployment consistency model",
        ];
        let mut equivalent = true;
        for q in &equiv_queries {
            let inc_res = knowledge_index::hybrid_search_with_embedder(
                &conn,
                "scale",
                &space_root,
                q,
                &query_opts,
                embedder,
            )?;
            let full_res = knowledge_index::hybrid_search_with_embedder(
                &fresh_conn,
                "scale",
                &space_root,
                q,
                &query_opts,
                embedder,
            )?;
            if inc_res.evidence.len() != full_res.evidence.len() {
                equivalent = false;
                break;
            }
            for (left, right) in inc_res.evidence.iter().zip(full_res.evidence.iter()) {
                if left.unit_id != right.unit_id
                    || left.page_path != right.page_path
                    || (left.score - right.score).abs() > 1e-6
                {
                    equivalent = false;
                    break;
                }
            }
            if !equivalent {
                break;
            }
        }

        reports.push(ScalePointReport {
            page_count,
            indexed_pages: stats.indexed_pages,
            knowledge_units: stats.knowledge_units,
            page_links: stats.page_links,
            unit_links: stats.unit_links,
            cached_embeddings: stats.cached_embeddings,
            rebuild_ms: round2(rebuild_ms),
            warm_refresh_ms: round2(warm_refresh_ms),
            db_size_kb: round2(db_size_kb),
            incremental_add_ms: round2(incremental_add_ms),
            incremental_edit_ms: round2(incremental_edit_ms),
            incremental_delete_ms: round2(incremental_delete_ms),
            incremental_vs_full_equivalent: equivalent,
            query_p50_ms: round2(percentile(&latencies_ms, 50.0)),
            query_p95_ms: round2(percentile(&latencies_ms, 95.0)),
        });
    }

    Ok(reports)
}

fn run_concurrent_reads(
    embedder: Arc<dyn EmbeddingProvider>,
    thread_counts: &[usize],
    queries_per_thread: usize,
) -> Result<Vec<ConcurrencyPointReport>, String> {
    let corpus_pages = 500usize;
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let space_root = temp.path().join("conc-space");
    let db_path = temp.path().join("conc.sqlite");
    fs::create_dir_all(&space_root).map_err(|e| e.to_string())?;
    corpus::write_synthetic_space(&space_root, corpus_pages)?;

    {
        let mut conn = open_wal_db(&db_path)?;
        knowledge_index::rebuild_space_with_embedder(
            &mut conn,
            "conc",
            &space_root,
            embedder.as_ref(),
        )?;
        knowledge_index::rebuild_links(&mut conn, "conc")?;
    }

    let queries: Arc<Vec<String>> = Arc::new(vec![
        "billing deterministic checkpointing backpressure".to_string(),
        "security sequence numbers SLA threshold".to_string(),
        "storage workflows operational runbook throttle".to_string(),
        "sync cross-domain policies worker shard".to_string(),
        "search module 0012 latency queue depth".to_string(),
        "runtime module 0045 backpressure flag".to_string(),
        "telemetry SLA threshold operational runbook".to_string(),
        "deployment module 0007 consistency model".to_string(),
    ]);

    let mut reports = Vec::new();
    for &threads in thread_counts {
        let wall_start = Instant::now();
        let mut handles = Vec::with_capacity(threads);
        for tid in 0..threads {
            let db_p = db_path.clone();
            let root_p = space_root.clone();
            let emb = Arc::clone(&embedder);
            let qs = Arc::clone(&queries);
            handles.push(thread::spawn(move || -> (Vec<f64>, usize) {
                let conn = match Connection::open_with_flags(
                    &db_p,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                ) {
                    Ok(c) => c,
                    Err(_) => return (Vec::new(), queries_per_thread),
                };
                let opts = HybridSearchOptions {
                    token_budget: Some(800),
                    max_hops: Some(2),
                    seed_top_k: Some(6),
                    include_backlinks: Some(true),
                    exclude_retired: Some(true),
                    follow_redirects: Some(true),
                    use_okf_governance: Some(true),
                    use_lexical: Some(true),
                    use_vector: Some(true),
                    use_graph_expansion: Some(true),
                    scope_filter: None,
                    as_of_date: Some(BENCH_AS_OF_DATE.to_string()),
                };
                let mut lats = Vec::with_capacity(queries_per_thread);
                let mut errs = 0usize;
                for q_idx in 0..queries_per_thread {
                    let query = &qs[(tid + q_idx) % qs.len()];
                    let t0 = Instant::now();
                    match knowledge_index::hybrid_search_with_embedder(
                        &conn,
                        "conc",
                        &root_p,
                        query,
                        &opts,
                        emb.as_ref(),
                    ) {
                        Ok(_) => lats.push(t0.elapsed().as_secs_f64() * 1000.0),
                        Err(_) => errs += 1,
                    }
                }
                (lats, errs)
            }));
        }

        let mut all_lats = Vec::with_capacity(threads * queries_per_thread);
        let mut total_errors = 0usize;
        for handle in handles {
            let (lats, errs) = handle
                .join()
                .map_err(|_| "benchmark worker thread panicked".to_string())?;
            all_lats.extend(lats);
            total_errors += errs;
        }
        let wall_secs = wall_start.elapsed().as_secs_f64();
        all_lats.sort_by(|a, b| a.total_cmp(b));
        let total_queries = threads * queries_per_thread;
        let qps = if wall_secs > 0.0 {
            (total_queries - total_errors) as f64 / wall_secs
        } else {
            0.0
        };

        reports.push(ConcurrencyPointReport {
            corpus_pages,
            threads,
            total_queries,
            wall_ms: round2(wall_secs * 1000.0),
            qps: round2(qps),
            p50_latency_ms: round2(percentile(&all_lats, 50.0)),
            p95_latency_ms: round2(percentile(&all_lats, 95.0)),
            errors: total_errors,
        });
    }

    Ok(reports)
}

fn slugify_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "page".to_string()
    } else {
        trimmed.to_string()
    }
}

struct ExternalAccumulator {
    config_name: String,
    recalls: Vec<f64>,
    full_hits: usize,
    mrrs: Vec<f64>,
    tokens_total: usize,
    budget_violations: usize,
    latencies_ms: Vec<f64>,
}

fn run_external_dataset(
    embedder: &dyn EmbeddingProvider,
    dataset_path: &Path,
) -> Result<ExternalDatasetReport, String> {
    let raw = fs::read_to_string(dataset_path)
        .map_err(|e| format!("cannot read dataset '{}': {e}", dataset_path.display()))?;
    let mut questions = Vec::new();
    for (line_no, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item: ExternalQuestion = serde_json::from_str(trimmed)
            .map_err(|e| format!("invalid JSONL on line {}: {e}", line_no + 1))?;
        questions.push(item);
    }
    if questions.is_empty() {
        return Err("external dataset contains no questions".to_string());
    }

    let budget = 800usize;
    let selected_configs: Vec<(&'static str, HybridSearchOptions)> = build_ablation_configs(budget)
        .into_iter()
        .filter(|(name, _)| *name == "baseline_fts5" || *name == "hybrid_full")
        .collect();

    let mut accum: Vec<ExternalAccumulator> = selected_configs
        .iter()
        .map(|(name, _)| ExternalAccumulator {
            config_name: (*name).to_string(),
            recalls: Vec::new(),
            full_hits: 0,
            mrrs: Vec::new(),
            tokens_total: 0,
            budget_violations: 0,
            latencies_ms: Vec::new(),
        })
        .collect();

    for (q_idx, q) in questions.iter().enumerate() {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let space_root = temp.path().join("ext-space");
        let db_path = temp.path().join("ext.sqlite");
        fs::create_dir_all(&space_root).map_err(|e| e.to_string())?;
        fs::write(space_root.join("index.md"), okf::root_index()).map_err(|e| e.to_string())?;

        let mut title_to_file = BTreeMap::new();
        for (p_idx, para) in q.paragraphs.iter().enumerate() {
            let filename = format!("{:02}-{}.md", p_idx, slugify_title(&para.title));
            title_to_file.insert(para.title.clone(), filename);
        }

        for para in &q.paragraphs {
            let filename = &title_to_file[&para.title];
            let mut body = format!(
                "---\ntype: Note\ntitle: {}\nstatus: active\n---\n\n# {}\n\n{}\n",
                serde_json::to_string(&para.title).unwrap_or_else(|_| "\"Note\"".to_string()),
                para.title,
                para.text
            );
            let mut link_lines = Vec::new();
            for link_title in &para.links {
                if let Some(target_file) = title_to_file.get(link_title) {
                    link_lines.push(format!("- [{link_title}]({target_file})"));
                }
            }
            // Also detect direct entity mentions to sibling paragraph titles (capped at 4)
            if link_lines.is_empty() {
                let lower_text = para.text.to_ascii_lowercase();
                for (other_title, target_file) in &title_to_file {
                    if other_title != &para.title
                        && other_title.len() >= 4
                        && lower_text.contains(&other_title.to_ascii_lowercase())
                    {
                        link_lines.push(format!("- [{other_title}]({target_file})"));
                        if link_lines.len() >= 4 {
                            break;
                        }
                    }
                }
            }
            if !link_lines.is_empty() {
                body.push_str("\n## Related\n\n");
                body.push_str(&link_lines.join("\n"));
                body.push('\n');
            }
            fs::write(space_root.join(filename), body).map_err(|e| e.to_string())?;
        }

        let space_id = format!("ext-{q_idx}-{}", q.id);
        let mut conn = open_wal_db(&db_path)?;
        knowledge_index::rebuild_space_with_embedder(&mut conn, &space_id, &space_root, embedder)?;
        knowledge_index::rebuild_links(&mut conn, &space_id)?;

        let gold_files: HashSet<&str> = q
            .supporting_titles
            .iter()
            .filter_map(|t| title_to_file.get(t).map(String::as_str))
            .collect();

        for ((_, opts), slot) in selected_configs.iter().zip(accum.iter_mut()) {
            let t0 = Instant::now();
            let res = knowledge_index::hybrid_search_with_embedder(
                &conn,
                &space_id,
                &space_root,
                &q.question,
                opts,
                embedder,
            )?;
            slot.latencies_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
            slot.tokens_total += res.tokens_used;
            if res.tokens_used > budget {
                slot.budget_violations += 1;
            }
            let retrieved_ordered: Vec<&str> =
                res.evidence.iter().map(|e| e.page_path.as_str()).collect();
            let retrieved_set: HashSet<&str> = retrieved_ordered.iter().copied().collect();
            let matched = gold_files
                .iter()
                .filter(|g| retrieved_set.contains(*g))
                .count();
            let recall = if gold_files.is_empty() {
                1.0
            } else {
                matched as f64 / gold_files.len() as f64
            };
            let rr = retrieved_ordered
                .iter()
                .enumerate()
                .find(|(_, p)| gold_files.contains(**p))
                .map(|(idx, _)| 1.0 / (idx as f64 + 1.0))
                .unwrap_or(0.0);
            slot.recalls.push(recall);
            if matched == gold_files.len() {
                slot.full_hits += 1;
            }
            slot.mrrs.push(rr);
        }
    }

    let n = questions.len() as f64;
    let configs = accum
        .into_iter()
        .map(|mut acc| {
            acc.latencies_ms.sort_by(|a, b| a.total_cmp(b));
            AblationConfigMetrics {
                config: acc.config_name,
                page_recall: round4(acc.recalls.iter().sum::<f64>() / n),
                full_multihop_coverage: round4(acc.full_hits as f64 / n),
                mrr: round4(acc.mrrs.iter().sum::<f64>() / n),
                avg_tokens_used: round2(acc.tokens_total as f64 / n),
                budget_violations: acc.budget_violations,
                p50_latency_ms: round2(percentile(&acc.latencies_ms, 50.0)),
                p95_latency_ms: round2(percentile(&acc.latencies_ms, 95.0)),
                by_category: BTreeMap::new(),
            }
        })
        .collect();

    Ok(ExternalDatasetReport {
        dataset_path: dataset_path.display().to_string(),
        questions_evaluated: questions.len(),
        token_budget: budget,
        configs,
    })
}

fn check_invariants(report: &BenchmarkReport) -> Vec<String> {
    let mut failures = Vec::new();
    for budget_rep in &report.retrieval_ablations {
        for cfg in &budget_rep.configs {
            if cfg.budget_violations > 0 {
                failures.push(format!(
                    "budget violation in {} @ budget={}: {}",
                    cfg.config, budget_rep.token_budget, cfg.budget_violations
                ));
            }
        }
        let baseline = budget_rep
            .configs
            .iter()
            .find(|c| c.config == "baseline_fts5");
        let hybrid = budget_rep
            .configs
            .iter()
            .find(|c| c.config == "hybrid_full");
        if let (Some(base), Some(hyb)) = (baseline, hybrid) {
            if hyb.page_recall <= base.page_recall {
                failures.push(format!(
                    "hybrid_full page_recall ({}) did not exceed baseline_fts5 ({}) @ budget={}",
                    hyb.page_recall, base.page_recall, budget_rep.token_budget
                ));
            }
            if hyb.full_multihop_coverage <= base.full_multihop_coverage {
                failures.push(format!(
                    "hybrid_full full_multihop_coverage ({}) did not exceed baseline_fts5 ({}) @ budget={}",
                    hyb.full_multihop_coverage,
                    base.full_multihop_coverage,
                    budget_rep.token_budget
                ));
            }
        }
    }

    let gov = &report.governance_correctness;
    if gov.retired_leakage_rate > 0.0 {
        failures.push(format!(
            "retired_leakage_rate must be 0.0, got {}",
            gov.retired_leakage_rate
        ));
    }
    if gov.redirect_resolution_rate < 1.0 {
        failures.push(format!(
            "redirect_resolution_rate must be 1.0, got {}",
            gov.redirect_resolution_rate
        ));
    }
    if gov.fresh_over_stale_win_rate < 1.0 {
        failures.push(format!(
            "fresh_over_stale_win_rate must be 1.0, got {}",
            gov.fresh_over_stale_win_rate
        ));
    }
    if gov.trust_reset_pass_rate < 1.0 {
        failures.push(format!(
            "trust_reset_pass_rate must be 1.0, got {}",
            gov.trust_reset_pass_rate
        ));
    }
    if gov.validator_precision < 1.0 || gov.validator_recall < 1.0 {
        failures.push(format!(
            "validate_bundle precision/recall must be 1.0/1.0, got {}/{}",
            gov.validator_precision, gov.validator_recall
        ));
    }

    for sp in &report.scaling_and_incremental {
        if !sp.incremental_vs_full_equivalent {
            failures.push(format!(
                "incremental index diverged from full rebuild at {} pages",
                sp.page_count
            ));
        }
    }

    for cp in &report.concurrent_reads {
        if cp.errors > 0 {
            failures.push(format!(
                "concurrent reads encountered {} errors at {} threads",
                cp.errors, cp.threads
            ));
        }
    }

    failures
}

fn print_summary(report: &BenchmarkReport) {
    println!("=========================================================================");
    println!("CoWiki Production Knowledge Engine Benchmark (cowiki-knowledge)");
    println!(
        "OKF v{} | Schema v{} | Embedder: {} ({}d)",
        report.okf_version,
        report.index_schema_version,
        report.embedding_model,
        report.embedding_dim
    );
    println!("=========================================================================\n");

    for budget_rep in &report.retrieval_ablations {
        println!(
            "### 1. Multi-Hop, Anchor & Paraphrase Retrieval Ablations (Budget = {} tokens, N = {})\n",
            budget_rep.token_budget, budget_rep.query_count
        );
        println!(
            "| Config | Page Recall | Full Multi-Hop | MRR | Avg Tokens | Violations | P50 (ms) | P95 (ms) |"
        );
        println!("|---|---:|---:|---:|---:|---:|---:|---:|");
        for cfg in &budget_rep.configs {
            println!(
                "| `{}` | {:.2}% | {:.2}% | {:.4} | {:.1} | {} | {:.2} | {:.2} |",
                cfg.config,
                cfg.page_recall * 100.0,
                cfg.full_multihop_coverage * 100.0,
                cfg.mrr,
                cfg.avg_tokens_used,
                cfg.budget_violations,
                cfg.p50_latency_ms,
                cfg.p95_latency_ms
            );
        }
        println!();
    }

    let g = &report.governance_correctness;
    println!("### 2. OKF v0.3 Governance Correctness\n");
    println!("| Check | Result |");
    println!("|---|---:|");
    println!(
        "| Retired leakage rate ({} queries) | {:.2}% |",
        g.retired_queries_tested,
        g.retired_leakage_rate * 100.0
    );
    println!(
        "| Redirect resolution rate ({} chains) | {:.2}% |",
        g.redirect_cases_tested,
        g.redirect_resolution_rate * 100.0
    );
    println!(
        "| Fresh verified > stale/deprecated ({} pairs) | {:.2}% |",
        g.fresh_vs_stale_pairs_tested,
        g.fresh_over_stale_win_rate * 100.0
    );
    println!(
        "| Agent-Edit Trust Reset pass rate ({} docs) | {:.2}% |",
        g.trust_reset_cases_tested,
        g.trust_reset_pass_rate * 100.0
    );
    println!(
        "| `validate_bundle` precision / recall ({}/{} defects) | {:.2}% / {:.2}% |\n",
        g.validator_detected_issues,
        g.validator_expected_issues,
        g.validator_precision * 100.0,
        g.validator_recall * 100.0
    );

    println!("### 3. Corpus Scaling & Incremental Update Equivalence\n");
    println!(
        "| Pages | Units | Links | Rebuild (ms) | Warm Refresh (ms) | DB (KB) | Inc Add/Edit/Del (ms) | Equiv | Query P50/P95 (ms) |"
    );
    println!("|---:|---:|---:|---:|---:|---:|---:|:---:|---:|");
    for s in &report.scaling_and_incremental {
        println!(
            "| {} | {} | {} | {:.1} | {:.1} | {:.1} | {:.2} / {:.2} / {:.2} | {} | {:.2} / {:.2} |",
            s.page_count,
            s.knowledge_units,
            s.page_links,
            s.rebuild_ms,
            s.warm_refresh_ms,
            s.db_size_kb,
            s.incremental_add_ms,
            s.incremental_edit_ms,
            s.incremental_delete_ms,
            if s.incremental_vs_full_equivalent {
                "PASS"
            } else {
                "FAIL"
            },
            s.query_p50_ms,
            s.query_p95_ms
        );
    }
    println!();

    println!("### 4. Multi-Threaded Read Throughput (SQLite WAL, 500-page Space)\n");
    println!("| Threads | Queries | Wall (ms) | Throughput (QPS) | P50 (ms) | P95 (ms) | Errors |");
    println!("|---:|---:|---:|---:|---:|---:|---:|");
    for c in &report.concurrent_reads {
        println!(
            "| {} | {} | {:.1} | {:.1} | {:.2} | {:.2} | {} |",
            c.threads,
            c.total_queries,
            c.wall_ms,
            c.qps,
            c.p50_latency_ms,
            c.p95_latency_ms,
            c.errors
        );
    }
    println!();

    if let Some(ext) = &report.external_dataset {
        println!(
            "### 5. External Multi-Hop Dataset ({}, N = {}, Budget = {} tokens)\n",
            ext.dataset_path, ext.questions_evaluated, ext.token_budget
        );
        println!("| Config | Page Recall | Full Multi-Hop | MRR | Avg Tokens | Violations |");
        println!("|---|---:|---:|---:|---:|---:|");
        for cfg in &ext.configs {
            println!(
                "| `{}` | {:.2}% | {:.2}% | {:.4} | {:.1} | {} |",
                cfg.config,
                cfg.page_recall * 100.0,
                cfg.full_multihop_coverage * 100.0,
                cfg.mrr,
                cfg.avg_tokens_used,
                cfg.budget_violations
            );
        }
        println!();
    }

    if report.invariants_passed {
        println!("STATUS: ALL PRODUCTION INVARIANTS PASSED");
    } else {
        println!("STATUS: INVARIANT FAILURES DETECTED:");
        for fail in &report.invariant_failures {
            println!("  - {fail}");
        }
    }
}

fn main() {
    let args = match parse_args() {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(2);
        }
    };

    let embedder: Arc<dyn EmbeddingProvider> = if args.embedder_mode == "remote" {
        match OpenAiCompatibleEmbedder::from_env() {
            Some(remote) => Arc::new(remote),
            None => {
                eprintln!(
                    "error: --embedder remote requires COWIKI_EMBEDDING_URL, COWIKI_EMBEDDING_API_KEY, and COWIKI_EMBEDDING_MODEL"
                );
                std::process::exit(2);
            }
        }
    } else {
        Arc::new(LocalHashProjectionEmbedder::default())
    };

    let retrieval_ablations = match run_retrieval_ablations(embedder.as_ref()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("retrieval ablation suite failed: {e}");
            std::process::exit(1);
        }
    };

    let governance_correctness = match run_governance_correctness(embedder.as_ref()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("governance suite failed: {e}");
            std::process::exit(1);
        }
    };

    let scaling_and_incremental =
        match run_scaling_and_incremental(embedder.as_ref(), &args.scale_pages) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("scaling suite failed: {e}");
                std::process::exit(1);
            }
        };

    let concurrent_reads = match run_concurrent_reads(
        Arc::clone(&embedder),
        &args.threads,
        args.queries_per_thread,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("concurrency suite failed: {e}");
            std::process::exit(1);
        }
    };

    let external_dataset = match &args.dataset_path {
        Some(path) => match run_external_dataset(embedder.as_ref(), path) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("external dataset evaluation failed: {e}");
                std::process::exit(1);
            }
        },
        None => None,
    };

    let mut report = BenchmarkReport {
        benchmark_version: "1.0.0".to_string(),
        generated_at_utc: okf::utc_today_iso(),
        okf_version: okf::OKF_VERSION.to_string(),
        index_schema_version: 4,
        embedder_mode: args.embedder_mode,
        embedding_model: embedder.model_name().to_string(),
        embedding_dim: embedder.dimension(),
        retrieval_ablations,
        governance_correctness,
        scaling_and_incremental,
        concurrent_reads,
        external_dataset,
        invariants_passed: true,
        invariant_failures: Vec::new(),
    };

    let failures = check_invariants(&report);
    report.invariants_passed = failures.is_empty();
    report.invariant_failures = failures;

    print_summary(&report);

    if let Some(out_path) = &args.out_path {
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("failed to create output directory: {e}");
                    std::process::exit(1);
                }
            }
        }
        match serde_json::to_string_pretty(&report) {
            Ok(json) => {
                if let Err(e) = fs::write(out_path, format!("{json}\n")) {
                    eprintln!("failed to write '{}': {e}", out_path.display());
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("failed to serialize benchmark report: {e}");
                std::process::exit(1);
            }
        }
    }

    if args.check_thresholds && !report.invariants_passed {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_handles_edge_cases() {
        assert_eq!(percentile(&[], 50.0), 0.0);
        assert_eq!(percentile(&[4.2], 95.0), 4.2);
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 50.0), 3.0);
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 95.0), 5.0);
    }

    #[test]
    fn external_jsonl_evaluator_runs_end_to_end() {
        let temp = tempfile::tempdir().unwrap();
        let jsonl = temp.path().join("sample.jsonl");
        let line = serde_json::json!({
            "id": "q1",
            "question": "How long does Alpha Service store journal records?",
            "supporting_titles": ["Alpha Service", "Journal Policy"],
            "paragraphs": [
                {
                    "title": "Alpha Service",
                    "text": "Alpha Service delegates immutable storage rules to Journal Policy.",
                    "links": ["Journal Policy"]
                },
                {
                    "title": "Journal Policy",
                    "text": "Immutable financial journal records are retained for seven years."
                },
                {
                    "title": "Unrelated Note",
                    "text": "Office espresso machine maintenance schedule."
                }
            ]
        });
        fs::write(&jsonl, format!("{line}\n")).unwrap();

        let embedder = LocalHashProjectionEmbedder::default();
        let ext = run_external_dataset(&embedder, &jsonl).unwrap();
        assert_eq!(ext.questions_evaluated, 1);
        let hyb = ext
            .configs
            .iter()
            .find(|c| c.config == "hybrid_full")
            .unwrap();
        assert_eq!(hyb.budget_violations, 0);
        assert_eq!(hyb.full_multihop_coverage, 1.0);
    }
}
