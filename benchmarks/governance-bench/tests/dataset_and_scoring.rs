use cowiki_governance_bench::{load_dataset, Counts};
use std::path::Path;

#[test]
fn pilot_baseline_matches_hand_counts() {
    let data = load_dataset(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pilot"),
        "dev",
    )
    .unwrap();
    let mut counts = Counts::default();
    for case in &data.cases {
        counts.observe(case, &data.gold[&case.id], &[], &case.candidate);
    }
    assert_eq!((counts.cases, counts.valid_cases), (24, 12));
    assert_eq!((counts.tp, counts.fp, counts.fn_), (0, 0, 16));
    assert_eq!((counts.repaired_cases, counts.residual_cases), (0, 12));
    assert_eq!(counts.metrics()["precision"], serde_json::Value::Null);
    assert_eq!(counts.metrics()["f1"], 0.0);
}

#[test]
fn duplicate_detections_do_not_erase_misses_and_a_flag_is_not_a_repair() {
    let data = load_dataset(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pilot"),
        "dev",
    )
    .unwrap();
    let case = data
        .cases
        .iter()
        .find(|case| data.gold[&case.id].issues.len() == 2)
        .unwrap();
    let gold = &data.gold[&case.id];
    let prediction = gold.issues[0].clone();
    let mut counts = Counts::default();
    counts.observe(
        case,
        gold,
        &[prediction.clone(), prediction],
        &case.candidate,
    );
    assert_eq!(
        (counts.tp, counts.fp, counts.fn_, counts.repaired_cases),
        (1, 1, 1, 0)
    );
    assert_eq!(counts.metrics()["f1"], 0.5);
    counts.observe(case, gold, &gold.issues, &gold.acceptable[0]);
    assert_eq!((counts.repaired_cases, counts.unintended_changes), (1, 0));
}

#[test]
fn a_corrected_value_does_not_excuse_loss_of_a_valid_edit() {
    let data = load_dataset(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pilot"),
        "dev",
    )
    .unwrap();
    let case = data
        .cases
        .iter()
        .find(|case| data.gold[&case.id].primary_type == "concurrent_edit")
        .unwrap();
    let label = &data.gold[&case.id];
    let mut counts = Counts::default();
    counts.observe(case, label, &label.issues, &case.base);
    assert_eq!((counts.repaired_cases, counts.unintended_changes), (0, 1));
}

#[test]
fn unknown_split_is_rejected_before_reading_inputs() {
    assert!(load_dataset(Path::new("."), "../dev").is_err());
}

#[test]
fn a_source_cannot_be_registered_again_in_another_split() {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pilot/dev/families.jsonl");
    let text = std::fs::read_to_string(manifest).unwrap();
    let mut duplicate: serde_json::Value =
        serde_json::from_str(text.lines().next().unwrap()).unwrap();
    duplicate["id"] = "another-family".into();
    duplicate["split"] = "held-out".into();
    std::fs::create_dir(temp.path().join("dev")).unwrap();
    std::fs::write(
        temp.path().join("dev/families.jsonl"),
        format!("{text}{duplicate}\n"),
    )
    .unwrap();
    let error = load_dataset(temp.path(), "dev").err().unwrap();
    assert!(error.contains("shared by families"), "{error}");
}

#[test]
fn dev_load_requires_only_dev_records() {
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("dev");
    std::fs::create_dir(&dest).unwrap();
    let pilot = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pilot/dev");
    for file in ["cases.jsonl", "families.jsonl", "gold.jsonl"] {
        std::fs::copy(pilot.join(file), dest.join(file)).unwrap();
    }
    // The locked split is deliberately absent from this checkout.
    assert_eq!(load_dataset(temp.path(), "dev").unwrap().cases.len(), 24);
}
