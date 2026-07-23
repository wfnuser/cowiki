use cowiki_cloud::config::Config;
use std::collections::HashMap;

fn valid_environment(repo_root: &str) -> HashMap<String, String> {
    HashMap::from([
        (
            "DATABASE_URL".to_string(),
            "postgres://cowiki:secret@localhost/cowiki".to_string(),
        ),
        ("COWIKI_REPO_ROOT".to_string(), repo_root.to_string()),
        (
            "COWIKI_PUBLIC_ORIGIN".to_string(),
            "https://cloud.cowiki.app".to_string(),
        ),
        ("GITHUB_CLIENT_ID".to_string(), "client-id".to_string()),
        (
            "GITHUB_CLIENT_SECRET".to_string(),
            "client-secret".to_string(),
        ),
        (
            "COWIKI_TOKEN_PEPPER".to_string(),
            "0123456789abcdef0123456789abcdef".to_string(),
        ),
    ])
}

#[test]
fn production_configuration_is_strict_and_normalized() {
    let temp = tempfile::tempdir().unwrap();
    let environment = valid_environment(temp.path().to_str().unwrap());
    let config = Config::from_values(environment).unwrap();

    assert_eq!(config.database_url.scheme(), "postgres");
    assert_eq!(config.repo_root, temp.path());
    assert_eq!(config.public_origin.as_str(), "https://cloud.cowiki.app/");
    assert_eq!(config.bind_addr.to_string(), "0.0.0.0:8787");
}

#[test]
fn missing_database_url_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let mut environment = valid_environment(temp.path().to_str().unwrap());
    environment.remove("DATABASE_URL");

    let error = Config::from_values(environment).unwrap_err();
    assert!(error.to_string().contains("DATABASE_URL"));
}

#[test]
fn sqlite_and_relative_repository_roots_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let mut sqlite = valid_environment(temp.path().to_str().unwrap());
    sqlite.insert("DATABASE_URL".into(), "sqlite://cloud.db".into());
    assert!(
        Config::from_values(sqlite)
            .unwrap_err()
            .to_string()
            .contains("PostgreSQL")
    );

    let relative = valid_environment("var/lib/cowiki");
    assert!(
        Config::from_values(relative)
            .unwrap_err()
            .to_string()
            .contains("absolute")
    );
}

#[test]
fn malformed_origin_and_short_pepper_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let mut origin = valid_environment(temp.path().to_str().unwrap());
    origin.insert("COWIKI_PUBLIC_ORIGIN".into(), "file:///tmp/cloud".into());
    assert!(
        Config::from_values(origin)
            .unwrap_err()
            .to_string()
            .contains("http")
    );

    for value in [
        "https://user:secret@cloud.cowiki.app",
        "https://cloud.cowiki.app/prefix",
        "https://cloud.cowiki.app?tenant=one",
    ] {
        let mut origin = valid_environment(temp.path().to_str().unwrap());
        origin.insert("COWIKI_PUBLIC_ORIGIN".into(), value.into());
        assert!(
            Config::from_values(origin).is_err(),
            "accepted origin {value}"
        );
    }

    let mut pepper = valid_environment(temp.path().to_str().unwrap());
    pepper.insert("COWIKI_TOKEN_PEPPER".into(), "short".into());
    assert!(
        Config::from_values(pepper)
            .unwrap_err()
            .to_string()
            .contains("32 bytes")
    );
}

#[test]
fn migration_encodes_roles_and_one_open_pr_per_branch() {
    let migration = include_str!("../migrations/001_control_plane.sql");

    for role in ["owner", "manager", "editor", "viewer"] {
        assert!(migration.contains(role), "missing role {role}");
    }
    assert!(migration.contains("CREATE UNIQUE INDEX one_open_pr_per_head"));
    assert!(migration.contains("WHERE status = 'open'"));
    assert!(!migration.to_lowercase().contains("sqlite"));
}
