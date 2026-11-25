use cowiki_knowledge::{
    embeddings::LocalHashProjectionEmbedder, governance::*, knowledge_index as index,
};
use rusqlite::Connection;
use std::{collections::BTreeMap, fs};
#[test]
fn prioritizes_locates_and_deduplicates_without_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let docs=BTreeMap::from([
        ("note.md","---\ntype: Note\ntitle: Test\ntrust_tier: agent_draft\ngenerated: true\nstale_after: 2025-01-01\ncustom: '[yaml](gone.md)'\n---\n# Note\n[real](gone.md) and [[stub]]\n\x60[inline](gone.md)\x60\n\x60\x60\x60md\n[fenced](gone.md)\n\x60\x60\x60\n[again](gone.md)\n"),
        ("stub.md","---\ntype: Note\ntitle: Stub\nredirect_to: note.md\n---\n"),
        ("x.md","---\ntype: Note\ntitle: X\nredirect_to: y.md\n---\n"),
        ("y.md","---\ntype: Note\ntitle: Y\nredirect_to: x.md\n---\n")
    ]);
    for (name, text) in &docs {
        fs::write(tmp.path().join(name), text).unwrap()
    }
    let mut db = Connection::open_in_memory().unwrap();
    index::initialize(&mut db).unwrap();
    index::rebuild_space_with_embedder(
        &mut db,
        "s",
        tmp.path(),
        &LocalHashProjectionEmbedder::default(),
    )
    .unwrap();
    let run = || audit_space(tmp.path(), &db, "s", "2026-11-23").unwrap();
    let a = run();
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&run()).unwrap()
    );
    assert_eq!(a.by_code["inbound_redirect"], 1);
    assert_eq!(a.by_code["redirect_cycle"], 2);
    assert_eq!(a.by_code["expired_stale_after"], 1);
    assert_eq!(a.by_code["unverified_agent_draft"], 1);
    assert_eq!(a.by_code["missing_generated_sources"], 1);
    assert_eq!(a.by_code["broken_link"], 2);
    let lines = a
        .issues
        .iter()
        .filter(|i| i.code == "broken_link")
        .map(|i| i.location.as_ref().unwrap().first)
        .collect::<Vec<_>>();
    assert_eq!(lines, [10, 15]);
    assert!(a
        .issues
        .iter()
        .find(|i| i.code == "missing_root_index")
        .unwrap()
        .location
        .is_none());
    assert!(a
        .issues
        .windows(2)
        .all(|pair| pair[0].severity <= pair[1].severity));
    assert!(introduced(&a, &a, "note.md").is_empty());
    for (name, text) in docs {
        assert_eq!(fs::read_to_string(tmp.path().join(name)).unwrap(), text);
    }
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 4);
    assert!(audit_space(tmp.path(), &db, "s", "today").is_err());
}
