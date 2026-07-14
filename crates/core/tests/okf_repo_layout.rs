use cowiki_core::git::WikiRepo;
use git2::{Repository, Signature};
use std::fs;
use std::path::Path;

fn branch_blobs(repo_path: &Path, branch: &str) -> Vec<(String, Vec<u8>)> {
    fn walk(
        git: &Repository,
        tree: &git2::Tree<'_>,
        prefix: &str,
        output: &mut Vec<(String, Vec<u8>)>,
    ) {
        for entry in tree.iter() {
            let name = entry.name().unwrap();
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            match entry.kind() {
                Some(git2::ObjectType::Tree) => {
                    walk(git, &git.find_tree(entry.id()).unwrap(), &path, output)
                }
                Some(git2::ObjectType::Blob) => {
                    output.push((path, git.find_blob(entry.id()).unwrap().content().to_vec()))
                }
                _ => {}
            }
        }
    }

    let git = Repository::open(repo_path).unwrap();
    let branch = git.find_branch(branch, git2::BranchType::Local).unwrap();
    let commit = branch.get().peel_to_commit().unwrap();
    let mut blobs = Vec::new();
    walk(&git, &commit.tree().unwrap(), "", &mut blobs);
    blobs
}

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
        "---\ntitle: Research Library\nsummary: Curated material.\n---\n\n",
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
    let migrated_index =
        String::from_utf8(repo.read_file("main", "teams/index.md").unwrap().unwrap()).unwrap();
    assert!(migrated_index.contains("# Research Library"));
    assert!(migrated_index.contains("Curated material."));
    assert!(migrated_index.contains("[Customer](<customer.md>)"));
    let root_index =
        String::from_utf8(repo.read_file("main", "index.md").unwrap().unwrap()).unwrap();
    assert!(root_index.contains("[Research Library](<teams/>)"));
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
    let blobs = branch_blobs(&path, "main");
    assert!(blobs.iter().any(|(name, body)| {
        name.starts_with(".cowiki/legacy/indexes/")
            && body == b"---\ntitle: Research Library\nsummary: Curated material.\n---\n\n"
    }));

    assert!(path.join("teams/customer.md").is_file());
    assert!(!path.join("wiki/teams/customer.md").exists());
    let git = Repository::open(&path).unwrap();
    let statuses = git.statuses(None).unwrap();
    assert!(statuses.is_empty(), "migration left a dirty worktree/index");

    // Opening again is idempotent and does not create another migration commit.
    let before = repo.commit_count("main").unwrap();
    drop(repo);
    let reopened = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    assert_eq!(reopened.commit_count("main").unwrap(), before);
}

#[test]
fn migration_archives_legacy_content_when_paths_collide() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("repo");
    let git = Repository::init(&path).unwrap();
    fs::create_dir_all(path.join("wiki")).unwrap();
    let canonical = b"---\ntype: Note\ntitle: Canonical\n---\n\nCanonical body.\n";
    let legacy = b"---\ntitle: Legacy\n---\n\nLegacy body must survive.\n";
    fs::write(path.join("topic.md"), canonical).unwrap();
    fs::write(path.join("wiki/topic.md"), legacy).unwrap();
    let mut index = git.index().unwrap();
    index.add_path(Path::new("topic.md")).unwrap();
    index.add_path(Path::new("wiki/topic.md")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = git.find_tree(tree_oid).unwrap();
    let sig = Signature::now("legacy", "legacy@example.com").unwrap();
    git.commit(Some("HEAD"), &sig, &sig, "legacy", &tree, &[])
        .unwrap();
    drop(tree);
    drop(git);

    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    assert_eq!(
        repo.read_file("main", "topic.md").unwrap().unwrap(),
        canonical
    );
    let blobs = branch_blobs(&path, "main");
    assert!(blobs
        .iter()
        .any(|(name, body)| { name.starts_with(".cowiki/legacy/collisions/") && body == legacy }));
}

#[test]
fn migration_does_not_overwrite_existing_preservation_files() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("repo");
    let git = Repository::init(&path).unwrap();
    fs::create_dir_all(path.join(".cowiki/legacy")).unwrap();
    let existing = b"Existing preservation file.\n";
    let invalid_log = b"# Old Log\n\n## yesterday\n* Preserve this too.\n";
    fs::write(path.join(".cowiki/legacy/log.md.txt"), existing).unwrap();
    fs::write(path.join("log.md"), invalid_log).unwrap();
    let mut index = git.index().unwrap();
    index
        .add_path(Path::new(".cowiki/legacy/log.md.txt"))
        .unwrap();
    index.add_path(Path::new("log.md")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = git.find_tree(tree_oid).unwrap();
    let sig = Signature::now("legacy", "legacy@example.com").unwrap();
    git.commit(Some("HEAD"), &sig, &sig, "legacy", &tree, &[])
        .unwrap();
    drop(tree);
    drop(git);

    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    assert_eq!(
        repo.read_file("main", ".cowiki/legacy/log.md.txt")
            .unwrap()
            .unwrap(),
        existing
    );
    let blobs = branch_blobs(&path, "main");
    assert!(blobs
        .iter()
        .any(|(name, body)| { name.starts_with(".cowiki/legacy/log.md") && body == invalid_log }));
}

