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

#[test]
fn development_slices_are_disjoint_families_and_pilot_is_bounded() {
    use cowiki_governance_bench::select_dev_slice;
    use std::collections::BTreeSet;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    let ids = |slice: &str| {
        let mut d = load_dataset(&root, "dev").unwrap();
        select_dev_slice(&mut d, "dev", slice).unwrap();
        (
            d.cases.len(),
            d.cases
                .into_iter()
                .map(|c| c.family)
                .collect::<BTreeSet<_>>(),
        )
    };
    let (train, tf) = ids("tune");
    let (valid, vf) = ids("validation");
    assert_eq!((train, valid, tf.len(), vf.len()), (48, 16, 6, 2));
    assert!(tf.is_disjoint(&vf));
    assert_eq!(ids("pilot").0, 16);
}

#[test]
fn checked_in_offline_references_match_each_case() {
    let tmp = tempfile::tempdir().unwrap();
    for mode in ["none", "rules"] {
        let generated = tmp.path().join(mode);
        let status = Command::new(env!("CARGO_BIN_EXE_cowiki-governance-bench"))
            .args(["--split", "dev", "--verifier", mode, "--baseline", "--out"])
            .arg(&generated)
            .status()
            .unwrap();
        assert!(status.success());
        let published =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("results/dev-{mode}.json"));
        let expected = std::fs::read(published).unwrap();
        let observed = std::fs::read(generated).unwrap();
        assert!(observed == expected, "{mode}: development baseline differs");
    }
}
