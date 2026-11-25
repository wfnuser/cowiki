use cowiki_governance_bench::{
    evaluation::{repair_redirects, Mode, Runner},
    prepare_case, Case, Family,
};
#[test]
fn pipeline_scores_only_new_occurrences_and_preserves_code() {
    let page = "---\ntype: Policy\ntitle: Guide\n---\n# Guide\n";
    let family = Family {
        id: "fixture".into(),
        split: "dev".into(),
        source_paths: vec!["doc".into()],
        pages: [
            (
                "mid.md".into(),
                "---\ntype: Redirect\ntitle: Mid\nredirect_to: canonical.md\n---\n".into(),
            ),
            (
                "old.md".into(),
                "---\ntype: Redirect\ntitle: Old\nredirect_to: mid.md\n---\n".into(),
            ),
            ("canonical.md".into(), page.into()),
        ]
        .into(),
    };
    let base = format!("{page}[old](mid.md)\n");
    let case = Case {
        id: "case".into(),
        family: family.id.clone(),
        request: "Edit".into(),
        path: "note.md".into(),
        base: base.clone(),
        candidate: format!("{base}See [[old#faq|answers]].\n\x60[[mid]]\x60\n"),
        history: vec![],
    };
    let (p, issues) = prepare_case(&family, &case, true).unwrap();
    assert_eq!(issues.len(), 1);
    let mut result = Runner::default().run(Mode::Rules, &p).maintenance.unwrap();
    result.initial.extend_from_slice(&issues);
    result.unresolved.extend_from_slice(&issues);
    let approved = [(issues[0].location.first, "old.md".into())].into();
    repair_redirects(
        &p,
        family.pages.keys().cloned().collect(),
        &mut result,
        &approved,
    )
    .unwrap();
    assert_eq!(
        result.final_text,
        case.candidate.replace("[[old#", "[[canonical#")
    );
    assert!(result
        .unresolved
        .iter()
        .all(|i| i.code != "inbound_redirect"));
}
