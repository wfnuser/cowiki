//! Open Knowledge Format (OKF) v0.1 conventions for a local desktop Space.
//!
//! Markdown and Git remain authoritative. This module only normalizes files
//! created through CoWiki and maintains the progressive-disclosure indexes.

use serde_yaml::{Mapping, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

pub const OKF_VERSION: &str = "0.1";
pub const RAW_SOURCES_DIR: &str = ".cowiki/sources";
const GENERATED_START: &str = "<!-- cowiki:generated-index:start -->";
const GENERATED_END: &str = "<!-- cowiki:generated-index:end -->";
const LEGACY_ROOT_METADATA_HEADING: &str = "## Legacy root index metadata";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Concept,
    Index,
    Log,
    Other,
}

impl DocumentKind {
    pub fn from_path(path: &Path) -> Self {
        match path.file_name().and_then(|name| name.to_str()) {
            Some("index.md") => Self::Index,
            Some("log.md") => Self::Log,
            Some(name) if name.ends_with(".md") => Self::Concept,
            _ => Self::Other,
        }
    }
}

pub fn root_index() -> String {
    format!("---\nokf_version: \"{OKF_VERSION}\"\n---\n\n# Knowledge\n")
}

pub fn folder_index(title: &str) -> String {
    format!("# {title}\n")
}

pub fn concept_relative_path(slug: &str) -> Result<PathBuf, String> {
    let normalized = normalize_relative_path(slug)?;
    let without_extension = normalized.strip_suffix(".md").unwrap_or(&normalized);
    let filename = Path::new(without_extension)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "page must name a Markdown file".to_string())?;
    if matches!(filename, "index" | "log" | "_index") {
        return Err(format!("'{filename}' is reserved by OKF"));
    }
    Ok(PathBuf::from(format!("{without_extension}.md")))
}

pub fn source_storage_path(filename: &str) -> Result<PathBuf, String> {
    if filename.trim().is_empty()
        || filename.contains(['/', '\\'])
        || filename.starts_with('.')
        || matches!(filename, "." | "..")
    {
        return Err("Source filename must be a plain filename".to_string());
    }
    let reserved = matches!(filename, "index.md" | "log.md");
    if filename.ends_with(".md") && !reserved {
        return Ok(Path::new(RAW_SOURCES_DIR).join(filename));
    }
    let digest = format!("{:x}", Sha256::digest(filename.as_bytes()));
    Ok(Path::new(RAW_SOURCES_DIR)
        .join("_encoded")
        .join(format!("{digest}.md")))
}

pub fn normalize_concept_document(content: &str, fallback_title: &str) -> Result<String, String> {
    normalize_concept_document_as(content, fallback_title, "Note")
}

fn normalize_concept_document_as(
    content: &str,
    fallback_title: &str,
    default_type: &str,
) -> Result<String, String> {
    if concept_is_conforming(content) {
        return Ok(content.to_string());
    }
    let (mut mapping, body) = match split_frontmatter(content) {
        Ok((Some(raw), body)) => match serde_yaml::from_str::<Mapping>(raw) {
            Ok(mapping) => (mapping, body),
            Err(_) => (Mapping::new(), content),
        },
        Ok((None, body)) => (Mapping::new(), body),
        Err(_) => (Mapping::new(), content),
    };
    set_if_missing_or_blank(&mut mapping, "type", Value::String(default_type.into()));
    set_if_missing_or_blank(
        &mut mapping,
        "title",
        Value::String(fallback_title.to_string()),
    );
    let description = Value::String("description".into());
    if !mapping.contains_key(&description) {
        if let Some(summary) = mapping.get(Value::String("summary".into())).cloned() {
            mapping.insert(description, summary);
        }
    }
    let yaml = serde_yaml::to_string(&mapping)
        .map_err(|error| format!("cannot serialize concept frontmatter: {error}"))?;
    Ok(format!(
        "---\n{yaml}---\n\n{}",
        body.trim_start_matches(['\r', '\n'])
    ))
}

