//! Opt-in measurement of real local-engine operations on disposable Spaces.
//! A fresh child process measures each prepared corpus; fixture creation is excluded.
use super::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Instant;

const RESULT_PREFIX: &str = "COWIKI_BENCH_RESULT=";
const SLUG: &str = "benchmark";

fn document_path(index: usize) -> String {
    format!("topics/group-{:03}/page-{index:05}.md", index / 100)
}

fn document(index: usize, count: usize) -> String {
    let next = (index + 1) % count;
    let mut text = format!(
        "---\ntitle: Topic {index:05}\ntype: Concept\nsummary: Deterministic local search fixture\n---\n\n# Topic {index:05}\n\n"
    );
    for section in 0..8 {
        text.push_str(&format!(
            "## Section {section}\n\nKnowledge topic{index:05} concerns local Markdown ownership, Git reviews, incremental search, and offline collaboration. This paragraph is deterministic evidence for the desktop benchmark.\n\n"
        ));
    }
    text.push_str(&format!(
        "[Related topic](../group-{:03}/page-{next:05}.md)\n",
        next / 100
    ));
    text
}

fn fixture(root: &Path, count: usize) {
    let folder = root.join("space");
    for index in 0..count {
        let file = folder.join(document_path(index));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, document(index, count)).unwrap();
    }
    let engine = LocalEngine::open(&root.join("metadata")).unwrap();
    engine.add_space("Benchmark", SLUG, &folder).unwrap();
    engine.submit(SLUG, &[]).unwrap();
    assert!(!engine.has_uncommitted_changes(SLUG).unwrap());
    std::fs::write(
        root.join("import.txt"),
        "A deterministic imported source for the benchmark.",
    )
    .unwrap();
}

fn elapsed<T>(action: impl FnOnce() -> T) -> (T, f64) {
    let start = Instant::now();
    let result = action();
    (result, start.elapsed().as_secs_f64() * 1000.0)
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    sorted[((sorted.len() as f64 * quantile).ceil() as usize).saturating_sub(1)]
}

fn distribution(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    json!({
        "unit": "ms", "samples": samples.len(), "min": samples[0],
        "p50": percentile(&samples, 0.50), "p95": percentile(&samples, 0.95),
        "max": samples[samples.len() - 1], "values": samples,
    })
}

fn record<T>(
    metrics: &mut BTreeMap<&'static str, Vec<f64>>,
    name: &'static str,
    action: impl FnOnce() -> T,
) -> T {
    let (result, duration) = elapsed(action);
    metrics.entry(name).or_default().push(duration);
    result
}

fn tree_bytes(root: &Path) -> u64 {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}

fn peak_rss_bytes() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        // SAFETY: getrusage initializes the supplied rusage on success.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } == 0 {
            let value = unsafe { usage.assume_init() }.ru_maxrss as u64;
            return Some(if cfg!(target_os = "macos") {
                value
            } else {
                value * 1024
            });
        }
    }
    None
}

