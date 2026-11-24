use cowiki_knowledge::{governance::*, knowledge_index::FileIndex};
fn field(path: &str, target: &str) -> (String, (String, Citation)) {
    (
        path.into(),
        (
            target.into(),
            Citation {
                path: path.into(),
                snapshot: digest(target),
                first: 4,
                last: 4,
            },
        ),
    )
}
#[test]
fn spans_preserve_aliases_titles_encoded_paths_and_all_protected_bytes() {
    let files = FileIndex::new(
        ["dir/note.md", "old.md", "mid.md", "new name.md"]
            .map(String::from)
            .to_vec(),
    );
    let fields = [field("old.md", "mid.md"), field("mid.md", "new%20name.md")].into();
    let text="---\r\ncustom: '[[old]]'\r\n---\r\n# Note\r\n[read](../old.md?q=1#sec \"title\") [[mid#S\\|shown]] [[old]]\r\n\x60[[old]]\x60\r\n~~~md\r\n[x](../old.md)\r\n~~~\r\n";
    let p = propose_redirects("dir/note.md", text, &files, &fields).unwrap();
    let result = p.patch.preview("dir/note.md", text).unwrap();
    assert_eq!(p.patch.edits.len(), 3);
    assert_eq!(p.evidence.len(), 2);
    assert_eq!(
        result,
        text.replace("../old.md?q", "../new%20name.md?q")
            .replace("[[mid#S", "[[new name#S")
            .replace("shown]] [[old]]", "shown]] [[new name]]")
    );
    assert!(propose_redirects("dir/note.md", &result, &files, &fields).is_none());
}
#[test]
fn unresolved_cycles_ambiguous_names_and_missing_terminals_stay_for_review() {
    let files = FileIndex::new(
        [
            "src.md", "a/old.md", "b/old.md", "x.md", "y.md", "lost.md", "done.md",
        ]
        .map(String::from)
        .to_vec(),
    );
    let fields = [
        field("a/old.md", "../done.md"),
        field("b/old.md", "../done.md"),
        field("x.md", "y.md"),
        field("y.md", "x.md"),
        field("lost.md", "absent.md"),
    ]
    .into();
    assert!(propose_redirects(
        "src.md",
        "[[old]] [cycle](x.md) [lost](lost.md)",
        &files,
        &fields
    )
    .is_none());
}