pub fn display_metadata(content: &str) -> (Option<String>, Option<String>) {
    let (frontmatter, body) = split_frontmatter(content).unwrap_or((None, content));
    let mapping = frontmatter.and_then(|raw| serde_yaml::from_str::<Mapping>(raw).ok());
    let field = |key: &str| {
        mapping
            .as_ref()
            .and_then(|mapping| mapping.get(Value::String(key.into())))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let heading = body
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    (
        field("title").or(heading),
        field("description").or_else(|| field("summary")),
    )
}

pub fn ensure_bundle(root: &Path) -> Result<(), String> {
    migrate_legacy_sources(root)?;
    normalize_documents(root)?;
    refresh_progressive_indexes(root)
}

pub fn ensure_supported_for_write(root: &Path) -> Result<(), String> {
    let index = root.join("index.md");
    if !index.is_file() {
        return Ok(());
    }
    let content = match std::fs::read_to_string(index) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    let version = split_frontmatter(&content)
        .ok()
        .and_then(|(frontmatter, _)| frontmatter)
        .and_then(|raw| serde_yaml::from_str::<Mapping>(raw).ok())
        .and_then(|mapping| mapping.get(Value::String("okf_version".into())).cloned())
        .and_then(|value| value.as_str().map(str::to_string));
    if version.as_deref().is_some_and(|value| value != OKF_VERSION) {
        return Err(format!(
            "This Space targets OKF {}; CoWiki can read it but will not rewrite it as {OKF_VERSION}.",
            version.unwrap_or_default()
        ));
    }
    Ok(())
}

pub fn needs_migration(root: &Path) -> Result<bool, String> {
    if !root.join("index.md").is_file() {
        return Ok(true);
    }
    let root_index = match std::fs::read_to_string(root.join("index.md")) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(true),
        Err(error) => return Err(error.to_string()),
    };
    let frontmatter = split_frontmatter(&root_index)
        .ok()
        .and_then(|(frontmatter, _)| frontmatter);
    let root_mapping = frontmatter.and_then(|raw| serde_yaml::from_str::<Mapping>(raw).ok());
    let declared_version = root_mapping
        .as_ref()
        .and_then(|mapping| mapping.get(Value::String("okf_version".into())))
        .and_then(Value::as_str);
    if declared_version.is_some_and(|version| version != OKF_VERSION) {
        // §11: consume future versions best-effort; never rewrite them as 0.1.
        return Ok(false);
    }
    if legacy_sources_need_migration(root)? {
        return Ok(true);
    }
    let root_ok = root_mapping.is_some_and(|mapping| {
        mapping.len() == 1
            && mapping
                .get(Value::String("okf_version".into()))
                .and_then(Value::as_str)
                .is_some_and(|version| version == OKF_VERSION)
            && index_body_is_conforming(&root_index)
    });
    if !root_ok {
        return Ok(true);
    }
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == root
                || is_visible_document_path(root, entry.path())
                || is_source_path(root, entry.path())
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if entry.file_type().is_dir()
            && is_visible_document_path(root, path)
            && !path.join("index.md").is_file()
        {
            return Ok(true);
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(true),
            Err(error) => return Err(error.to_string()),
        };
        let invalid = match DocumentKind::from_path(relative) {
            DocumentKind::Concept => !concept_is_conforming(&content),
            DocumentKind::Index => {
                !index_body_is_conforming(&content)
                    || (relative != Path::new("index.md") && content.starts_with("---"))
                    || (content.contains(GENERATED_START) && !content.contains(GENERATED_END))
            }
            DocumentKind::Log => !log_is_conforming(&content),
            _ => false,
        };
        if invalid {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether opening must rewrite existing user-authored bytes. Missing indexes
/// and a missing optional version declaration are maintenance, not conformance
/// failures, and must never block a dirty-but-conforming bundle.
pub fn needs_content_migration(root: &Path) -> Result<bool, String> {
    if legacy_sources_need_migration(root)? {
        return Ok(true);
    }
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == root
                || is_visible_document_path(root, entry.path())
                || is_source_path(root, entry.path())
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(true),
            Err(error) => return Err(error.to_string()),
        };
        let invalid = match DocumentKind::from_path(relative) {
            DocumentKind::Concept => !concept_is_conforming(&content),
            DocumentKind::Index if relative == Path::new("index.md") => {
                !root_index_is_conforming(&content)
            }
            DocumentKind::Index => {
                !index_body_is_conforming(&content) || content.starts_with("---")
            }
            DocumentKind::Log => !log_is_conforming(&content),
            DocumentKind::Other => false,
        };
        if invalid {
            return Ok(true);
        }
    }
    Ok(false)
}

