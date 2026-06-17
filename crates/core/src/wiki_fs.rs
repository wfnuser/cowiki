//! WikiFs — directory-aware wrapper around WikiRepo.
//!
//! Provides write/read/list operations scoped to known content directories
//! ("wiki", "entities", "concepts") with a string+whitelist design:
//! adding a 4th directory = adding one string to CONTENT_DIRS.

use crate::git::WikiRepo;

/// Known content directories. String + whitelist, not enum — easy to extend.
const CONTENT_DIRS: &[&str] = &["wiki", "entities", "concepts", "sources"];

/// Validate a content directory path against the known whitelist.
/// Accepts top-level dirs ("wiki", "entities", "concepts") and
/// subdirectory paths under them ("wiki/messi", "entities/people").
/// Accepts with or without trailing slash.
pub fn validate_dir(dir: &str) -> Result<&str, String> {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        return Err("dir must not be empty".into());
    }
    if dir.contains("..") {
        return Err(format!("dir contains path traversal: {dir}"));
    }
    // Check exact match
    if CONTENT_DIRS.contains(&dir) {
        return Ok(dir);
    }
    // Check subdirectory under a known content dir
    for root in CONTENT_DIRS {
        if dir.starts_with(&format!("{root}/")) {
            return Ok(dir);
        }
    }
    Err(format!(
        "unknown content dir: {dir}. valid: {} or subdirs under them",
        CONTENT_DIRS.join(", ")
    ))
}

/// Validate a page slug against path traversal and injection attacks.
///
/// Slugs may contain `/` to represent nested subdirectories under a content dir
/// (e.g. slug `projects/my-page` writes to `wiki/projects/my-page.md`).
///
/// Rejects: empty, `..` (traversal), leading `/` (absolute), `\\` (Windows sep),
/// null bytes, and segments that are exactly `..`.
pub fn validate_slug(slug: &str) -> Result<&str, String> {
    if slug.is_empty() {
        return Err("slug must not be empty".into());
    }
    if slug.contains("..") {
        return Err(format!("slug contains path traversal: {slug}"));
    }
    if slug.starts_with('/') {
        return Err(format!("slug must not start with '/': {slug}"));
    }
    if slug.contains('\\') {
        return Err(format!("slug contains backslash: {slug}"));
    }
    if slug.contains('\0') {
        return Err("slug contains null byte".into());
    }
    // Also reject slugs that end with / (would create a directory, not a file)
    if slug.ends_with('/') {
        return Err(format!("slug must not end with '/': {slug}"));
    }
    Ok(slug)
}

/// Return all known content directories.
pub fn all_dirs() -> &'static [&'static str] {
    CONTENT_DIRS
}

/// Write a page under a content directory.
pub fn write_page(
    repo: &WikiRepo,
    branch: &str,
    dir: &str,
    slug: &str,
    content: &[u8],
    author: &str,
) -> Result<(), String> {
    let dir = validate_dir(dir)?;
    validate_slug(slug)?;
    let path = format!("{dir}/{slug}.md");
    repo.write_file(branch, &path, content, &format!("edit: {slug}"), author)
        .map_err(|e| format!("write error: {e}"))
}

/// Read a page from a content directory.
pub fn read_page(
    repo: &WikiRepo,
    branch: &str,
    dir: &str,
    slug: &str,
) -> Result<Option<Vec<u8>>, String> {
    let dir = validate_dir(dir)?;
    validate_slug(slug)?;
    let path = format!("{dir}/{slug}.md");
    repo.read_file(branch, &path)
        .map_err(|e| format!("read error: {e}"))
}

/// Remove a page from a content directory.
pub fn remove_page(
    repo: &WikiRepo,
    branch: &str,
    dir: &str,
    slug: &str,
    author: &str,
) -> Result<(), String> {
    let dir = validate_dir(dir)?;
    validate_slug(slug)?;
    let path = format!("{dir}/{slug}.md");
    repo.delete_path(branch, &path, &format!("remove: {slug}"), author)
        .map_err(|e| format!("remove error: {e}"))
}

/// List pages in a content directory (flat).
pub fn list_pages(repo: &WikiRepo, branch: &str, dir: &str) -> Result<Vec<String>, String> {
    let dir = validate_dir(dir)?;
    repo.list_files(branch, dir)
        .map_err(|e| format!("list error: {e}"))
}

/// List pages recursively in a content directory.
pub fn list_pages_recursive(
    repo: &WikiRepo,
    branch: &str,
    dir: &str,
) -> Result<Vec<String>, String> {
    let dir = validate_dir(dir)?;
    repo.list_files_recursive(branch, dir)
        .map_err(|e| format!("list error: {e}"))
}

