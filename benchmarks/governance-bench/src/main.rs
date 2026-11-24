use cowiki_governance_bench::{load_dataset, Counts};
use std::{collections::BTreeMap, env, fs, path::Path, process::ExitCode, time::Instant};

fn run() -> Result<(), String> {
    let mut split = String::from("dev");
    let mut out = None;
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" {
            println!("cowiki-governance-bench --split dev|held-out --verifier none [--out FILE]");
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--split" => split = value,
            "--out" => out = Some(value),
            "--verifier" if value == "none" => {}
            _ => return Err(format!("unsupported option: {flag} {value}")),
        }
    }
    let dataset = load_dataset(&Path::new(env!("CARGO_MANIFEST_DIR")).join("data"), &split)?;
    let mut total = Counts::default();
    let mut by_type = BTreeMap::<String, Counts>::new();
    let mut rows = Vec::new();
    for case in &dataset.cases {
        let start = Instant::now();
        let final_state = case.candidate.clone();
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        let label = &dataset.gold[&case.id];
        total.observe(case, label, &[], &final_state);
        by_type
            .entry(label.primary_type.clone())
            .or_default()
            .observe(case, label, &[], &final_state);
        rows.push(serde_json::json!({"id":case.id,"family":case.family,"primary_type":label.primary_type,
            "final_text":final_state,"findings":[],"latency_ns":elapsed_ns,"calls":0,"prompt_tokens":0,"completion_tokens":0}));
    }
    let by_type: BTreeMap<_, _> = by_type
        .into_iter()
        .map(|(ty, counts)| (ty, counts.metrics()))
        .collect();
    let report = serde_json::json!({"schema_version":1,"split":split,"verifier":"none",
        "total":total.metrics(),"by_type":by_type,"cases":rows});
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = out {
        fs::write(&path, format!("{json}\n")).map_err(|e| format!("{path}: {e}"))?;
    } else {
        println!("{json}");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