fn measure(root: &Path, count: usize, search_samples: usize, mutation_samples: usize) -> Value {
    let metadata = root.join("metadata");
    let folder = root.join("space");
    let mut metrics = BTreeMap::new();
    let engine = record(&mut metrics, "startup_process_cold", || {
        LocalEngine::open(&metadata).unwrap()
    });
    assert_eq!(engine.list_spaces().unwrap().len(), 1);
    for _ in 0..mutation_samples {
        let reopened = record(&mut metrics, "startup_warm", || {
            LocalEngine::open(&metadata).unwrap()
        });
        drop(reopened);
    }
    let hits = record(&mut metrics, "search_first_use", || {
        engine.search(SLUG, "topic00000", 20).unwrap()
    });
    assert!(!hits.keyword.is_empty());
    for index in 0..search_samples {
        let query = format!("topic{:05}", (index * 37) % count);
        let response = record(&mut metrics, "search_warm", || {
            engine.search(SLUG, &query, 20).unwrap()
        });
        assert!(
            !response.keyword.is_empty(),
            "fixture query must produce a result"
        );
    }
    for iteration in 0..mutation_samples {
        let path = document_path(iteration % count);
        let original = std::fs::read_to_string(folder.join(&path)).unwrap();
        let edited = format!("{original}\nBenchmark edit {iteration}.\n");
        let commits = engine.commit_count(SLUG).unwrap();
        record(&mut metrics, "auto_save", || {
            let _guard = engine.lock_mutations().unwrap();
            engine
                .write_page_checked(SLUG, &path, &edited, Some(&original), false)
                .unwrap();
        });
        assert_eq!(
            engine.commit_count(SLUG).unwrap(),
            commits,
            "Auto Save must not commit"
        );
        assert_eq!(std::fs::read_to_string(folder.join(&path)).unwrap(), edited);
        let diffs = record(&mut metrics, "review_working_diff", || {
            engine.working_diff(SLUG).unwrap()
        });
        assert!(diffs.iter().any(|diff| diff.path == path));
        record(&mut metrics, "checkpoint", || {
            engine
                .create_checkpoint(SLUG, Some("Benchmark checkpoint"))
                .unwrap()
        });
        record(&mut metrics, "history", || engine.history(SLUG).unwrap());
        engine.submit(SLUG, &[]).unwrap();

        let renamed = format!("topics/renamed-{iteration}.md");
        record(&mut metrics, "rename", || {
            engine.rename_path(SLUG, &path, &renamed).unwrap()
        });
        assert!(folder.join(&renamed).is_file());
        record(&mut metrics, "delete", || {
            engine.delete_path(SLUG, &renamed).unwrap()
        });
        assert!(!folder.join(&renamed).exists());
        // Restore the same corpus before the next measured operation.
        engine.write_page(SLUG, &path, &original).unwrap();
        engine.submit(SLUG, &[]).unwrap();

        let input = root.join(format!("import-{iteration}.txt"));
        std::fs::write(
            &input,
            format!("Unique source iteration {iteration}: local offline import."),
        )
        .unwrap();
        let outcomes = record(&mut metrics, "file_import", || {
            engine
                .ingest_files(SLUG, &[input.to_string_lossy().into_owned()])
                .unwrap()
        });
        assert!(outcomes[0].source.is_some());
        assert!(outcomes[0].error.is_none());
        engine.submit(SLUG, &[]).unwrap();

        let change = record(&mut metrics, "background_create", || {
            engine
                .create_agent_change(SLUG, "Benchmark fake Agent")
                .unwrap()
        });
        let background = format!("{original}\nBackground edit {iteration}.\n");
        std::fs::write(change.worktree_path.join(&path), &background).unwrap();
        let changes = record(&mut metrics, "review_agent_changes", || {
            engine.list_agent_changes(SLUG).unwrap()
        });
        let reviewed = changes.iter().find(|item| item.id == change.id).unwrap();
        assert!(!reviewed.diffs.is_empty());
        record(&mut metrics, "background_merge", || {
            engine.merge_agent_change(SLUG, &change.id).unwrap()
        });
        assert_eq!(
            std::fs::read_to_string(folder.join(&path)).unwrap(),
            background
        );
        assert!(
            !change.worktree_path.exists(),
            "merged worktree must be cleaned"
        );
        engine.submit(SLUG, &[]).unwrap();

        let discarded = engine
            .create_agent_change(SLUG, "Discarded fake Agent")
            .unwrap();
        std::fs::write(discarded.worktree_path.join(&path), "Discard this change").unwrap();
        record(&mut metrics, "background_discard", || {
            engine.discard_agent_change(SLUG, &discarded.id).unwrap()
        });
        assert_eq!(
            std::fs::read_to_string(folder.join(&path)).unwrap(),
            background
        );
        assert!(
            !discarded.worktree_path.exists(),
            "discarded worktree must be cleaned"
        );
    }
    let repository = Repository::open(&folder).unwrap();
    let active_worktrees = repository.worktrees().unwrap().len();
    assert_eq!(active_worktrees, 0);
    let metrics = metrics
        .into_iter()
        .map(|(name, samples)| (name, distribution(samples)))
        .collect::<BTreeMap<_, _>>();
    let sqlite_bytes = ["local.db", "local.db-wal", "local.db-shm"]
        .iter()
        .filter_map(|name| std::fs::metadata(metadata.join(name)).ok())
        .map(|file| file.len())
        .sum::<u64>();
    json!({
        "documents": count, "metrics": metrics,
        "peak_rss_bytes": peak_rss_bytes(),
        "sqlite_bytes": sqlite_bytes,
        "git_bytes": tree_bytes(&folder.join(".git")),
        "active_worktrees_after_cleanup": active_worktrees,
    })
}

fn bounded_setting(name: &str, default: usize, max: usize) -> usize {
    let value = std::env::var(name)
        .map(|value| {
            value
                .parse::<usize>()
                .expect("benchmark setting must be an integer")
        })
        .unwrap_or(default);
    assert!(
        (1..=max).contains(&value),
        "{name} must be between 1 and {max}"
    );
    value
}