fn normalize_documents(root: &Path) -> Result<(), String> {
    let files = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == root
                || is_visible_document_path(root, entry.path())
                || is_source_path(root, entry.path())
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    for path in files {
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        let kind = DocumentKind::from_path(relative);
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
                let archive = archive_legacy_bytes(root, relative, &bytes)?;
                let normalized = match kind {
                    DocumentKind::Concept => {
                        let fallback = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let default_type = if is_source_path(root, &path) {
                            "Source"
                        } else {
                            "Note"
                        };
                        let body = format!(
                            "The original non-UTF-8 bytes are preserved at `{}`.\n",
                            archive
                                .strip_prefix(root)
                                .unwrap_or(&archive)
                                .to_string_lossy()
                        );
                        normalize_concept_document_as(&body, &fallback, default_type)?
                    }
                    DocumentKind::Index if relative == Path::new("index.md") => root_index(),
                    DocumentKind::Index => folder_index(
                        relative
                            .parent()
                            .and_then(Path::file_name)
                            .unwrap_or_default()
                            .to_string_lossy()
                            .as_ref(),
                    ),
                    DocumentKind::Log => replacement_log(relative),
                    DocumentKind::Other => continue,
                };
                std::fs::write(path, normalized).map_err(|error| error.to_string())?;
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        let normalized = match kind {
            DocumentKind::Concept => {
                if concept_is_conforming(&content) {
                    continue;
                }
                let fallback = display_metadata(&content).0.unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                });
                let default_type = if is_source_path(root, &path) {
                    "Source"
                } else {
                    "Note"
                };
                normalize_concept_document_as(&content, &fallback, default_type)?
            }
            DocumentKind::Index if relative == Path::new("index.md") => {
                normalize_root_index(&content)?
            }
            DocumentKind::Index => normalize_nested_index(relative, &content)?,
            DocumentKind::Log if !log_is_conforming(&content) => {
                archive_legacy_document(root, relative, &content)?;
                replacement_log(relative)
            }
            _ => continue,
        };
        if normalized != content {
            std::fs::write(path, normalized).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn concept_is_conforming(content: &str) -> bool {
    let Ok((Some(raw), _)) = split_frontmatter(content) else {
        return false;
    };
    serde_yaml::from_str::<Mapping>(raw)
        .ok()
        .and_then(|mapping| mapping.get(Value::String("type".into())).cloned())
        .and_then(|value| value.as_str().map(str::to_string))
        .is_some_and(|value| !value.trim().is_empty())
}

fn index_body_is_conforming(content: &str) -> bool {
    split_frontmatter(content)
        .map(|(_, body)| body.lines().any(|line| line.starts_with("# ")))
        .unwrap_or(false)
}

fn root_index_is_conforming(content: &str) -> bool {
    let Ok((frontmatter, body)) = split_frontmatter(content) else {
        return false;
    };
    if !body.lines().any(|line| line.starts_with("# ")) {
        return false;
    }
    let Some(raw) = frontmatter else {
        return true;
    };
    let Ok(mapping) = serde_yaml::from_str::<Mapping>(raw) else {
        return false;
    };
    mapping
        .keys()
        .all(|key| key.as_str() == Some("okf_version"))
        && mapping
            .get(Value::String("okf_version".into()))
            .is_none_or(|value| {
                value
                    .as_str()
                    .is_some_and(|version| !version.trim().is_empty())
            })
}

fn normalize_root_index(content: &str) -> Result<String, String> {
    let (mut legacy_mapping, body) = match split_frontmatter(content) {
        Ok((Some(raw), body)) => match serde_yaml::from_str::<Mapping>(raw) {
            Ok(mapping) => (mapping, body),
            Err(_) => (Mapping::new(), content),
        },
        Ok((None, body)) => (Mapping::new(), body),
        Err(_) => (Mapping::new(), content),
    };
    legacy_mapping.remove(Value::String("okf_version".into()));
    let mut mapping = Mapping::new();
    mapping.insert(
        Value::String("okf_version".into()),
        Value::String(OKF_VERSION.into()),
    );
    let mut body = body.trim_start_matches(['\r', '\n']).to_string();
    if !body.lines().any(|line| line.starts_with("# ")) {
        body = if body.trim().is_empty() {
            "# Knowledge\n".into()
        } else {
            format!("# Knowledge\n\n{body}")
        };
    }
    if !legacy_mapping.is_empty() && !body.contains(LEGACY_ROOT_METADATA_HEADING) {
        let legacy_yaml = serde_yaml::to_string(&legacy_mapping)
            .map_err(|error| format!("cannot preserve root index metadata: {error}"))?;
        body = format!(
            "{}\n\n{LEGACY_ROOT_METADATA_HEADING}\n\n```yaml\n{legacy_yaml}```\n",
            body.trim_end()
        );
    }
    let yaml = serde_yaml::to_string(&mapping).map_err(|error| error.to_string())?;
    Ok(format!("---\n{yaml}---\n\n{body}"))
}

fn normalize_nested_index(path: &Path, content: &str) -> Result<String, String> {
    let (mapping, body) = match split_frontmatter(content) {
        Ok((Some(raw), body)) => match serde_yaml::from_str::<Mapping>(raw) {
            Ok(mapping) => (Some(mapping), body),
            Err(_) => (None, content),
        },
        Ok((None, body)) => (None, body),
        Err(_) => (None, content),
    };
    let title = mapping
        .as_ref()
        .and_then(|mapping| mapping.get(Value::String("title".into())))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
    let description = mapping.as_ref().and_then(|mapping| {
        mapping
            .get(Value::String("description".into()))
            .or_else(|| mapping.get(Value::String("summary".into())))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    });
    let mut body = body.trim_start_matches(['\r', '\n']).to_string();
    if !body.lines().any(|line| line.starts_with("# ")) {
        body = if body.trim().is_empty() {
            format!("# {title}\n")
        } else {
            format!("# {title}\n\n{body}")
        };
    }
    if let Some(description) = description {
        if !body.contains(description) {
            body = format!("{}\n\n{description}\n", body.trim_end());
        }
    }
    Ok(body)
}

fn log_is_conforming(content: &str) -> bool {
    if content.starts_with("---") {
        return false;
    }
    let mut lines = content.lines().filter(|line| !line.trim().is_empty());
    let Some(title) = lines.next() else {
        return false;
    };
    if !title.starts_with("# ") || title.starts_with("## ") {
        return false;
    }

    let mut previous_date: Option<&str> = None;
    let mut saw_date = false;
    let mut group_has_entry = false;
    for line in lines {
        if let Some(date) = line.strip_prefix("## ") {
            if (saw_date && !group_has_entry) || !valid_iso_date(date) {
                return false;
            }
            if previous_date.is_some_and(|previous| date >= previous) {
                return false;
            }
            previous_date = Some(date);
            saw_date = true;
            group_has_entry = false;
        } else if line.starts_with("* ") || line.starts_with("- ") || line.starts_with("+ ") {
            if !saw_date {
                return false;
            }
            group_has_entry = true;
        } else if !(saw_date && group_has_entry && line.starts_with(char::is_whitespace)) {
            return false;
        }
    }
    saw_date && group_has_entry
}

fn valid_iso_date(value: &str) -> bool {
    if value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || !value
            .chars()
            .enumerate()
            .all(|(index, character)| matches!(index, 4 | 7) || character.is_ascii_digit())
    {
        return false;
    }
    let year = value[..4].parse::<u32>().unwrap_or_default();
    let month = value[5..7].parse::<u32>().unwrap_or_default();
    let day = value[8..].parse::<u32>().unwrap_or_default();
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day > 0 && day <= days
}

fn archive_legacy_document(root: &Path, relative: &Path, content: &str) -> Result<(), String> {
    archive_legacy_bytes(root, relative, content.as_bytes()).map(|_| ())
}

fn archive_legacy_bytes(root: &Path, relative: &Path, content: &[u8]) -> Result<PathBuf, String> {
    let preferred = root
        .join(".cowiki/legacy")
        .join(relative)
        .with_extension("md.legacy");
    let mut archive = preferred.clone();
    let mut suffix = 1;
    while archive.exists() {
        archive = PathBuf::from(format!("{}.{}", preferred.to_string_lossy(), suffix));
        suffix += 1;
    }
    if let Some(parent) = archive.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&archive, content).map_err(|error| error.to_string())?;
    Ok(archive)
}

