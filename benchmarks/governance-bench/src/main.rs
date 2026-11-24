use cowiki_governance_bench::{
    evaluation::{Cost, Mode, Runner},
    load_dataset, prepare_case, semantic_findings, Counts, Finding,
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

/// Measured wall time belongs in the full report. CI compares deterministic
/// cost counts and every per-case finding, patch outcome and final document.
fn cost_report(cost: &Cost, baseline: bool) -> serde_json::Value {
    let mut value = json!(cost);
    if baseline {
        if let Some(fields) = value.as_object_mut() {
            for key in [
                "lookup_ns",
                "verifier_ns",
                "repair_and_check_ns",
                "elapsed_ns",
            ] {
                fields.remove(key);
            }
        }
    }
    value
}

fn run() -> Result<(), String> {
    let mut split = String::from("dev");
    let mut out = None;
    let mut baseline = false;
    let mut verifier = String::from("rules");
    let mut dev_slice = None;
    let mut cache = String::from("record");
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--baseline" {
            baseline = true;
            continue;
        }
        if flag == "--help" {
            println!("cowiki-governance-bench --split dev|held-out --verifier none|rules|single-pass|remote|both [--dev-slice tune|validation|pilot] [--cache record|replay] [--out FILE] [--baseline]");
            return Ok(());
        }
        let val = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--cache" if matches!(val.as_str(), "record" | "replay") => cache = val,
            "--verifier" => verifier = val,
            "--dev-slice" => dev_slice = Some(val),
            "--split" => split = val,
            "--out" => out = Some(val),
            _ => return Err(format!("invalid option {flag} {val}")),
        }
    }
    let mode: Mode = verifier.parse()?;
    if baseline && (split != "dev" || dev_slice.is_some() || mode.uses_model()) {
        return Err("--baseline requires full dev and an offline method (none or rules)".into());
    }
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
    if dev_slice.is_some() && split != "dev" {
        return Err("--dev-slice requires --split dev".into());
    }
    let mut dataset = load_dataset(&Path::new(env!("CARGO_MANIFEST_DIR")).join("data"), &split)?;
    if let Some(slice) = &dev_slice {
        cowiki_governance_bench::select_dev_slice(&mut dataset, &split, slice)?;
    }
    let mut total = Counts::default();
    let mut cost = Cost::default();
    let mut by_type = BTreeMap::<String, (Counts, Cost)>::new();
    let mut by_code = BTreeMap::<String, Counts>::new();
    let mut rows = Vec::new();
    for case in &dataset.cases {
        let start = Instant::now();
        let (packet, structural) = prepare_case(
            &dataset.families[&case.family],
            case,
            matches!(mode, Mode::Rules | Mode::Both),
        )?;
        let packet = protocol::pack(packet);
        let lookup_ns = start.elapsed().as_nanos() as u64;
        let hash = digest(&serde_json::to_string(&packet).map_err(|e| e.to_string())?);
        let mut outcome = runner.run(mode, &packet);
        if let Some(r) = outcome.maintenance.as_mut() {
            let redirects = |issues: &[cowiki_knowledge::governance::Issue]| {
                issues
                    .iter()
                    .filter(|i| i.code == "inbound_redirect")
                    .filter_map(|i| Some((i.location.first, i.evidence.as_ref()?.path.clone())))
                    .collect::<BTreeSet<_>>()
            };
            let model_links = redirects(&r.initial);
            let rule_links = redirects(&structural);
            let approved = match mode {
                Mode::Rules => rule_links,
                Mode::Remote => model_links,
                Mode::Both => rule_links.intersection(&model_links).cloned().collect(),
                _ => BTreeSet::new(),
            };
            for mut item in structural {
                let same = |i: &cowiki_knowledge::governance::Issue| {
                    i.code == item.code
                        && i.location == item.location
                        && i.evidence == item.evidence
                };
                if !r.initial.iter().any(same) {
                    if mode == Mode::Both {
                        item.message.push_str(" Structural detectors disagree.")
                    }
                    r.initial.push(item.clone());
                    r.unresolved.push(item);
                }
            }
            let repair_start = Instant::now();
            let paths = dataset.families[&case.family]
                .pages
                .keys()
                .cloned()
                .chain(std::iter::once(case.path.clone()))
                .collect();
            cowiki_governance_bench::evaluation::repair_redirects(&packet, paths, r, &approved)?;
            outcome.repair_and_check_ns += repair_start.elapsed().as_nanos() as u64;
            r.initial.sort();
            r.unresolved.sort();
        }
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
        if baseline {
            let maintenance = serde_json::to_string(&result).map_err(|e| e.to_string())?;
            rows.push(json!({"id":case.id,"family":case.family,"primary_type":label.primary_type,
                "packet_hash":hash,"findings":findings,"maintenance_hash":digest(&maintenance),
                "final_text_hash":digest(final_text),"retrieval_calls":packet.retrieval_calls,
                "retrieval_tokens":packet.retrieval_tokens,"semantic_gold":semantic.len(),"gold_evidence_retrieved":retrieved}));
            continue;
        }
        rows.push(json!({"id":case.id,"family":case.family,"primary_type":label.primary_type,"final_text":final_text,
            "findings":findings,"outcome":outcome,"packet_hash":hash,"retrieval_calls":packet.retrieval_calls,
            "retrieval_tokens":packet.retrieval_tokens,"changed_claims":packet.changed.len(),"evidence_claims":packet.evidence.len(),
            "semantic_gold":semantic.len(),"gold_evidence_retrieved":retrieved,"lookup_ns":lookup_ns,"elapsed_ns":elapsed}));
    }
    let groups: BTreeMap<_, _> = by_type
        .into_iter()
        .map(|(ty, (counts, cost))| {
            (
                ty,
                json!({"metrics":counts.metrics(),"cost":cost_report(&cost,baseline)}),
            )
        })
        .collect();
    let codes:BTreeMap<_,_>=by_code.into_iter().map(|(code,c)|{
        let m=c.metrics();
        (code,json!({"tp":c.tp,"fp":c.fp,"fn":c.fn_,"precision":m["precision"],"recall":m["recall"],"f1":m["f1"]}))
    }).collect();
    let report = json!({"schema_version":2,"split":split,"verifier":verifier,"dev_slice":dev_slice,"baseline":baseline,"total":total.metrics(),"cost":cost_report(&cost,baseline),
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