#[test]
fn migration_does_not_reformat_already_conforming_concepts() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("repo");
    let git = Repository::init(&path).unwrap();
    let valid =
        b"---\n# producer comment\ntype: Custom\nunknown: { nested: true }\n---\n\nExact body.\n";
    let invalid = b"No frontmatter yet.\n";
    fs::write(path.join("valid.md"), valid).unwrap();
    fs::write(path.join("invalid.md"), invalid).unwrap();
    let mut index = git.index().unwrap();
    index.add_path(Path::new("valid.md")).unwrap();
    index.add_path(Path::new("invalid.md")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = git.find_tree(tree_oid).unwrap();
    let sig = Signature::now("legacy", "legacy@example.com").unwrap();
    git.commit(Some("HEAD"), &sig, &sig, "legacy", &tree, &[])
        .unwrap();
    drop(tree);
    drop(git);

    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    assert_eq!(repo.read_file("main", "valid.md").unwrap().unwrap(), valid);
}

#[test]
fn indexes_are_kept_as_progressive_disclosure_for_changed_directories() {
    let data = tempfile::tempdir().unwrap();
    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    repo.write_file(
        "main",
        "architecture.md",
        b"---\ntype: Note\ntitle: System Architecture\ndescription: System boundaries.\n---\n",
        "add architecture",
        "alice",
    )
    .unwrap();
    repo.write_file(
        "main",
        "teams/customer.md",
        b"---\ntype: Note\ntitle: Customer Team\n---\n",
        "add team",
        "alice",
    )
    .unwrap();
    repo.write_file(
        "main",
        "empty/index.md",
        b"# Empty Folder\n",
        "add empty folder",
        "alice",
    )
    .unwrap();

    let root = String::from_utf8(repo.read_file("main", "index.md").unwrap().unwrap()).unwrap();
    assert!(root.contains("[System Architecture](<architecture.md>) - System boundaries."));
    assert!(root.contains("[Teams](<teams/>)"));
    assert!(root.contains("[Empty Folder](<empty/>)"));
    let teams =
        String::from_utf8(repo.read_file("main", "teams/index.md").unwrap().unwrap()).unwrap();
    assert!(teams.contains("[Customer Team](<customer.md>)"));
}

#[test]
fn failed_index_refresh_does_not_advance_the_branch() {
    let data = tempfile::tempdir().unwrap();
    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    drop(repo);
    let path = data.path().join("repo");
    let git = Repository::open(&path).unwrap();
    let parent = git.head().unwrap().peel_to_commit().unwrap();
    let bad_index = b"# Teams\n\n<!-- cowiki:generated-index:start -->\n";
    let mut index = git.index().unwrap();
    index.read_tree(&parent.tree().unwrap()).unwrap();
    index
        .add_frombuffer(
            &git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o100644,
                uid: 0,
                gid: 0,
                file_size: bad_index.len() as u32,
                id: git2::Oid::zero(),
                flags: 0,
                flags_extended: 0,
                path: b"teams/index.md".to_vec(),
            },
            bad_index,
        )
        .unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = git.find_tree(tree_oid).unwrap();
    let sig = Signature::now("legacy", "legacy@example.com").unwrap();
    let bad_commit = git
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "bad index fixture",
            &tree,
            &[&parent],
        )
        .unwrap();
    drop(tree);
    drop(parent);
    drop(git);

    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    let result = repo.write_file(
        "main",
        "teams/customer.md",
        b"---\ntype: Note\ntitle: Customer\n---\n",
        "add customer",
        "alice",
    );
    assert!(result.is_err());
    let git = Repository::open(&path).unwrap();
    assert_eq!(git.refname_to_id("refs/heads/main").unwrap(), bad_commit);
    assert!(repo
        .read_file("main", "teams/customer.md")
        .unwrap()
        .is_none());
}