fn replacement_log(relative: &Path) -> String {
    let date = time::OffsetDateTime::now_utc().date();
    format!(
        "# Directory Update Log\n\n## {date}\n* **Migration**: Preserved the legacy log from `{}` under `.cowiki/legacy/`.\n",
        relative.to_string_lossy()
    )
}

fn is_visible_document_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .all(|component| !component.as_os_str().to_string_lossy().starts_with('.'))
}

fn is_source_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).ok().is_some_and(|relative| {
        relative == Path::new(".cowiki") || relative.starts_with(RAW_SOURCES_DIR)
    })
}

pub fn refresh_progressive_indexes(root: &Path) -> Result<(), String> {
    let mut directories = BTreeSet::new();
    directories.insert(root.to_path_buf());
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| entry.path() == root || !is_hidden_relative(root, entry.path()))
        .filter_map(Result::ok)
    {
        if entry.file_type().is_dir() {
            directories.insert(entry.path().to_path_buf());
        }
    }
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|directory| std::cmp::Reverse(directory.components().count()));
    for directory in directories {
        refresh_directory_index(root, &directory)?;
    }
    Ok(())
}

fn refresh_directory_index(root: &Path, directory: &Path) -> Result<(), String> {
    let index_path = directory.join("index.md");
    let existing = if index_path.is_file() {
        std::fs::read_to_string(&index_path).map_err(|error| error.to_string())?
    } else if directory == root {
        root_index()
    } else {
        folder_index(&directory.file_name().unwrap_or_default().to_string_lossy())
    };
    let mut entries = Vec::new();
    for child in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let child = child.map_err(|error| error.to_string())?;
        let path = child.path();
        let file_type = child.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        let name = child.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if file_type.is_dir() {
            let child_index = path.join("index.md");
            let title = std::fs::read_to_string(child_index)
                .ok()
                .and_then(|body| display_metadata(&body).0)
                .unwrap_or_else(|| name.clone());
            entries.push((title, format!("{name}/"), None));
        } else if DocumentKind::from_path(&path) == DocumentKind::Concept {
            let body = std::fs::read_to_string(&path).unwrap_or_default();
            let (title, description) = display_metadata(&body);
            entries.push((
                title.unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                }),
                name,
                description,
            ));
        }
    }
    entries.sort_by_key(|left| left.0.to_lowercase());
    let existing = if directory == root {
        normalize_root_index(&existing)?
    } else {
        existing
    };
    let updated = update_generated_listing(&existing, &entries)?;
    if updated != existing {
        std::fs::write(index_path, updated).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn update_generated_listing(
    content: &str,
    entries: &[(String, String, Option<String>)],
) -> Result<String, String> {
    let mut listing = GENERATED_START.to_string();
    for (title, target, description) in entries {
        listing.push_str("\n* [");
        listing.push_str(&escape_label(title));
        listing.push_str("](<");
        listing.push_str(&escape_target(target));
        listing.push_str(">)");
        if let Some(description) = description
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            listing.push_str(" - ");
            listing.push_str(&escape_text(description));
        }
    }
    listing.push('\n');
    listing.push_str(GENERATED_END);
    if let Some(start) = content.find(GENERATED_START) {
        let after = &content[start..];
        let end_offset = after
            .find(GENERATED_END)
            .ok_or_else(|| "generated index block has no closing marker".to_string())?;
        let end = start + end_offset + GENERATED_END.len();
        Ok(format!(
            "{}{}{}",
            &content[..start],
            listing,
            &content[end..]
        ))
    } else {
        Ok(format!("{}\n\n{listing}\n", content.trim_end()))
    }
}

