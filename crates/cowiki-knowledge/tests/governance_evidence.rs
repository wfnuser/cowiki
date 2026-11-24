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

#[test]
fn attached_withdrawal_is_cited_without_classifying_it_as_the_assertion() {
    let space = tempfile::tempdir().unwrap();
    let archived = "---\nstatus: retired\n---\n# Receipts\nReceipts expire after 4 days.\n\nThis rule was superseded by the approved charter.\n\nUnrelated archival details.\n\nRetired terminals require a key.\n";
    fs::write(space.path().join("archive.md"), archived).unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let embedder = LocalHashProjectionEmbedder::default();
    ki::initialize(&mut db).unwrap();
    ki::rebuild_space_with_embedder(&mut db, "space", space.path(), &embedder).unwrap();
    let input = GovernanceInput {
        path: "notice.md".into(),
        request: String::new(),
        base: String::new(),
        candidate: "Receipts expire after 4 days.".into(),
        history: vec![],
    };
    let packet = SnapshotIndex {
        connection: &db,
        space_id: "space",
        root: space.path(),
        embedder: &embedder,
    }
    .locate(input, "2025-01-01")
    .unwrap();
    let source = packet
        .evidence
        .iter()
        .find(|e| e.claim.text == "Receipts expire after 4 days.")
        .unwrap();
    assert_eq!(source.claim.location.to_string(), "[archive.md:L5-L7]");
    let other = packet
        .evidence
        .iter()
        .find(|e| e.claim.text == "Unrelated archival details.")
        .unwrap();
    assert_eq!(other.claim.location.first, other.claim.location.last);
    let issues = interpret(&packet, &RuleConflictDetector.classify(&packet).unwrap()).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.code == "reintroduces_retired"
            && i.evidence.as_ref() == Some(&source.claim.location)));
}

#[test]
fn bulky_shared_metadata_preserves_history_sources_and_exact_revision_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let corrected = "Locker access lasts 12 days.";
    let erroneous = "Locker access lasts 2 days.";
    let source="---\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n# Charter\nLocker access lasts 12 days.\n";
    fs::write(dir.path().join("charter.md"), source).unwrap();
    let header = format!("---\nscope: default\ncustom: {}\n---\n", "x".repeat(485));
    let note = |line: &str| format!("{header}# Charter\n{line}\n");
    let base = note(corrected);
    let candidate = note(erroneous);
    fs::write(dir.path().join("locker.md"), &base).unwrap();
    let revision = |id: &str, text: String| Revision {
        id: id.into(),
        path: "locker.md".into(),
        text,
        source_path: "charter.md".into(),
    };
    let history = vec![
        revision("before", candidate.clone()),
        revision("ratified", base.clone()),
    ];
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let embedder = LocalHashProjectionEmbedder::default();
    ki::initialize(&mut db).unwrap();
    ki::rebuild_space_with_embedder(&mut db, "s", dir.path(), &embedder).unwrap();
    let input = GovernanceInput {
        path: "locker.md".into(),
        request: "Amend access duration.".into(),
        base,
        candidate,
        history,
    };
    let packet = protocol::pack(
        SnapshotIndex {
            connection: &db,
            space_id: "s",
            root: dir.path(),
            embedder: &embedder,
        }
        .locate(input, "2025-01-01")
        .unwrap(),
    );
    assert!(protocol::admission(&packet) + protocol::OUTPUT_LIMIT <= protocol::TOKEN_LIMIT);
    let wire = protocol::model_input(&packet);
    assert_eq!(wire["history"][0]["frontmatter_from"], "base");
    assert_eq!(
        wire["history"][1]["text"],
        format!("# Charter\n{corrected}\n")
    );
    assert!(packet.input.history[0].text.starts_with(&header));
    let issues = interpret(&packet, &RuleConflictDetector.classify(&packet).unwrap()).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.code == "revision_regression"
            && i.evidence.as_ref().unwrap().path == "charter.md"));
    let mut different = packet;
    different.input.history[0].text = different.input.history[0]
        .text
        .replace("scope: default", "scope: laboratory");
    let different_wire = protocol::model_input(&different);
    assert!(different_wire["history"][0]
        .get("frontmatter_from")
        .is_none());
    assert!(different_wire["history"][0]["text"]
        .as_str()
        .unwrap()
        .contains("scope: laboratory"));
}
