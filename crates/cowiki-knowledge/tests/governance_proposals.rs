use cowiki_knowledge::{governance::*, okf};
fn replacement(doc: &str) -> FilePatch {
    let start = doc.find("bad value").unwrap();
    FilePatch {
        path: "policy.md".into(),
        preimage: digest(doc),
        edits: vec![TextEdit {
            start,
            end: start + 9,
            before: "bad value".into(),
            after: "supported value".into(),
        }],
    }
}
#[test]
fn verified_transition_keeps_custom_yaml_and_rejects_stale_preimage() {
    let doc="---\r\ntype: Policy\r\ntitle: Trial\r\nverified: true # reviewed\r\ntrust_tier: human_verified\r\ncustom:\r\n  preserved: 'with original quotes'\r\n---\r\n# Trial\r\nbad value\r\n";
    let patch = replacement(doc);
    let t = prepare_review(&patch, doc, doc, "2026-11-23").unwrap();
    assert_eq!(t.before, doc);
    assert!(t.after.contains("verified: false # reviewed\r\n"));
    assert!(t
        .after
        .contains("custom:\r\n  preserved: 'with original quotes'\r\n"));
    assert!(t.after.ends_with("# Trial\r\nsupported value\r\n"));
    let m = okf::parse_metadata(&t.after);
    assert!(!m.is_human_verified() && m.generated);
    assert_eq!(m.updated_at.as_deref(), Some("2026-11-23"));
    assert!(prepare_review(&patch, doc, &format!("{doc}external edit"), "2026-11-23").is_err());
    assert!(prepare_review(&patch, doc, doc, "today").is_err());
    for path in [
        "../p.md",
        ".cowiki/sources/p.md",
        "/tmp/p.md",
        "index.md",
        "dir/log.md",
        "a//b.md",
        "C:/p.md",
    ] {
        assert!(!eligible_concept(path));
    }
}
#[test]
fn base_trust_is_authoritative_when_proposal_already_removed_verification() {
    let candidate = "---\ntype: Note\nverified: false\ncustom: abc\n---\nbad value";
    let base = candidate.replace("false", "true");
    let t = prepare_review(&replacement(candidate), &base, candidate, "2026-11-23").unwrap();
    assert_eq!(okf::parse_metadata(&t.after).trust_tier, "agent_draft");
    assert!(!t.after.ends_with('\n'));
}