fn migrate_legacy_sources(root: &Path) -> Result<(), String> {
    let legacy = root.join("sources");
    let target = root.join(RAW_SOURCES_DIR);
    if !legacy_sources_need_migration(root)? {
        return Ok(());
    }
    std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
    let files = walkdir::WalkDir::new(&legacy)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    for source in files {
        let relative = source
            .strip_prefix(&legacy)
            .map_err(|error| error.to_string())?
            .to_path_buf();
        let filename = relative
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let encoded = !filename.ends_with(".md") || matches!(filename, "index.md" | "log.md");
        let destination = if !encoded {
            relative.clone()
        } else {
            let digest = format!(
                "{:x}",
                Sha256::digest(relative.to_string_lossy().as_bytes())
            );
            Path::new("_encoded").join(format!("{digest}.md"))
        };
        let destination = target.join(destination);
        if destination.exists() {
            let content = std::fs::read(&source).map_err(|error| error.to_string())?;
            archive_legacy_bytes(
                root,
                Path::new("source-collisions").join(&relative).as_path(),
                &content,
            )?;
            std::fs::remove_file(&source).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::rename(&source, &destination).map_err(|error| error.to_string())?;
        if encoded {
            normalize_migrated_encoded_source(root, &relative, &destination)?;
        }
    }
    std::fs::remove_dir_all(legacy).map_err(|error| error.to_string())
}

fn legacy_sources_need_migration(root: &Path) -> Result<bool, String> {
    if declared_root_version(root)?.is_some() {
        return Ok(false);
    }
    let legacy = root.join("sources");
    if !legacy.is_dir() {
        return Ok(false);
    }
    for entry in walkdir::WalkDir::new(legacy)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(true),
            Err(error) => return Err(error.to_string()),
        };
        let conforming = match DocumentKind::from_path(entry.path()) {
            DocumentKind::Concept => concept_is_conforming(&content),
            DocumentKind::Index => {
                !content.starts_with("---") && index_body_is_conforming(&content)
            }
            DocumentKind::Log => log_is_conforming(&content),
            DocumentKind::Other => false,
        };
        if !conforming {
            return Ok(true);
        }
    }
    Ok(false)
}

