use cowiki_cloud::git_repo::{
    GitRepoStore, ReceiveMode, ReceiveUpdate, ZERO_OID, validate_receive_updates,
};
use cowiki_cloud::model::MemberRole;
use std::process::Command;
use uuid::Uuid;

fn update(reference: &str, old_oid: &str, new_oid: &str) -> ReceiveUpdate {
    ReceiveUpdate {
        old_oid: old_oid.to_string(),
        new_oid: new_oid.to_string(),
        reference: reference.to_string(),
    }
}

#[test]
fn repository_paths_are_uuid_derived_and_initialization_is_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let store = GitRepoStore::new(root.path()).unwrap();
    let id = Uuid::new_v4();
    let path = store.repo_path(id);

    assert_eq!(path, root.path().join(format!("{id}.git")));
    store.ensure_space(id).unwrap();
    store.ensure_space(id).unwrap();
    assert!(path.join("HEAD").is_file());
    assert!(path.join("hooks/pre-receive").is_file());
    assert_eq!(
        std::fs::read_to_string(path.join("HEAD")).unwrap(),
        "ref: refs/heads/main\n"
    );
}

#[test]
fn bootstrap_requires_owner_and_equal_main_and_user_heads() {
    let user = Uuid::new_v4();
    let oid = "1".repeat(40);
    let valid = vec![
        update("refs/heads/main", ZERO_OID, &oid),
        update(&format!("refs/heads/user/{user}"), ZERO_OID, &oid),
    ];
    assert!(
        validate_receive_updates(&valid, user, MemberRole::Owner, ReceiveMode::Bootstrap).is_ok()
    );
    assert!(
        validate_receive_updates(&valid, user, MemberRole::Manager, ReceiveMode::Bootstrap)
            .is_err()
    );

    let mut different = valid.clone();
    different[1].new_oid = "2".repeat(40);
    assert!(
        validate_receive_updates(&different, user, MemberRole::Owner, ReceiveMode::Bootstrap)
            .is_err()
    );
}

#[test]
fn regular_pushes_can_only_update_the_authenticated_users_branch() {
    let user = Uuid::new_v4();
    let old = "1".repeat(40);
    let new = "2".repeat(40);
    let own = vec![update(&format!("refs/heads/user/{user}"), &old, &new)];
    assert!(validate_receive_updates(&own, user, MemberRole::Editor, ReceiveMode::Normal).is_ok());

    for forbidden in [
        "refs/heads/main".to_string(),
        format!("refs/heads/user/{}", Uuid::new_v4()),
        "refs/tags/v1".to_string(),
    ] {
        assert!(
            validate_receive_updates(
                &[update(&forbidden, &old, &new)],
                user,
                MemberRole::Owner,
                ReceiveMode::Normal
            )
            .is_err(),
            "allowed {forbidden}"
        );
    }
    assert!(
        validate_receive_updates(
            &[update(&format!("refs/heads/user/{user}"), &old, ZERO_OID)],
            user,
            MemberRole::Editor,
            ReceiveMode::Normal
        )
        .is_err()
    );
    assert!(validate_receive_updates(&own, user, MemberRole::Viewer, ReceiveMode::Normal).is_err());
}

#[test]
fn fast_forward_main_uses_expected_head_and_compare_and_swap() {
    let root = tempfile::tempdir().unwrap();
    let working = tempfile::tempdir().unwrap();
    let store = GitRepoStore::new(root.path()).unwrap();
    let space = Uuid::new_v4();
    let user = Uuid::new_v4();
    let user_branch = format!("user/{user}");
    store.ensure_space(space).unwrap();

    run(working.path(), &["init", "-b", "main"]);
    run(working.path(), &["config", "user.name", "Test"]);
    run(
        working.path(),
        &["config", "user.email", "test@cowiki.local"],
    );
    std::fs::write(working.path().join("index.md"), "# One\n").unwrap();
    run(working.path(), &["add", "index.md"]);
    run(working.path(), &["commit", "-m", "one"]);
    run(
        working.path(),
        &[
            "remote",
            "add",
            "cloud",
            store.repo_path(space).to_str().unwrap(),
        ],
    );
    run(working.path(), &["push", "cloud", "main"]);
    let base = rev(working.path(), "HEAD");

    run(working.path(), &["checkout", "-b", "user"]);
    std::fs::write(working.path().join("index.md"), "# Two\n").unwrap();
    run(working.path(), &["commit", "-am", "two"]);
    let head = rev(working.path(), "HEAD");
    run(
        working.path(),
        &["push", "cloud", &format!("HEAD:refs/heads/{user_branch}")],
    );

    assert!(store.fast_forward_main(space, &user_branch, &base).is_err());
    let merged = store.fast_forward_main(space, &user_branch, &head).unwrap();
    assert_eq!(merged.old_main_oid, base);
    assert_eq!(merged.main_oid, head);
    let retried = store.fast_forward_main(space, &user_branch, &head).unwrap();
    assert!(retried.already_merged);

    run(working.path(), &["checkout", "--orphan", "divergent"]);
    std::fs::write(working.path().join("index.md"), "# Divergent\n").unwrap();
    run(working.path(), &["add", "index.md"]);
    run(working.path(), &["commit", "-m", "divergent"]);
    let divergent = rev(working.path(), "HEAD");
    run(
        working.path(),
        &[
            "push",
            "--force",
            "cloud",
            &format!("HEAD:refs/heads/{user_branch}"),
        ],
    );
    let error = store
        .fast_forward_main(space, &user_branch, &divergent)
        .unwrap_err();
    assert!(error.to_string().contains("not based"));
}

fn run(directory: &std::path::Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .env("COWIKI_INTERNAL", "1")
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn rev(directory: &std::path::Path, revision: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", revision])
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
