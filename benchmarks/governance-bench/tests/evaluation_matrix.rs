use cowiki_governance_bench::{evaluation::Mode, load_dataset};
use serde_json::Value;
use std::{path::Path, process::Command};
#[test]
fn development_reports_pair_every_case_and_reconcile_metrics() {
    let dir = tempfile::tempdir().unwrap();
    let mut hashes = vec![];
    for mode in ["none", "rules"] {
        let file = dir.path().join(mode);
        assert!(Command::new(env!("CARGO_BIN_EXE_cowiki-governance-bench"))
            .args(["--split", "dev", "--verifier", mode, "--out"])
            .arg(&file)
            .status()
            .unwrap()
            .success());
        let rep: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        let rows = rep["cases"].as_array().unwrap();
        assert_eq!(rows.len(), 64);
        assert_eq!(rep["cost"]["logical_calls"], 0);
        assert_eq!(rep["total"]["counts"]["valid_cases"], 29);
        for key in ["tp", "fp", "fn"] {
            let sum = rep["detection_by_code"]
                .as_object()
                .unwrap()
                .values()
                .map(|group| group[key].as_u64().unwrap())
                .sum::<u64>();
            assert_eq!(rep["total"]["counts"][key], sum);
        }
        hashes.push(
            rows.iter()
                .map(|r| (r["id"].clone(), r["packet_hash"].clone()))
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(hashes[0], hashes[1]);
    assert!(load_dataset(Path::new("."), "test").is_err());
    assert!("unknown".parse::<Mode>().is_err());
}