fn declared_root_version(root: &Path) -> Result<Option<String>, String> {
    let content = match std::fs::read_to_string(root.join("index.md")) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    Ok(split_frontmatter(&content)
        .ok()
        .and_then(|(frontmatter, _)| frontmatter)
        .and_then(|raw| serde_yaml::from_str::<Mapping>(raw).ok())
        .and_then(|mapping| mapping.get(Value::String("okf_version".into())).cloned())
        .and_then(|value| value.as_str().map(str::to_string)))
}

fn normalize_migrated_encoded_source(
    root: &Path,
    original_relative: &Path,
    destination: &Path,
) -> Result<(), String> {
    let bytes = std::fs::read(destination).map_err(|error| error.to_string())?;
    let original_name = original_relative.to_string_lossy();
    let body = match std::str::from_utf8(&bytes) {
        Ok(content) => content.to_string(),
        Err(_) => {
            let archive = archive_legacy_bytes(
                root,
                Path::new("source-binaries")
                    .join(original_relative)
                    .as_path(),
                &bytes,
            )?;
            format!(
                "The original non-UTF-8 Source bytes are preserved at `{}`.\n",
                archive
                    .strip_prefix(root)
                    .unwrap_or(&archive)
                    .to_string_lossy()
            )
        }
    };
    let normalized = normalize_concept_document_as(&body, &original_name, "Source")?;
    std::fs::write(destination, normalized).map_err(|error| error.to_string())
}