fn provenance_output(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn benchmark_report(
    sizes: &[usize],
    searches: usize,
    mutations: usize,
    corpora: Vec<Value>,
) -> Value {
    let status = provenance_output(
        "git",
        &["status", "--porcelain", "--untracked-files=normal"],
    );
    json!({
        "schema_version": 2, "fixture_version": 1,
        "source": { "commit": provenance_output("git", &["rev-parse", "--verify", "HEAD"]), "dirty": status.map(|value| !value.is_empty()) },
        "toolchain": { "rustc": provenance_output("rustc", &["--version", "--verbose"]), "cargo": provenance_output("cargo", &["--version"]), "node": provenance_output("node", &["--version"]) },
        "settings": { "sizes": sizes, "search_samples": searches, "mutation_samples": mutations, "fixture_version": 1, "paragraphs_per_document": 8, "documents_per_directory": 100, "search_limit": 20, "worker_process_per_corpus": true },
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "available_parallelism": std::thread::available_parallelism().map(|n| n.get()).ok(),
        "startup_definition": "Fresh worker process with an existing healthy index; OS filesystem caches are not evicted.",
        "latency_percentile": "nearest-rank", "corpora": corpora,
    })
}

#[test]
#[ignore = "run through desktop_benchmark; this is the fresh-process worker"]
fn benchmark_worker() {
    let root = PathBuf::from(
        std::env::var("COWIKI_BENCH_WORKER_ROOT").expect("run desktop_benchmark instead"),
    );
    let count = bounded_setting("COWIKI_BENCH_DOCUMENTS", 100, 10_000);
    let searches = bounded_setting("COWIKI_BENCH_SEARCH_SAMPLES", 20, 1_000);
    let mutations = bounded_setting("COWIKI_BENCH_MUTATION_SAMPLES", 5, 100);
    println!(
        "{RESULT_PREFIX}{}",
        measure(&root, count, searches, mutations)
    );
}

#[test]
#[ignore = "opt-in benchmark; creates disposable 100/1,000/10,000-document Spaces"]
fn desktop_benchmark() {
    let sizes =
        std::env::var("COWIKI_BENCH_SIZES").unwrap_or_else(|_| "100,1000,10000".to_string());
    let sizes = sizes
        .split(',')
        .map(|value| value.trim().parse::<usize>().expect("invalid corpus size"))
        .collect::<Vec<_>>();
    assert!(!sizes.is_empty() && sizes.len() <= 3);
    assert!(sizes.iter().all(|value| (1..=10_000).contains(value)));
    let searches = bounded_setting("COWIKI_BENCH_SEARCH_SAMPLES", 20, 1_000);
    let mutations = bounded_setting("COWIKI_BENCH_MUTATION_SAMPLES", 5, 100);
    // Capture the checkout and toolchain before fixture creation or measurements.
    let mut report = benchmark_report(&sizes, searches, mutations, Vec::new());
    let mut results = Vec::new();
    for count in sizes {
        eprintln!("Preparing {count} documents (excluded from timings)...");
        let root = tempfile::tempdir().unwrap();
        fixture(root.path(), count);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "local_engine::benchmarks::benchmark_worker",
                "--nocapture",
            ])
            .env("COWIKI_BENCH_WORKER_ROOT", root.path())
            .env("COWIKI_BENCH_DOCUMENTS", count.to_string())
            .env("COWIKI_BENCH_SEARCH_SAMPLES", searches.to_string())
            .env("COWIKI_BENCH_MUTATION_SAMPLES", mutations.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "worker failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let result = stdout
            .lines()
            .find_map(|line| line.strip_prefix(RESULT_PREFIX))
            .expect("worker did not return measurements");
        results.push(serde_json::from_str::<Value>(result).unwrap());
        eprintln!("Measured {count} documents.");
    }
    report["corpora"] = json!(results);
    let text = serde_json::to_string_pretty(&report).unwrap();
    if let Ok(path) = std::env::var("COWIKI_BENCH_OUTPUT") {
        std::fs::write(path, &text).unwrap();
    }
    println!("{text}");
}

#[test]
fn benchmark_smoke_exercises_the_complete_offline_workflow() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path(), 3);
    let report = measure(root.path(), 3, 2, 1);
    assert_eq!(report["active_worktrees_after_cleanup"], 0);
    assert_eq!(report["metrics"]["search_warm"]["samples"], 2);
    assert!(report["sqlite_bytes"].as_u64().unwrap() > 0);
    let complete = benchmark_report(&[3], 2, 1, vec![report]);
    assert_eq!(complete["schema_version"], 2);
    assert_eq!(complete["settings"]["sizes"], json!([3]));
    assert_eq!(
        complete["settings"]["search_samples"],
        complete["corpora"][0]["metrics"]["search_warm"]["samples"]
    );
    assert_eq!(
        complete["settings"]["mutation_samples"],
        complete["corpora"][0]["metrics"]["startup_warm"]["samples"]
    );
    let commit = &complete["source"]["commit"];
    assert!(commit.is_null() || commit.as_str().is_some_and(|value| value.len() == 40));
    // Cross-compiled test runners and source archives need not have the build
    // toolchain or Git installed. Missing provenance is explicitly null.
    for (tool, prefix) in [("rustc", "rustc "), ("cargo", "cargo ")] {
        let value = &complete["toolchain"][tool];
        assert!(
            value.is_null()
                || value
                    .as_str()
                    .is_some_and(|value| value.starts_with(prefix))
        );
    }
    assert!(provenance_output("cowiki-nonexistent-provenance-tool", &[]).is_none());
}

#[test]
fn benchmark_percentiles_use_nearest_rank_without_interpolating_tail_samples() {
    let report = distribution(vec![20.0, 1.0, 4.0, 2.0]);
    assert_eq!(report["p50"], 2.0);
    assert_eq!(report["p95"], 20.0);
}
