use cowiki_core::git::WikiRepo;
use git2::{Repository, Signature};
use std::fs;
use std::path::Path;

#[test]
fn fresh_repo_is_an_okf_bundle_at_the_repo_root() {
    let data = tempfile::tempdir().unwrap();
    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();

    let index = repo.read_file("main", "index.md").unwrap().unwrap();
    assert!(String::from_utf8(index)
        .unwrap()
        .contains("okf_version: \"0.1\""));
    assert!(repo
        .list_files_recursive("main", "wiki")
        .unwrap()
        .is_empty());
    assert!(repo
        .list_files_recursive("main", "sources")
        .unwrap()
        .is_empty());
    assert!(repo.validate_okf_branch("main").unwrap().is_conformant());
}

#[test]
fn opening_a_legacy_repo_migrates_paths_and_documents_without_data_loss() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("repo");
    let git = Repository::init(&path).unwrap();
    fs::create_dir_all(path.join("wiki/teams")).unwrap();
    fs::create_dir_all(path.join("sources")).unwrap();
    fs::write(
        path.join("wiki/teams/customer.md"),
        "---\ntitle: Customer\nsummary: Customer record.\npage_id: abc\n---\n\n# Customer\n",
    )
    .unwrap();
    fs::write(
        path.join("wiki/teams/_index.md"),
        "---\ntitle: Teams\n---\n\n# Teams\n",
    )
    .unwrap();
    fs::write(path.join("sources/interview.md"), "raw interview").unwrap();
    fs::write(
        path.join("wiki/broken.md"),
        "---\ntitle: [broken\n---\n\nDo not lose me.\n",
    )
    .unwrap();
    fs::create_dir_all(path.join("wiki/bad-index")).unwrap();
    fs::write(
        path.join("wiki/bad-index/_index.md"),
        "---\ntitle: [broken\n---\n\nOriginal index notes.\n",
    )
    .unwrap();
    fs::create_dir_all(path.join("wiki/unclosed-index")).unwrap();
    fs::write(
        path.join("wiki/unclosed-index/_index.md"),
        "---\ntitle: Unclosed\nOriginal unclosed notes.\n",
    )
    .unwrap();
    fs::write(
        path.join("log.md"),
        "# Old Log\n\n## yesterday\n* Keep this.\n",
    )
    .unwrap();
    fs::write(path.join("wiki/invalid-utf8.md"), b"Legacy bytes: \xff").unwrap();
    let mut index = git.index().unwrap();
    index.add_path(Path::new("wiki/teams/customer.md")).unwrap();
    index.add_path(Path::new("wiki/teams/_index.md")).unwrap();
    index.add_path(Path::new("sources/interview.md")).unwrap();
    index.add_path(Path::new("wiki/broken.md")).unwrap();
    index
        .add_path(Path::new("wiki/bad-index/_index.md"))
        .unwrap();
    index
        .add_path(Path::new("wiki/unclosed-index/_index.md"))
        .unwrap();
    index.add_path(Path::new("log.md")).unwrap();
    index.add_path(Path::new("wiki/invalid-utf8.md")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = git.find_tree(tree_oid).unwrap();
    let sig = Signature::now("legacy", "legacy@example.com").unwrap();
    git.commit(Some("HEAD"), &sig, &sig, "legacy", &tree, &[])
        .unwrap();
    drop(tree);
    drop(git);

    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    let concept = String::from_utf8(
        repo.read_file("main", "teams/customer.md")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(concept.contains("type: Note"));
    assert!(concept.contains("description: Customer record."));
    assert!(concept.contains("page_id: abc"));
    assert!(repo.read_file("main", "teams/index.md").unwrap().is_some());
    let source = String::from_utf8(
        repo.read_file("main", ".cowiki/sources/interview.md")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(source.contains("type: Source"));
    assert_eq!(
        cowiki_core::okf::source_body(&source).unwrap(),
        "raw interview"
    );
    assert!(repo
        .read_file("main", "wiki/teams/customer.md")
        .unwrap()
        .is_none());
    assert!(repo
        .read_file("main", "sources/interview.md")
        .unwrap()
        .is_none());
    let repaired =
        String::from_utf8(repo.read_file("main", "broken.md").unwrap().unwrap()).unwrap();
    assert!(repaired.contains("type: Note"));
    assert!(repaired.contains("Do not lose me."));
    let repaired_index = String::from_utf8(
        repo.read_file("main", "bad-index/index.md")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(repaired_index.contains("Original index notes."));
    let unclosed_index = String::from_utf8(
        repo.read_file("main", "unclosed-index/index.md")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(unclosed_index.contains("Original unclosed notes."));
    assert_eq!(
        repo.read_file("main", ".cowiki/legacy/log.md.txt")
            .unwrap()
            .unwrap(),
        b"# Old Log\n\n## yesterday\n* Keep this.\n"
    );
    assert_eq!(
        repo.read_file("main", ".cowiki/legacy/invalid-utf8.md.bin")
            .unwrap()
            .unwrap(),
        b"Legacy bytes: \xff"
    );
    let validation = repo.validate_okf_branch("main").unwrap();
    assert!(validation.is_conformant(), "{:?}", validation.issues);

    // Opening again is idempotent and does not create another migration commit.
    let before = repo.commit_count("main").unwrap();
    drop(repo);
    let reopened = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    assert_eq!(reopened.commit_count("main").unwrap(), before);
}

#[test]
fn review_diffs_use_concept_ids_without_a_wiki_prefix() {
    let data = tempfile::tempdir().unwrap();
    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    repo.ensure_branch_exists("user/alice").unwrap();
    repo.write_file(
        "user/alice",
        "architecture/overview.md",
        b"---\ntype: Note\ntitle: Overview\n---\n",
        "edit",
        "alice",
    )
    .unwrap();

    let diffs = repo
        .diff_files("user/alice", &["architecture/overview".into()])
        .unwrap();
    assert_eq!(diffs[0].path, "architecture/overview.md");
    assert!(diffs[0].is_new());
}