/// List across all content directories (recursive).
/// Returns an error only if ALL directories fail; individual
/// directory failures (e.g., empty dir not yet created) are tolerated.
pub fn list_all_dirs(repo: &WikiRepo, branch: &str) -> Result<Vec<String>, String> {
    let mut all = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for dir in CONTENT_DIRS {
        match repo.list_files_recursive(branch, dir) {
            Ok(files) => all.extend(files),
            Err(e) => errors.push(format!("{dir}: {e}")),
        }
    }
    if all.is_empty() && !errors.is_empty() {
        Err(format!("all content dirs failed: {}", errors.join("; ")))
    } else {
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::WikiRepo;

    fn temp_repo() -> (WikiRepo, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = WikiRepo::open_or_init(&tmp.path().to_string_lossy()).expect("open_or_init");
        (repo, tmp)
    }

    #[test]
    fn test_nested_path_write_and_read() {
        let (repo, _tmp) = temp_repo();

        // Write a page at a nested path: wiki/messy/mengnan-dating-guide.md
        write_page(
            &repo,
            "main",
            "wiki",
            "messy/mengnan-dating-guide",
            b"# Guide\n\ncontent here",
            "test",
        )
        .expect("write nested page should succeed");

        // Read it back
        let content = read_page(&repo, "main", "wiki", "messy/mengnan-dating-guide")
            .expect("read should succeed")
            .expect("page should exist");

        let body = String::from_utf8_lossy(&content);
        assert!(body.contains("Guide"), "body should contain title: {body}");
        assert!(
            body.contains("content here"),
            "body should contain content: {body}"
        );
    }

    #[test]
    fn test_nested_path_deeply_nested() {
        let (repo, _tmp) = temp_repo();

        write_page(
            &repo,
            "main",
            "wiki",
            "a/b/c/d/deep-page",
            b"# Deep",
            "test",
        )
        .expect("deeply nested write should succeed");

        let content = read_page(&repo, "main", "wiki", "a/b/c/d/deep-page")
            .expect("read should succeed")
            .expect("page should exist");

        assert_eq!(String::from_utf8_lossy(&content), "# Deep");
    }

    #[test]
    fn test_nested_path_entities_dir() {
        let (repo, _tmp) = temp_repo();

        write_page(
            &repo,
            "main",
            "entities",
            "people/alice",
            b"# Alice",
            "test",
        )
        .expect("nested entities write should succeed");

        let content = read_page(&repo, "main", "entities", "people/alice")
            .expect("read should succeed")
            .expect("page should exist");

        assert_eq!(String::from_utf8_lossy(&content), "# Alice");
    }

    #[test]
    fn test_nested_path_creates_intermediate_dirs() {
        let (repo, _tmp) = temp_repo();

        // Write to a path where neither intermediate dirs exist
        write_page(&repo, "main", "wiki", "foo/bar/baz/qux", b"# Qux", "test")
            .expect("nested write with mkdir -p should succeed");

        // Recursive listing should find the nested file
        let files = list_pages_recursive(&repo, "main", "wiki").expect("list should succeed");

        assert!(
            files.iter().any(|f| f.contains("foo/bar/baz/qux.md")),
            "should find nested file in listing: {files:?}"
        );
    }

    #[test]
    fn test_validate_dir_valid() {
        assert!(validate_dir("wiki").is_ok());
        assert!(validate_dir("entities").is_ok());
        assert!(validate_dir("concepts").is_ok());
    }

    #[test]
    fn test_validate_dir_subdirectory() {
        assert!(validate_dir("wiki/messi").is_ok());
        assert!(validate_dir("entities/people").is_ok());
        assert!(validate_dir("concepts/patterns/error").is_ok());
        assert!(validate_dir("wiki/a/b/c").is_ok());
    }

    #[test]
    fn test_validate_dir_trailing_slash() {
        assert!(validate_dir("wiki/").is_ok());
        assert!(validate_dir("entities/").is_ok());
        assert!(validate_dir("wiki/messi/").is_ok());
    }

    #[test]
    fn test_validate_dir_invalid() {
        assert!(validate_dir("invalid").is_err());
        assert!(validate_dir("").is_err());
        assert!(validate_dir("../etc").is_err());
        assert!(validate_dir("all").is_err());
        assert!(validate_dir("wiki/../etc").is_err());
    }

    #[test]
    fn test_validate_slug_valid() {
        assert!(validate_slug("my-page").is_ok());
        assert!(validate_slug("hello-world-123").is_ok());
        assert!(validate_slug("a").is_ok());
    }

    #[test]
    fn test_validate_slug_nested_path() {
        // Nested subdirectories are allowed
        assert!(validate_slug("projects/my-page").is_ok());
        assert!(validate_slug("a/b/c").is_ok());
        assert!(validate_slug("deep/nested/path/page").is_ok());
    }

    #[test]
    fn test_validate_slug_empty() {
        assert!(validate_slug("").is_err());
    }

    #[test]
    fn test_validate_slug_path_traversal() {
        assert!(validate_slug("../etc").is_err());
        assert!(validate_slug("foo/../../bar").is_err());
        assert!(validate_slug("..").is_err());
    }

    #[test]
    fn test_validate_slug_absolute_path() {
        assert!(validate_slug("/etc/passwd").is_err());
    }

    #[test]
    fn test_validate_slug_backslash() {
        assert!(validate_slug("foo\\bar").is_err());
    }

    #[test]
    fn test_validate_slug_trailing_slash() {
        assert!(validate_slug("foo/").is_err());
    }

    #[test]
    fn test_validate_slug_null_byte() {
        assert!(validate_slug("foo\0bar").is_err());
    }

    #[test]
    fn test_all_dirs_contains_expected() {
        let dirs = all_dirs();
        assert!(dirs.contains(&"wiki"));
        assert!(dirs.contains(&"entities"));
        assert!(dirs.contains(&"concepts"));
        assert!(dirs.contains(&"sources"));
        assert_eq!(dirs.len(), 4);
    }
}
