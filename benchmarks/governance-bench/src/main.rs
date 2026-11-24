use cowiki_governance_bench::{evidence_packet, load_dataset, semantic_findings, Counts};
use cowiki_knowledge::governance::{digest, maintain, RuleConflictDetector};
use std::{collections::BTreeMap, env, fs, path::Path, process::ExitCode, time::Instant};

fn run() -> Result<(), String> {
    let mut split = String::from("dev");
    let mut out = None;
    let mut mode = String::from("rules");
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" {
            println!(
                "cowiki-governance-bench --split dev|held-out --verifier none|rules [--out FILE]"
            );
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--split" => split = value,
            "--out" => out = Some(value),
            "--verifier" if matches!(value.as_str(), "none" | "rules") => mode = value,
            _ => return Err(format!("unsupported option: {flag} {value}")),
        }
    }
    let dataset = load_dataset(&Path::new(env!("CARGO_MANIFEST_DIR")).join("data"), &split)?;
    let mut total = Counts::default();
    let mut by_type = BTreeMap::<String, Counts>::new();
    let mut rows = Vec::new();
    for case in &dataset.cases {
        let start = Instant::now();
        let packet = evidence_packet(&dataset.families[&case.family], case)?;
        let packet_hash = digest(&serde_json::to_string(&packet).map_err(|e| e.to_string())?);
        let result = if mode == "rules" {
            Some(maintain(&packet, &RuleConflictDetector)?)
        } else {
            None
        };
        let final_state = result.as_ref().map_or(&case.candidate, |r| &r.final_text);
        let findings = result
            .as_ref()
            .map(|r| semantic_findings(&r.initial))
            .unwrap_or_default();
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        let label = &dataset.gold[&case.id];
        total.observe(case, label, &findings, final_state);
        by_type
            .entry(label.primary_type.clone())
            .or_default()
            .observe(case, label, &findings, final_state);
        rows.push(serde_json::json!({"id":case.id,"family":case.family,"primary_type":label.primary_type,
            "final_text":final_state,"findings":findings,"maintenance":result,"packet_hash":packet_hash,"retrieval_calls":packet.retrieval_calls,"retrieval_tokens":packet.retrieval_tokens,"latency_ns":elapsed_ns,"calls":0,"prompt_tokens":0,"completion_tokens":0}));
    }
    let by_type: BTreeMap<_, _> = by_type
        .into_iter()
        .map(|(ty, counts)| (ty, counts.metrics()))
        .collect();
    let report = serde_json::json!({"schema_version":1,"split":split,"verifier":mode,
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
