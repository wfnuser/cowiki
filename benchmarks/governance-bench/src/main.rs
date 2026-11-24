use cowiki_governance_bench::{
    evaluation::{Cost, Mode, Runner},
    evidence_packet, load_dataset, semantic_findings, Counts, Finding,
};
use cowiki_knowledge::governance::{digest, protocol};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
    process::ExitCode,
    time::Instant,
};

fn run() -> Result<(), String> {
    let mut split = String::from("dev");
    let mut out = None;
    let mut verifier = String::from("rules");
    let mut cache = String::from("record");
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" {
            println!("cowiki-governance-bench --split dev|held-out --verifier none|rules|single-pass|remote|both [--cache record|replay] [--out FILE]");
            return Ok(());
        }
        let val = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--cache" if matches!(val.as_str(), "record" | "replay") => cache = val,
            "--verifier" => verifier = val,
            "--split" => split = val,
            "--out" => out = Some(val),
            _ => return Err(format!("invalid option {flag} {val}")),
        }
    }
    let mode: Mode = verifier.parse()?;
    #[cfg(feature = "remote-verifier")]
    let runner = Runner {
        provider: if mode.uses_model() {
            use cowiki_knowledge::governance::remote::*;
            Some(RemoteVerifier::from_env(if cache == "record" {
                CacheMode::Record
            } else {
                CacheMode::Replay
            })?)
        } else {
            None
        },
    };
    #[cfg(not(feature = "remote-verifier"))]
    let runner = {
        if mode.uses_model() || cache == "replay" {
            return Err("remote mode requires --features remote-verifier".into());
        }
        Runner::default()
    };
    let dataset = load_dataset(&Path::new(env!("CARGO_MANIFEST_DIR")).join("data"), &split)?;
    let mut total = Counts::default();
    let mut cost = Cost::default();
    let mut by_type = BTreeMap::<String, (Counts, Cost)>::new();
    let mut by_code = BTreeMap::<String, Counts>::new();
    let mut rows = Vec::new();
    for case in &dataset.cases {
        let start = Instant::now();
        let packet = protocol::pack(evidence_packet(&dataset.families[&case.family], case)?);
        let lookup_ns = start.elapsed().as_nanos() as u64;
        let hash = digest(&serde_json::to_string(&packet).map_err(|e| e.to_string())?);
        let outcome = runner.run(mode, &packet);
        let elapsed = start.elapsed().as_nanos() as u64;
        let result = outcome.maintenance.as_ref();
        let final_text = result.map_or(&case.candidate, |r| &r.final_text);
        let findings = result
            .map(|r| semantic_findings(&r.initial))
            .unwrap_or_default();
        let label = &dataset.gold[&case.id];
        let semantic = label
            .issues
            .iter()
            .filter(|f| {
                matches!(
                    f.code.as_str(),
                    "contradicts_verified"
                        | "reintroduces_retired"
                        | "revision_regression"
                        | "duplicate_overlap"
                )
            })
            .collect::<Vec<_>>();
        let retrieved = semantic
            .iter()
            .filter(|f| {
                packet.evidence.iter().any(|e| {
                    f.evidence.path == e.claim.location.path
                        && e.claim.location.first <= f.evidence.last
                        && f.evidence.first <= e.claim.location.last
                })
            })
            .count();
        let (group, group_cost) = by_type.entry(label.primary_type.clone()).or_default();
        total.observe(case, label, &findings, final_text);
        group.observe(case, label, &findings, final_text);
        cost.observe(&packet, &outcome, lookup_ns, elapsed);
        group_cost.observe(&packet, &outcome, lookup_ns, elapsed);
        let mut remaining = label.issues.iter().collect::<BTreeSet<_>>();
        for f in &findings {
            let c = by_code.entry(f.code.clone()).or_default();
            if remaining.remove(f) {
                c.tp += 1
            } else {
                c.fp += 1
            }
        }
        for Finding { code, .. } in remaining {
            by_code.entry(code.clone()).or_default().fn_ += 1;
        }
        rows.push(json!({"id":case.id,"family":case.family,"primary_type":label.primary_type,"final_text":final_text,
            "findings":findings,"outcome":outcome,"packet_hash":hash,"retrieval_calls":packet.retrieval_calls,
            "retrieval_tokens":packet.retrieval_tokens,"changed_claims":packet.changed.len(),"evidence_claims":packet.evidence.len(),
            "semantic_gold":semantic.len(),"gold_evidence_retrieved":retrieved,"lookup_ns":lookup_ns,"elapsed_ns":elapsed}));
    }
    let groups: BTreeMap<_, _> = by_type
        .into_iter()
        .map(|(ty, (counts, cost))| (ty, json!({"metrics":counts.metrics(),"cost":cost})))
        .collect();
    let codes:BTreeMap<_,_>=by_code.into_iter().map(|(code,c)|{
        let m=c.metrics();
        (code,json!({"tp":c.tp,"fp":c.fp,"fn":c.fn_,"precision":m["precision"],"recall":m["recall"],"f1":m["f1"]}))
    }).collect();
    let report = json!({"schema_version":2,"split":split,"verifier":verifier,"total":total.metrics(),"cost":cost,
        "by_type":groups,"detection_by_code":codes,"cases":rows});
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = out {
        fs::write(&path, format!("{text}\n")).map_err(|e| format!("{path}: {e}"))?
    } else {
        println!("{text}")
    }
    if cost.failures > 0 {
        return Err(format!(
            "{} cases failed; outcomes and costs retained",
            cost.failures
        ));
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