fn is_hidden_relative(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
}

fn normalize_relative_path(path: &str) -> Result<String, String> {
    if path.is_empty() || Path::new(path).is_absolute() || path.contains('\\') {
        return Err("path must be a non-empty Space-relative path".into());
    }
    let mut segments = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment
                    .to_str()
                    .ok_or_else(|| "path must be valid UTF-8".to_string())?;
                if segment.starts_with('.') {
                    return Err("hidden paths are reserved for CoWiki internals".into());
                }
                segments.push(segment);
            }
            _ => return Err("path may not escape the Space".into()),
        }
    }
    Ok(segments.join("/"))
}

fn split_frontmatter(content: &str) -> Result<(Option<&str>, &str), String> {
    let Some(after_opening) = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))
    else {
        return Ok((None, content));
    };
    let mut offset = 0;
    for line in after_opening.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Ok((
                Some(&after_opening[..offset]),
                &after_opening[offset + line.len()..],
            ));
        }
        offset += line.len();
    }
    Err("frontmatter has no closing delimiter".into())
}

fn set_if_missing_or_blank(mapping: &mut Mapping, key: &str, fallback: Value) {
    let key = Value::String(key.into());
    let missing = mapping
        .get(&key)
        .and_then(Value::as_str)
        .is_none_or(|value| value.trim().is_empty());
    if missing {
        mapping.insert(key, fallback);
    }
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace(['\r', '\n'], " ")
}

