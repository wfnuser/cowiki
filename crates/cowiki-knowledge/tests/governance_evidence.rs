use cowiki_knowledge::{
    embeddings::LocalHashProjectionEmbedder, governance::*, knowledge_index as ki,
};
use std::fs;

#[test]
fn retired_only_lookup_survives_active_competition_and_redirects() {
    let dir = tempfile::tempdir().unwrap();
    let active =
        "---\ntype: Policy\nverified: true\n---\n# Policy\nThe policy window lasts 7 days.\n";
    for i in 0..72 {
        fs::write(dir.path().join(format!("page{i}.md")), active).unwrap();
    }
    fs::write(dir.path().join("old.md"),"---\nstatus: retired\nredirect_to: page0.md\n---\n# Policy\nThe policy window lasts 2 days.\n").unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let embedder = LocalHashProjectionEmbedder::default();
    ki::initialize(&mut db).unwrap();
    ki::rebuild_space_with_embedder(&mut db, "wiki", dir.path(), &embedder).unwrap();
    let input = GovernanceInput {
        path: "page0.md".into(),
        request: String::new(),
        base: active.into(),
        candidate: active.replace("7 days", "2 days"),
        history: vec![],
    };
    let packet = SnapshotIndex {
        connection: &db,
        space_id: "wiki",
        root: dir.path(),
        embedder: &embedder,
    }
    .locate(input, "2026-11-23")
    .unwrap();
    assert_eq!(packet.changed[0].location.to_string(), "[page0.md:L6-L6]");
    assert!(packet
        .evidence
        .iter()
        .any(|e| e.status == "retired" && e.claim.location.to_string() == "[old.md:L6-L6]"));
    assert!(packet
        .evidence
        .iter()
        .any(|e| e.human_verified && e.claim.text.contains("7 days")));
    assert_eq!(packet.retrieval_calls, 2);
    let regular = ki::hybrid_search_with_embedder(
        &db,
        "wiki",
        dir.path(),
        "policy window",
        &Default::default(),
        &embedder,
    )
    .unwrap();
    assert!(regular.evidence.iter().all(|e| e.status != "retired"));
}

#[test]
fn changed_only_skips_code_and_unchanged_lines_between_edits() {
    let input = GovernanceInput {
        path: "p.md".into(),
        request: String::new(),
        base: "one\nkeep\nthree\n".into(),
        candidate: "first\nkeep\nlast\n```\nexample\n```\n".into(),
        history: vec![],
    };
    let lines = changed_claims(&input)
        .unwrap()
        .into_iter()
        .map(|c| c.location.first)
        .collect::<Vec<_>>();
    assert_eq!(lines, [1, 3]);
}
