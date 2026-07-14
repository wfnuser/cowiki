use cowiki_core::okf::{
    concept_path, folder_index_path, migrate_legacy_path, normalize_concept_document, source_body,
    source_document, source_path, validate_bundle, validate_document, BundleFile, DocumentKind,
    OKF_VERSION,
};

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
        b"# Directory Update Log\n\n## 14 July 2026\nNo list.\n",
    );
    assert!(issues.iter().any(|issue| issue.rule == "log-date"));
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