fn escape_label(value: &str) -> String {
    escape_text(value)
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn escape_target(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('?', "%3F")
        .replace('#', "%23")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_index_keeps_only_the_version_key_without_losing_legacy_metadata() {
        let original = "---\nokf_version: \"0.1\"\ntitle: Old home\nowner: docs\n---\n\n# Home\n";
        let normalized = normalize_root_index(original).expect("normalize root index");
        let (frontmatter, body) = split_frontmatter(&normalized).expect("parse root index");
        let mapping =
            serde_yaml::from_str::<Mapping>(frontmatter.expect("frontmatter")).expect("parse YAML");

        assert_eq!(mapping.len(), 1);
        assert_eq!(
            mapping
                .get(Value::String("okf_version".into()))
                .and_then(Value::as_str),
            Some("0.1")
        );
        assert!(body.contains("title: Old home"));
        assert!(body.contains("owner: docs"));
        assert_eq!(normalize_root_index(&normalized).unwrap(), normalized);
    }

    #[test]
    fn logs_require_every_date_group_to_be_valid_flat_and_newest_first() {
        assert!(log_is_conforming(
            "# Updates\n\n## 2026-07-14\n* New\n\n## 2026-07-13\n- Old\n"
        ));
        assert!(!log_is_conforming(
            "# Updates\n\n## 2026-07-13\n* Old\n\n## 2026-07-14\n* New\n"
        ));
        assert!(!log_is_conforming(
            "# Updates\n\n## 2026-02-30\n* Impossible date\n"
        ));
        assert!(!log_is_conforming(
            "# Updates\n\n## 2026-07-14\nNo list entry\n"
        ));
        assert!(!log_is_conforming(
            "# Updates\n\n## 2026-07-14\n* Good\n\n## someday\n* Invalid group\n"
        ));
    }

    #[test]
    fn indexes_need_a_section_heading_and_future_versions_are_not_rewritten() {
        let missing_heading = tempfile::tempdir().unwrap();
        std::fs::write(
            missing_heading.path().join("index.md"),
            "---\nokf_version: \"0.1\"\n---\n",
        )
        .unwrap();
        assert!(needs_migration(missing_heading.path()).unwrap());
        ensure_bundle(missing_heading.path()).unwrap();
        assert!(
            std::fs::read_to_string(missing_heading.path().join("index.md"))
                .unwrap()
                .contains("# Knowledge")
        );

        let future = tempfile::tempdir().unwrap();
        let future_index = "---\nokf_version: \"0.2\"\n---\n\n# Future bundle\n";
        std::fs::write(future.path().join("index.md"), future_index).unwrap();
        std::fs::write(
            future.path().join("concept.md"),
            "---\ntype: Future concept\n---\n",
        )
        .unwrap();
        std::fs::create_dir_all(future.path().join("sources")).unwrap();
        std::fs::write(
            future.path().join("sources/topic.md"),
            "---\ntype: Note\n---\n\nFuture ordinary hierarchy.\n",
        )
        .unwrap();
        assert!(!needs_migration(future.path()).unwrap());
        assert_eq!(
            std::fs::read_to_string(future.path().join("index.md")).unwrap(),
            future_index
        );
        assert!(future.path().join("sources/topic.md").is_file());

        let current = tempfile::tempdir().unwrap();
        std::fs::write(current.path().join("index.md"), root_index()).unwrap();
        std::fs::create_dir_all(current.path().join("sources")).unwrap();
        std::fs::write(
            current.path().join("sources/topic.md"),
            "---\ntype: Note\n---\n\nCurrent ordinary hierarchy.\n",
        )
        .unwrap();
        std::fs::write(
            current.path().join("sources/draft.md"),
            "# Malformed but still ordinary hierarchy.\n",
        )
        .unwrap();
        ensure_bundle(current.path()).unwrap();
        assert!(current.path().join("sources/topic.md").is_file());
        assert!(
            std::fs::read_to_string(current.path().join("sources/draft.md"))
                .unwrap()
                .contains("type: Note")
        );
        assert!(!current.path().join(RAW_SOURCES_DIR).exists());
    }

    #[test]
    fn non_utf8_reserved_documents_are_archived_and_replaced_by_their_own_kind() {
        let bundle = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(bundle.path().join("docs")).unwrap();
        std::fs::write(bundle.path().join("index.md"), [0xff]).unwrap();
        std::fs::write(bundle.path().join("docs/index.md"), [0xfe]).unwrap();
        std::fs::write(bundle.path().join("docs/log.md"), [0xfd]).unwrap();

        assert!(needs_migration(bundle.path()).unwrap());
        ensure_bundle(bundle.path()).unwrap();

        let root = std::fs::read_to_string(bundle.path().join("index.md")).unwrap();
        let nested = std::fs::read_to_string(bundle.path().join("docs/index.md")).unwrap();
        let log = std::fs::read_to_string(bundle.path().join("docs/log.md")).unwrap();
        assert!(root.contains("okf_version: '0.1'") || root.contains("okf_version: \"0.1\""));
        assert!(!nested.starts_with("---"));
        assert!(nested.starts_with("# docs"));
        assert!(!log.starts_with("---"));
        assert!(log.starts_with("# Directory Update Log"));
        assert_eq!(
            std::fs::read(bundle.path().join(".cowiki/legacy/index.md.legacy")).unwrap(),
            [0xff]
        );
        assert_eq!(
            std::fs::read(bundle.path().join(".cowiki/legacy/docs/index.md.legacy")).unwrap(),
            [0xfe]
        );
        assert_eq!(
            std::fs::read(bundle.path().join(".cowiki/legacy/docs/log.md.legacy")).unwrap(),
            [0xfd]
        );
    }
}
