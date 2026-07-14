use cowiki_core::okf::{
    concept_path, folder_index_path, migrate_legacy_path, normalize_concept_document, root_index,
    source_body, source_document, source_path, update_index_entries, validate_bundle,
    validate_document, BundleFile, DocumentKind, IndexEntry, OKF_VERSION,
};
use std::path::Path;

#[test]
fn canonical_paths_follow_okf_bundle_rules() {
    assert_eq!(
        concept_path("architecture/overview").unwrap(),
        "architecture/overview.md"
    );
    assert_eq!(
        folder_index_path("architecture").unwrap(),
        "architecture/index.md"
    );
    assert_eq!(source_path("notes.md").unwrap(), ".cowiki/sources/notes.md");
    let encoded_source = source_path("interview.txt").unwrap();
    assert!(encoded_source.starts_with(".cowiki/sources/_encoded/"));
    assert!(encoded_source.ends_with(".md"));
    assert_ne!(source_path("foo").unwrap(), source_path("foo.md").unwrap());
    for reserved in ["index.md", "log.md"] {
        let path = source_path(reserved).unwrap();
        assert_eq!(DocumentKind::from_path(&path), DocumentKind::Concept);
        assert!(path.starts_with(".cowiki/sources/_encoded/"));
    }
    let long_name = format!("{}.txt", "界".repeat(80));
    let long_path = source_path(&long_name).unwrap();
    assert!(
        Path::new(&long_path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .len()
            <= 255
    );
    assert!(concept_path("index").is_err());
    assert!(concept_path("../secret").is_err());
}

#[test]
fn reserved_files_follow_the_full_v01_structure() {
    let issues = validate_document("index.md", b"Just prose.\n");
    assert_eq!(issues[0].rule, "index-heading");

    let issues = validate_document("docs/index.md", b"---\ntype: Index\n---\n# Docs\n");
    assert_eq!(issues[0].rule, "index-frontmatter");

    let issues = validate_document(
        "log.md",
        b"# Directory Update Log\n\n## 2026-07-13\n* Newer.\n\n## 2026-07-14\n* Older is in the wrong position.\n",
    );
    assert!(issues.iter().any(|issue| issue.rule == "log-order"));

    let issues = validate_document(
        "log.md",
        b"---\ntype: Log\n---\n\n# Directory Update Log\n\n## 2026-07-14\n* Updated.\n",
    );
    assert!(issues.iter().any(|issue| issue.rule == "log-frontmatter"));

    let issues = validate_document(
        "log.md",
        b"# Directory Update Log\n\n## 14 July 2026\nNo list.\n",
    );
    assert!(issues.iter().any(|issue| issue.rule == "log-date"));
    assert!(issues.iter().any(|issue| issue.rule == "log-entry"));

    let issues = validate_document(
        "log.md",
        b"# Directory Update Log\n\n## 2026-07-14\n- A standard Markdown bullet is valid.\n",
    );
    assert!(issues.is_empty(), "{issues:?}");

    let issues = validate_document(
        "log.md",
        b"# Directory Update Log\n\n## 2026-07-14\n  - Nested is not a flat list entry.\n",
    );
    assert!(issues.iter().any(|issue| issue.rule == "log-entry"));
}

#[test]
fn source_storage_is_an_additive_okf_concept_not_an_exception() {
    let document = source_document("interview.md", "Raw interview body.").unwrap();
    assert!(document.contains("type: Source"));
    assert!(validate_document(".cowiki/sources/interview.md", document.as_bytes()).is_empty());
    assert_eq!(source_body(&document).unwrap(), "Raw interview body.");
}

#[test]
fn bundle_validation_checks_every_markdown_document_including_hidden_directories() {
    let files = vec![
        BundleFile::new(
            "index.md",
            b"---\nokf_version: \"0.1\"\n---\n\n# Knowledge\n",
        ),
        BundleFile::new(
            "concepts/customer.md",
            b"---\ntype: Entity\nproducer_extension: yes\n---\n\n[Future](/missing.md)\n",
        ),
        BundleFile::new(".cowiki/sources/bad.md", b"raw source without frontmatter"),
    ];
    let result = validate_bundle(files);
    assert!(!result.is_conformant());
    assert_eq!(result.concepts, 2);
    assert!(result
        .issues
        .iter()
        .any(|issue| issue.path == ".cowiki/sources/bad.md" && issue.rule == "frontmatter"));
    assert!(!result
        .issues
        .iter()
        .any(|issue| issue.rule == "link-target"));
}

#[test]
fn bundle_validation_rejects_a_stale_present_index() {
    let result = validate_bundle(vec![
        BundleFile::new(
            "index.md",
            b"# Knowledge\n\nProse mentions architecture.md, but [a backup](architecture.md.bak) is not the concept.\n",
        ),
        BundleFile::new(
            "architecture.md",
            b"---\ntype: Note\ntitle: Architecture\n---\n",
        ),
        BundleFile::new("assets/logo.svg", b"<svg />"),
    ]);
    assert!(result
        .issues
        .iter()
        .any(|issue| issue.rule == "index-entry" && issue.path == "index.md"));
    assert!(!result
        .issues
        .iter()
        .any(|issue| issue.message.contains("assets/")));
}

#[test]
fn index_coverage_accepts_standard_markdown_link_variants() {
    let result = validate_bundle(vec![
        BundleFile::new(
            "index.md",
            b"# Knowledge\n\n* [Architecture](architecture.md \"Design notes\")\n",
        ),
        BundleFile::new("architecture.md", b"---\ntype: Note\n---\n"),
    ]);
    assert!(result.is_conformant(), "{:?}", result.issues);
}

#[test]
fn generated_indexes_escape_titles_and_support_markdown_sensitive_paths() {
    let root = update_index_entries(
        "index.md",
        &root_index(),
        &[IndexEntry {
            title: "My ]\nDocs".into(),
            target: "my docs/".into(),
            description: None,
        }],
    )
    .unwrap();
    let nested = update_index_entries(
        "my docs/index.md",
        "# My Docs\n",
        &[IndexEntry {
            title: "A [special] page".into(),
            target: "foo(bar)>?#.md".into(),
            description: None,
        }],
    )
    .unwrap();

    assert!(root.contains("[My \\] Docs](<my docs/>)"));
    assert!(nested.contains("[A \\[special\\] page](<foo(bar)%3E%3F%23.md>)"));
    let result = validate_bundle(vec![
        BundleFile::new("index.md", root.as_bytes()),
        BundleFile::new("my docs/index.md", nested.as_bytes()),
        BundleFile::new(
            "my docs/foo(bar)>?#.md",
            b"---\ntype: Note\ntitle: Special\n---\n",
        ),
    ]);
    assert!(result.is_conformant(), "{:?}", result.issues);
}

#[test]
fn generated_index_refresh_is_idempotent_when_metadata_contains_its_end_marker() {
    let marker = "<!-- cowiki:generated-index:end -->";
    let entries = vec![IndexEntry {
        title: format!("Marker {marker}"),
        target: "page.md".into(),
        description: Some(format!("Description {marker}")),
    }];
    let once = update_index_entries("index.md", &root_index(), &entries).unwrap();
    let twice = update_index_entries("index.md", &once, &entries).unwrap();

    assert_eq!(once, twice);
    assert_eq!(
        twice
            .matches("<!-- cowiki:generated-index:start -->")
            .count(),
        1
    );
    assert_eq!(twice.matches(marker).count(), 1);

    let human_prose = format!("{once}\nHuman explanation mentioning {marker}\n");
    let refreshed = update_index_entries("index.md", &human_prose, &entries).unwrap();
    assert!(refreshed.contains(&format!("Human explanation mentioning {marker}")));
}

#[test]
fn legacy_layout_has_a_lossless_migration_target() {
    assert_eq!(
        migrate_legacy_path("wiki/getting-started.md"),
        "getting-started.md"
    );
    assert_eq!(migrate_legacy_path("wiki/team/_index.md"), "team/index.md");
    assert_eq!(
        migrate_legacy_path("sources/source-a.md"),
        ".cowiki/sources/source-a.md"
    );
    assert_eq!(
        migrate_legacy_path("entities/customer.md"),
        "entities/customer.md"
    );
}

#[test]
fn concept_normalization_adds_required_type_and_preserves_extensions() {
    let legacy =
        "---\ntitle: Customer\nsummary: Existing summary\npage_id: stable-id\n---\n\n# Customer\n";
    let normalized = normalize_concept_document(legacy, "Customer").unwrap();

    assert!(normalized.contains("type: Note"));
    assert!(normalized.contains("title: Customer"));
    assert!(normalized.contains("description: Existing summary"));
    assert!(normalized.contains("summary: Existing summary"));
    assert!(normalized.contains("page_id: stable-id"));
    assert!(normalized.ends_with("# Customer\n"));
    assert!(validate_document("customer.md", normalized.as_bytes()).is_empty());
}

#[test]
fn validator_enforces_only_okf_v01_hard_requirements() {
    let issues = validate_document("concept.md", b"# Missing frontmatter\n");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule, "frontmatter");

    let issues = validate_document("concept.md", b"---\ntitle: Missing type\n---\n");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule, "type");

    let issues = validate_document(
        "concept.md",
        b"---\ntype: Custom Producer Type\nunknown: true\n---\n",
    );
    assert!(issues.is_empty());

    let root_index = format!("---\nokf_version: \"{OKF_VERSION}\"\n---\n\n# Knowledge\n");
    assert!(validate_document("index.md", root_index.as_bytes()).is_empty());
    assert!(validate_document("docs/index.md", b"# Docs\n").is_empty());
    assert!(validate_document("docs/log.md", b"# Log\n\n## 2026-07-14\n* Updated.\n").is_empty());
}

#[test]
fn document_kind_distinguishes_reserved_files_from_concepts() {
    assert_eq!(DocumentKind::from_path("index.md"), DocumentKind::Index);
    assert_eq!(DocumentKind::from_path("nested/log.md"), DocumentKind::Log);
    assert_eq!(
        DocumentKind::from_path("nested/page.md"),
        DocumentKind::Concept
    );
    assert_eq!(DocumentKind::from_path("asset.png"), DocumentKind::Other);
}