#[test]
fn writing_a_page_preserves_unrelated_git_entry_modes_and_submodules() {
    let data = tempfile::tempdir().unwrap();
    let repo = WikiRepo::open_or_init(data.path().to_str().unwrap()).unwrap();
    let path = data.path().join("repo");
    let git = Repository::open(&path).unwrap();
    let parent = git.head().unwrap().peel_to_commit().unwrap();
    let executable = b"#!/bin/sh\n";
    let symlink = b"target.txt";
    let mut index = git.index().unwrap();
    index.read_tree(&parent.tree().unwrap()).unwrap();
    for (entry_path, mode, contents) in [
        ("tools/check", 0o100755, executable.as_slice()),
        ("latest", 0o120000, symlink.as_slice()),
    ] {
        index
            .add_frombuffer(
                &git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode,
                    uid: 0,
                    gid: 0,
                    file_size: contents.len() as u32,
                    id: git2::Oid::zero(),
                    flags: 0,
                    flags_extended: 0,
                    path: entry_path.as_bytes().to_vec(),
                },
                contents,
            )
            .unwrap();
    }
    let submodule_oid = parent.id();
    index
        .add(&git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o160000,
            uid: 0,
            gid: 0,
            file_size: 0,
            id: submodule_oid,
            flags: 0,
            flags_extended: 0,
            path: b"vendor/example".to_vec(),
        })
        .unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = git.find_tree(tree_oid).unwrap();
    let sig = Signature::now("fixture", "fixture@example.com").unwrap();
    git.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "add special entries",
        &tree,
        &[&parent],
    )
    .unwrap();
    drop(tree);
    drop(parent);
    drop(git);

    repo.write_file(
        "main",
        "page.md",
        b"---\ntype: Note\ntitle: Page\n---\n",
        "write page",
        "alice",
    )
    .unwrap();

    let git = Repository::open(&path).unwrap();
    let tree = git.head().unwrap().peel_to_tree().unwrap();
    assert_eq!(
        tree.get_path(Path::new("tools/check")).unwrap().filemode(),
        0o100755
    );
    assert_eq!(
        tree.get_path(Path::new("latest")).unwrap().filemode(),
        0o120000
    );
    let submodule = tree.get_path(Path::new("vendor/example")).unwrap();
    assert_eq!(submodule.filemode(), 0o160000);
    assert_eq!(submodule.id(), submodule_oid);
}

#[test]
fn a_failing_secondary_branch_does_not_advance_the_checked_out_branch() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("repo");
    let git = Repository::init(&path).unwrap();
    fs::create_dir_all(path.join("wiki")).unwrap();
    fs::write(path.join("wiki/page.md"), "Legacy page.\n").unwrap();
    let mut index = git.index().unwrap();
    index.add_path(Path::new("wiki/page.md")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = git.find_tree(tree_oid).unwrap();
    let sig = Signature::now("legacy", "legacy@example.com").unwrap();
    let main_oid = git
        .commit(Some("HEAD"), &sig, &sig, "legacy", &tree, &[])
        .unwrap();
    let parent = git.find_commit(main_oid).unwrap();
    let mut bad_index = git.index().unwrap();
    bad_index.read_tree(&parent.tree().unwrap()).unwrap();
    let malformed = b"# Knowledge\n\n<!-- cowiki:generated-index:start -->\n";
    bad_index
        .add_frombuffer(
            &git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o100644,
                uid: 0,
                gid: 0,
                file_size: malformed.len() as u32,
                id: git2::Oid::zero(),
                flags: 0,
                flags_extended: 0,
                path: b"index.md".to_vec(),
            },
            malformed,
        )
        .unwrap();
    let bad_tree_oid = bad_index.write_tree().unwrap();
    let bad_tree = git.find_tree(bad_tree_oid).unwrap();
    git.commit(
        Some("refs/heads/zzz-bad"),
        &sig,
        &sig,
        "bad secondary branch",
        &bad_tree,
        &[&parent],
    )
    .unwrap();
    drop(bad_tree);
    drop(parent);
    drop(tree);
    drop(git);

    assert!(WikiRepo::open_or_init(data.path().to_str().unwrap()).is_err());
    let git = Repository::open(&path).unwrap();
    assert_eq!(git.refname_to_id("refs/heads/main").unwrap(), main_oid);
    assert!(path.join("wiki/page.md").is_file());
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

    repo.write_file(
        "user/alice",
        "architecture/index.md",
        b"# Architecture\n\n* [Overview](overview.md)\n",
        "edit index",
        "alice",
    )
    .unwrap();
    let index_diffs = repo
        .diff_files("user/alice", &["architecture/index".into()])
        .unwrap();
    assert_eq!(index_diffs[0].path, "architecture/index.md");
}
