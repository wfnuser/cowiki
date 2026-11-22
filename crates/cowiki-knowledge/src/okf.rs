//! Open Knowledge Format (OKF) v0.3 conventions for a local desktop Space
//! (backward-compatible with OKF v0.1 and v0.2 bundles).
//!
//! Markdown and Git remain authoritative. This module normalizes files created
//! through CoWiki, parses OKF v0.3 governance/trust metadata, validates bundle
//! conformance, and maintains progressive-disclosure indexes.

use serde::Serialize;
use serde_yaml::{Mapping, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

pub const OKF_VERSION: &str = "0.3";
pub const SUPPORTED_OKF_VERSIONS: &[&str] = &["0.1", "0.2", "0.3"];
pub const RAW_SOURCES_DIR: &str = ".cowiki/sources";
pub const VALID_STATUSES: &[&str] = &["active", "draft", "deprecated", "retired"];
pub const VALID_TRUST_TIERS: &[&str] = &[
    "human_verified",
    "human_authored",
    "agent_draft",
    "unverified",
];
const GENERATED_START: &str = "<!-- cowiki:generated-index:start -->";
const GENERATED_END: &str = "<!-- cowiki:generated-index:end -->";
const LEGACY_ROOT_METADATA_HEADING: &str = "## Legacy root index metadata";

pub fn is_supported_version(version: &str) -> bool {
    SUPPORTED_OKF_VERSIONS.contains(&version.trim())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OkfMetadata {
    pub doc_type: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub sources: Vec<String>,
    pub status: String,
    pub stale_after: Option<String>,
    pub trust_tier: String,
    pub verified: bool,
    pub generated: bool,
    pub redirect_to: Option<String>,
    pub updated_at: Option<String>,
    pub scope: String,
    pub version: String,
}

impl Default for OkfMetadata {
    fn default() -> Self {
        Self {
            doc_type: "Note".to_string(),
            title: None,
            description: None,
            tags: Vec::new(),
            aliases: Vec::new(),
            sources: Vec::new(),
            status: "active".to_string(),
            stale_after: None,
            trust_tier: "unverified".to_string(),
            verified: false,
            generated: false,
            redirect_to: None,
            updated_at: None,
            scope: "default".to_string(),
            version: "v1".to_string(),
        }
    }
}

impl OkfMetadata {
    pub fn is_human_verified(&self) -> bool {
        self.verified || self.trust_tier.eq_ignore_ascii_case("human_verified")
    }

    pub fn is_retired(&self) -> bool {
        self.status.eq_ignore_ascii_case("retired")
    }

    pub fn is_deprecated(&self) -> bool {
        self.status.eq_ignore_ascii_case("deprecated")
    }

    pub fn is_stale_as_of(&self, as_of: Option<&str>) -> bool {
        let Some(stale_after) = self.stale_after.as_deref() else {
            return false;
        };
        is_iso_date_expired(stale_after, as_of)
    }

    pub fn governance_multiplier(&self, as_of: Option<&str>, exclude_retired: bool) -> (f64, bool) {
        let is_stale = self.is_stale_as_of(as_of);
        if self.is_retired() {
            return (if exclude_retired { 0.0 } else { 0.25 }, is_stale);
        }
        let mut mult = 1.0_f64;
        if self.is_deprecated() {
            mult *= 0.42;
        } else if self.status.eq_ignore_ascii_case("draft") {
            mult *= 0.90;
        }
        if self.is_human_verified() {
            mult *= 1.08;
        } else if self.trust_tier.eq_ignore_ascii_case("agent_draft") {
            mult *= 0.95;
        }
        if is_stale {
            mult *= 0.65;
        }
        (mult, is_stale)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OkfValidationIssue {
    pub code: String,
    pub severity: String,
    pub rfc_level: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OkfValidationReport {
    pub okf_version: Option<String>,
    pub valid: bool,
    pub checked_files: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub summary_by_code: BTreeMap<String, usize>,
    pub issues: Vec<OkfValidationIssue>,
}

pub fn utc_today_iso() -> String {
    time::OffsetDateTime::now_utc().date().to_string()
}

pub(crate) fn is_iso_date_expired(stale_after: &str, as_of: Option<&str>) -> bool {
    let stale_after = stale_after.trim();
    if !valid_iso_date(stale_after) {
        return false;
    }
    let reference = as_of
        .map(str::trim)
        .filter(|value| valid_iso_date(value))
        .map(str::to_string)
        .unwrap_or_else(utc_today_iso);
    stale_after < reference.as_str()
}

pub fn parse_metadata(content: &str) -> OkfMetadata {
    let (frontmatter, body) = split_frontmatter(content).unwrap_or((None, content));
    let mapping = frontmatter.and_then(|raw| serde_yaml::from_str::<Mapping>(raw).ok());
    let string_field = |key: &str| -> Option<String> {
        mapping
            .as_ref()
            .and_then(|map| map.get(Value::String(key.into())))
            .and_then(yaml_scalar_to_string)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let list_field = |key: &str| -> Vec<String> {
        mapping
            .as_ref()
            .and_then(|map| map.get(Value::String(key.into())))
            .map(yaml_to_string_list)
            .unwrap_or_default()
    };
    let bool_field = |key: &str| -> Option<bool> {
        mapping
            .as_ref()
            .and_then(|map| map.get(Value::String(key.into())))
            .and_then(yaml_to_bool)
    };
    let heading = body
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let verified = bool_field("verified").unwrap_or(false);
    let trust_tier = string_field("trust_tier")
        .map(|value| value.to_lowercase())
        .unwrap_or_else(|| {
            if verified {
                "human_verified".to_string()
            } else {
                "unverified".to_string()
            }
        });
    let generated = bool_field("generated").unwrap_or_else(|| trust_tier == "agent_draft");
    let status = string_field("status")
        .map(|value| value.to_lowercase())
        .unwrap_or_else(|| "active".to_string());

    OkfMetadata {
        doc_type: string_field("type").unwrap_or_else(|| "Note".to_string()),
        title: string_field("title").or(heading),
        description: string_field("description").or_else(|| string_field("summary")),
        tags: list_field("tags"),
        aliases: list_field("aliases"),
        sources: list_field("sources"),
        status,
        stale_after: string_field("stale_after"),
        trust_tier,
        verified,
        generated,
        redirect_to: string_field("redirect_to"),
        updated_at: string_field("updated_at"),
        scope: string_field("scope").unwrap_or_else(|| "default".to_string()),
        version: string_field("version").unwrap_or_else(|| "v1".to_string()),
    }
}

/// Apply OKF v0.3 Agent-Edit Trust Reset to a Markdown document while
/// preserving all unknown user frontmatter keys and body content.
///
/// Any edit produced by an AI agent demotes `verified: true -> false`,
/// sets `trust_tier: agent_draft` and `generated: true`, updates `updated_at`,
/// and merges any newly cited provenance paths into `sources`.
pub fn apply_agent_edit_trust_reset(
    content: &str,
    fallback_title: &str,
    added_sources: &[String],
    updated_at: Option<&str>,
) -> Result<String, String> {
    let (mut mapping, body) = match split_frontmatter(content) {
        Ok((Some(raw), body)) => match serde_yaml::from_str::<Mapping>(raw) {
            Ok(mapping) => (mapping, body),
            Err(_) => (Mapping::new(), content),
        },
        Ok((None, body)) => (Mapping::new(), body),
        Err(_) => (Mapping::new(), content),
    };
    remove_legacy_concept_kind(&mut mapping);
    set_if_missing_or_blank(&mut mapping, "type", Value::String("Note".into()));
    let heading_title = display_metadata(content)
        .0
        .unwrap_or_else(|| fallback_title.to_string());
    set_if_missing_or_blank(&mut mapping, "title", Value::String(heading_title));

    let mut merged_sources = mapping
        .get(Value::String("sources".into()))
        .map(yaml_to_string_list)
        .unwrap_or_default();
    for source in added_sources {
        let trimmed = source.trim();
        if !trimmed.is_empty() && !merged_sources.iter().any(|existing| existing == trimmed) {
            merged_sources.push(trimmed.to_string());
        }
    }
    if !merged_sources.is_empty() {
        mapping.insert(
            Value::String("sources".into()),
            Value::Sequence(merged_sources.into_iter().map(Value::String).collect()),
        );
    }

    mapping.insert(Value::String("verified".into()), Value::Bool(false));
    mapping.insert(
        Value::String("trust_tier".into()),
        Value::String("agent_draft".into()),
    );
    mapping.insert(Value::String("generated".into()), Value::Bool(true));
    let date_value = updated_at
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(utc_today_iso);
    mapping.insert(
        Value::String("updated_at".into()),
        Value::String(date_value),
    );

    let yaml = serde_yaml::to_string(&mapping)
        .map_err(|error| format!("cannot serialize concept frontmatter: {error}"))?;
    Ok(format!(
        "---\n{yaml}---\n\n{}",
        body.trim_start_matches(['\r', '\n'])
    ))
}

fn yaml_scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn yaml_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn yaml_to_string_list(value: &Value) -> Vec<String> {
    match value {
        Value::Sequence(items) => items
            .iter()
            .filter_map(yaml_scalar_to_string)
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        Value::String(single) => single
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

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
    remove_legacy_concept_kind(&mut mapping);
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
        .and_then(|value| yaml_scalar_to_string(&value));
    if version
        .as_deref()
        .is_some_and(|value| !is_supported_version(value))
    {
        return Err(format!(
            "This Space targets OKF {}; CoWiki supports OKF up to {OKF_VERSION} and will not rewrite it.",
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
        .and_then(yaml_scalar_to_string);
    if declared_version
        .as_deref()
        .is_some_and(|version| !is_supported_version(version))
    {
        // §11: consume future versions best-effort; never rewrite them.
        return Ok(false);
    }
    if legacy_sources_need_migration(root)? {
        return Ok(true);
    }
    let root_ok = root_mapping.is_some_and(|mapping| {
        mapping.len() == 1
            && mapping
                .get(Value::String("okf_version".into()))
                .and_then(yaml_scalar_to_string)
                .is_some_and(|version| is_supported_version(&version))
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
        // Attachments (images, PDFs, recordings) are not OKF documents.
        if matches!(DocumentKind::from_path(relative), DocumentKind::Other) {
            continue;
        }
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
        // Attachments (images, PDFs, recordings) are not OKF documents.
        if matches!(DocumentKind::from_path(relative), DocumentKind::Other) {
            continue;
        }
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
        // Attachments are left alone; reading them as text would archive a
        // copy of every image or PDF under .cowiki/legacy.
        if matches!(kind, DocumentKind::Other) {
            continue;
        }
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
    let Ok(mapping) = serde_yaml::from_str::<Mapping>(raw) else {
        return false;
    };
    if has_legacy_concept_kind(&mapping) {
        return false;
    }
    mapping
        .get(Value::String("type".into()))
        .cloned()
        .and_then(|value| value.as_str().map(str::to_string))
        .is_some_and(|value| !value.trim().is_empty())
}

fn has_legacy_concept_kind(mapping: &Mapping) -> bool {
    mapping
        .get(Value::String("kind".into()))
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("concept"))
}

fn remove_legacy_concept_kind(mapping: &mut Mapping) {
    if has_legacy_concept_kind(mapping) {
        mapping.remove(Value::String("kind".into()));
    }
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
    let preserved_version = legacy_mapping
        .remove(Value::String("okf_version".into()))
        .and_then(|value| yaml_scalar_to_string(&value))
        .map(|value| value.trim().to_string())
        .filter(|value| is_supported_version(value))
        .unwrap_or_else(|| OKF_VERSION.to_string());
    let mut mapping = Mapping::new();
    mapping.insert(
        Value::String("okf_version".into()),
        Value::String(preserved_version),
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

pub fn valid_iso_date(value: &str) -> bool {
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
    refresh_progressive_indexes_with(root, |_, _| {})
}

fn index_metadata(path: &Path) -> io::Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_symlink() => Err(io::Error::other(format!(
            "refusing symbolic link at index {}",
            path.display()
        ))),
        Ok(meta) if meta.is_file() => Ok(Some(meta)),
        Ok(_) => Err(io::Error::other(format!(
            "not a regular index file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn open_existing_index(path: &Path, writable: bool) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(writable);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("refusing a non-regular index file"));
    }
    Ok(file)
}

fn read_index(path: &Path) -> io::Result<Option<String>> {
    if index_metadata(path)?.is_none() {
        return Ok(None);
    }
    let mut text = String::new();
    open_existing_index(path, false)?.read_to_string(&mut text)?;
    Ok(Some(text))
}

#[cfg(target_os = "macos")]
fn copy_index_security(source: &std::fs::File, target: &std::fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    use std::os::macos::fs::MetadataExt;

    // Darwin copyfile merges an ACL with the sibling's inherited entries.
    // Instead set the *exact* extended ACL, including the no-ACL case.
    const ACL_TYPE_EXTENDED: libc::c_int = 0x100;
    unsafe extern "C" {
        fn acl_get_fd_np(fd: libc::c_int, ty: libc::c_int) -> *mut libc::c_void;
        fn acl_init(count: libc::c_int) -> *mut libc::c_void;
        fn acl_set_fd_np(fd: libc::c_int, acl: *mut libc::c_void, ty: libc::c_int) -> libc::c_int;
        fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
    }
    // SAFETY: both FDs are live regular files. The Darwin ACL object is freed
    // exactly once after acl_set_fd_np; neither operation consumes it.
    unsafe {
        let mut acl = acl_get_fd_np(source.as_raw_fd(), ACL_TYPE_EXTENDED);
        if acl.is_null() {
            let error = io::Error::last_os_error();
            // Darwin returns ENOENT for an open file without an extended ACL.
            if error.raw_os_error() != Some(libc::ENOENT) {
                return Err(error);
            }
            acl = acl_init(0);
            if acl.is_null() {
                return Err(io::Error::last_os_error());
            }
        }
        let applied = acl_set_fd_np(target.as_raw_fd(), acl, ACL_TYPE_EXTENDED);
        let error = (applied != 0).then(io::Error::last_os_error);
        acl_free(acl);
        if let Some(error) = error {
            return Err(error);
        }
        if libc::fcopyfile(
            source.as_raw_fd(),
            target.as_raw_fd(),
            std::ptr::null_mut(),
            libc::COPYFILE_STAT | libc::COPYFILE_XATTR,
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    let rights = |meta: std::fs::Metadata| {
        (
            meta.st_uid(),
            meta.st_gid(),
            meta.st_mode(),
            meta.st_flags(),
        )
    };
    if rights(source.metadata()?) != rights(target.metadata()?) {
        return Err(io::Error::other(
            "cannot preserve index ownership, mode or file flags",
        ));
    }
    Ok(())
}

// On macOS copy security metadata before writing any content to the sibling.
// Elsewhere keep an existing inode (and its native ACL) until we can copy
// security descriptors there too. Never replace protected files with mode alone.
fn replace_index(path: &Path, text: &str, record_change: impl FnOnce()) -> io::Result<()> {
    let original = index_metadata(path)?
        .map(|_| open_existing_index(path, true))
        .transpose()?;
    #[cfg(not(target_os = "macos"))]
    if let Some(mut existing) = original.as_ref() {
        existing.set_len(0)?;
        record_change();
        return existing.write_all(text.as_bytes());
    }
    let mut builder = tempfile::Builder::new();
    builder.prefix(".cowiki-index-");
    #[cfg(unix)]
    if original.is_none() {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666)); // filtered by umask
    }
    let mut pending = builder.tempfile_in(path.parent().expect("index has a directory"))?;
    #[cfg(target_os = "macos")]
    if let Some(existing) = &original {
        copy_index_security(existing, pending.as_file())?;
    }
    pending.write_all(text.as_bytes())?;
    if original.is_some() {
        index_metadata(path)?;
        pending.persist(path)?;
    } else {
        pending.persist_noclobber(path)?;
    }
    record_change();
    Ok(())
}

/// Reviewing a conforming draft only updates generated listings. Keep their
/// prior text, not a Git snapshot of the entire Space, for error recovery.
pub fn refresh_review_indexes(root: &Path) -> Result<(), String> {
    let mut originals = Vec::new();
    let result = refresh_progressive_indexes_with(root, |path, previous| {
        originals.push((path.to_path_buf(), previous));
    });
    if let Err(mut error) = result {
        for (path, previous) in originals.into_iter().rev() {
            let restored = match previous {
                Some(text) => replace_index(&path, &text, || {}),
                None => index_metadata(&path).and_then(|metadata| {
                    if metadata.is_some() {
                        std::fs::remove_file(&path)
                    } else {
                        Ok(())
                    }
                }),
            };
            if let Err(cause) = restored {
                error.push_str(&format!("; could not restore {}: {cause}", path.display()));
            }
        }
        return Err(error);
    }
    Ok(())
}

fn refresh_progressive_indexes_with(
    root: &Path,
    mut record_change: impl FnMut(&Path, Option<String>),
) -> Result<(), String> {
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
        refresh_directory_index(root, &directory, &mut record_change)?;
    }
    Ok(())
}

/// Refresh only the progressive indexes that list `relative`: its folder and
/// every ancestor up to the Space root. A page write cannot change any other
/// folder's listing, so walking the whole tree on every save is unnecessary.
pub fn refresh_indexes_for(root: &Path, relative: &Path) -> Result<(), String> {
    if is_hidden_relative(root, &root.join(relative)) {
        return Ok(());
    }
    let mut directory = root.join(relative);
    while directory.pop() {
        if !directory.starts_with(root) {
            break;
        }
        if directory.is_dir() {
            refresh_directory_index(root, &directory, |_, _| {})?;
        }
        if directory == root {
            break;
        }
    }
    Ok(())
}

fn refresh_directory_index(
    root: &Path,
    directory: &Path,
    record_change: impl FnOnce(&Path, Option<String>),
) -> Result<(), String> {
    let index_path = directory.join("index.md");
    let previous = read_index(&index_path).map_err(|error| error.to_string())?;
    let had_index = previous.is_some();
    let existing = previous.unwrap_or_else(|| {
        if directory == root {
            root_index()
        } else {
            folder_index(&directory.file_name().unwrap_or_default().to_string_lossy())
        }
    });
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
            let title = read_index(&child_index)
                .map_err(|error| error.to_string())?
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
    let updated = if directory == root {
        update_generated_listing(&normalize_root_index(&existing)?, &entries)?
    } else {
        update_generated_listing(&existing, &entries)?
    };
    if updated != existing {
        replace_index(&index_path, &updated, || {
            record_change(&index_path, had_index.then_some(existing))
        })
        .map_err(|error| error.to_string())?;
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
        if source.file_name().is_some_and(|name| name == ".gitkeep")
            && std::fs::read(&source)
                .map_err(|error| error.to_string())?
                .iter()
                .all(u8::is_ascii_whitespace)
        {
            std::fs::remove_file(&source).map_err(|error| error.to_string())?;
            continue;
        }
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

pub fn declared_version(root: &Path) -> Result<Option<String>, String> {
    declared_root_version(root)
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
        .and_then(|value| yaml_scalar_to_string(&value))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

/// Validate a local CoWiki Space against OKF v0.3 RFC-2119 conformance rules
/// (read-only; never mutates any file in the Space).
pub fn validate_bundle(
    root: &Path,
    as_of: Option<&str>,
    check_links: bool,
) -> Result<OkfValidationReport, String> {
    if !root.is_dir() {
        return Err("Space directory does not exist".to_string());
    }
    let reference_date = as_of
        .map(str::trim)
        .filter(|value| valid_iso_date(value))
        .map(str::to_string)
        .unwrap_or_else(utc_today_iso);

    let mut issues: Vec<OkfValidationIssue> = Vec::new();
    let mut push_issue =
        |code: &str, severity: &str, rfc_level: &str, path: &str, message: String| {
            issues.push(OkfValidationIssue {
                code: code.to_string(),
                severity: severity.to_string(),
                rfc_level: rfc_level.to_string(),
                path: path.to_string(),
                message,
            });
        };

    let root_index_path = root.join("index.md");
    let mut declared_ver: Option<String> = None;
    match std::fs::symlink_metadata(&root_index_path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            push_issue(
                "missing_root_index",
                "error",
                "MUST",
                "index.md",
                "Space root MUST contain an 'index.md' entry document.".to_string(),
            );
        }
        Err(error) => {
            push_issue(
                "unreadable_root_index",
                "error",
                "MUST",
                "index.md",
                format!("Cannot inspect root 'index.md': {error}"),
            );
        }
        Ok(meta) if meta.file_type().is_symlink() => {
            push_issue(
                "symlink_root_index",
                "error",
                "MUST",
                "index.md",
                "Root 'index.md' MUST be a regular file, not a symbolic link.".to_string(),
            );
        }
        Ok(meta) if !meta.file_type().is_file() => {
            push_issue(
                "missing_root_index",
                "error",
                "MUST",
                "index.md",
                "Space root 'index.md' MUST be a regular Markdown file.".to_string(),
            );
        }
        Ok(_) => match std::fs::read_to_string(&root_index_path) {
            Err(_) => {
                push_issue(
                    "unreadable_utf8",
                    "error",
                    "MUST",
                    "index.md",
                    "Root 'index.md' MUST be valid UTF-8.".to_string(),
                );
            }
            Ok(root_text) => {
                match split_frontmatter(&root_text) {
                    Err(error) => {
                        push_issue(
                            "invalid_frontmatter",
                            "error",
                            "MUST",
                            "index.md",
                            format!("Root 'index.md' has malformed frontmatter: {error}"),
                        );
                    }
                    Ok((None, body)) => {
                        push_issue(
                            "missing_okf_version",
                            "error",
                            "MUST",
                            "index.md",
                            "Root 'index.md' MUST declare 'okf_version' in YAML frontmatter."
                                .to_string(),
                        );
                        if !body.lines().any(|line| line.starts_with("# ")) {
                            push_issue(
                                "index_missing_heading",
                                "error",
                                "MUST",
                                "index.md",
                                "Index document MUST include an H1 heading.".to_string(),
                            );
                        }
                    }
                    Ok((Some(raw), body)) => {
                        match serde_yaml::from_str::<Mapping>(raw) {
                            Ok(map) => {
                                declared_ver = map
                                    .get(Value::String("okf_version".into()))
                                    .and_then(yaml_scalar_to_string)
                                    .map(|value| value.trim().to_string())
                                    .filter(|value| !value.is_empty());
                                match declared_ver.as_deref() {
                                    None => push_issue(
                                        "missing_okf_version",
                                        "error",
                                        "MUST",
                                        "index.md",
                                        "Root 'index.md' frontmatter MUST include non-empty 'okf_version'."
                                            .to_string(),
                                    ),
                                    Some(ver) if !is_supported_version(ver) => push_issue(
                                        "unsupported_okf_version",
                                        "warning",
                                        "SHOULD",
                                        "index.md",
                                        format!(
                                            "Declared okf_version '{ver}' is newer than supported versions ({}).",
                                            SUPPORTED_OKF_VERSIONS.join(", ")
                                        ),
                                    ),
                                    Some(_) => {}
                                }
                            }
                            Err(error) => push_issue(
                                "invalid_frontmatter",
                                "error",
                                "MUST",
                                "index.md",
                                format!("Root 'index.md' has invalid YAML frontmatter: {error}"),
                            ),
                        }
                        if !body.lines().any(|line| line.starts_with("# ")) {
                            push_issue(
                                "index_missing_heading",
                                "error",
                                "MUST",
                                "index.md",
                                "Index document MUST include an H1 heading.".to_string(),
                            );
                        }
                    }
                }
                if root_text.contains(GENERATED_START) && !root_text.contains(GENERATED_END) {
                    push_issue(
                        "unclosed_generated_index_marker",
                        "error",
                        "MUST",
                        "index.md",
                        "Index document has opening generated-index marker without closing marker."
                            .to_string(),
                    );
                }
            }
        },
    }

    let file_index = crate::knowledge_index::FileIndex::scan(root)?;
    let live_files: HashSet<String> = file_index.files().iter().cloned().collect();
    let mut live_dirs: HashSet<String> = HashSet::new();
    let mut markdown_files: Vec<(String, PathBuf)> = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == root
                || is_visible_document_path(root, entry.path())
                || is_source_path(root, entry.path())
        })
        .filter_map(Result::ok)
    {
        if entry.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        if path == root {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let rel_str = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if entry.file_type().is_dir() {
            live_dirs.insert(rel_str);
        } else if entry.file_type().is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            markdown_files.push((rel_str, path.to_path_buf()));
        }
    }
    markdown_files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut redirects: HashMap<String, String> = HashMap::new();
    for (rel, full_path) in &markdown_files {
        let content = match std::fs::read_to_string(full_path) {
            Ok(content) => content,
            Err(_) => {
                push_issue(
                    "unreadable_utf8",
                    "error",
                    "MUST",
                    rel,
                    "Markdown document MUST be valid UTF-8.".to_string(),
                );
                continue;
            }
        };

        let kind = DocumentKind::from_path(Path::new(rel));
        match kind {
            DocumentKind::Index => {
                if rel != "index.md" {
                    if !index_body_is_conforming(&content) {
                        push_issue(
                            "index_missing_heading",
                            "error",
                            "MUST",
                            rel,
                            "Nested index document MUST include an H1 heading.".to_string(),
                        );
                    }
                    if content.contains(GENERATED_START) && !content.contains(GENERATED_END) {
                        push_issue(
                            "unclosed_generated_index_marker",
                            "error",
                            "MUST",
                            rel,
                            "Nested index has unclosed generated-index marker.".to_string(),
                        );
                    }
                }
                continue;
            }
            DocumentKind::Log => {
                if !log_is_conforming(&content) {
                    push_issue(
                        "invalid_log_format",
                        "warning",
                        "SHOULD",
                        rel,
                        "Log document SHOULD follow descending ISO date heading format without frontmatter."
                            .to_string(),
                    );
                }
                continue;
            }
            DocumentKind::Other => continue,
            DocumentKind::Concept => {}
        }

        let (raw_frontmatter, body) = match split_frontmatter(&content) {
            Ok((Some(raw), body)) => (raw, body),
            Ok((None, _)) => {
                push_issue(
                    "missing_frontmatter",
                    "error",
                    "MUST",
                    rel,
                    "Concept document MUST begin with YAML frontmatter.".to_string(),
                );
                continue;
            }
            Err(error) => {
                push_issue(
                    "invalid_frontmatter",
                    "error",
                    "MUST",
                    rel,
                    format!("Malformed YAML frontmatter: {error}"),
                );
                continue;
            }
        };

        let mapping = match serde_yaml::from_str::<Mapping>(raw_frontmatter) {
            Ok(mapping) => mapping,
            Err(error) => {
                push_issue(
                    "invalid_frontmatter",
                    "error",
                    "MUST",
                    rel,
                    format!("Invalid YAML frontmatter: {error}"),
                );
                continue;
            }
        };

        let doc_type = mapping
            .get(Value::String("type".into()))
            .and_then(yaml_scalar_to_string)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if doc_type.is_none() {
            push_issue(
                "missing_required_field",
                "error",
                "MUST",
                rel,
                "Concept frontmatter MUST declare a non-empty 'type' field.".to_string(),
            );
        }

        let has_title = mapping
            .get(Value::String("title".into()))
            .and_then(yaml_scalar_to_string)
            .is_some_and(|value| !value.trim().is_empty());
        let has_h1 = body.lines().any(|line| line.starts_with("# "));
        if !has_title && !has_h1 {
            push_issue(
                "missing_required_field",
                "error",
                "MUST",
                rel,
                "Concept document MUST declare 'title' in frontmatter or an H1 heading."
                    .to_string(),
            );
        }

        if let Some(status_raw) = mapping
            .get(Value::String("status".into()))
            .and_then(yaml_scalar_to_string)
        {
            let lowered = status_raw.trim().to_ascii_lowercase();
            if !VALID_STATUSES.contains(&lowered.as_str()) {
                push_issue(
                    "invalid_status",
                    "error",
                    "MUST",
                    rel,
                    format!(
                        "Invalid status '{status_raw}'; MUST be one of: {}.",
                        VALID_STATUSES.join(", ")
                    ),
                );
            }
        }

        let trust_lower = mapping
            .get(Value::String("trust_tier".into()))
            .and_then(yaml_scalar_to_string)
            .map(|value| value.trim().to_ascii_lowercase());
        if let Some(ref trust) = trust_lower {
            if !VALID_TRUST_TIERS.contains(&trust.as_str()) {
                push_issue(
                    "invalid_trust_tier",
                    "error",
                    "MUST",
                    rel,
                    format!(
                        "Invalid trust_tier '{trust}'; MUST be one of: {}.",
                        VALID_TRUST_TIERS.join(", ")
                    ),
                );
            }
        }

        let verified = mapping
            .get(Value::String("verified".into()))
            .and_then(yaml_to_bool)
            .unwrap_or(false);
        let generated = mapping
            .get(Value::String("generated".into()))
            .and_then(yaml_to_bool)
            .unwrap_or(false);
        if verified && (trust_lower.as_deref() == Some("agent_draft") || generated) {
            push_issue(
                "trust_reset_violation",
                "error",
                "MUST",
                rel,
                "Agent-generated or agent_draft document MUST NOT carry 'verified: true'."
                    .to_string(),
            );
        }

        if let Some(stale_raw) = mapping
            .get(Value::String("stale_after".into()))
            .and_then(yaml_scalar_to_string)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            if !valid_iso_date(&stale_raw) {
                push_issue(
                    "invalid_stale_after",
                    "error",
                    "MUST",
                    rel,
                    format!(
                        "'stale_after' value '{stale_raw}' MUST be a valid ISO date YYYY-MM-DD."
                    ),
                );
            } else if stale_raw.as_str() < reference_date.as_str() {
                push_issue(
                    "expired_stale_after",
                    "warning",
                    "SHOULD",
                    rel,
                    format!("Document expired on {stale_raw} (as of {reference_date})."),
                );
            }
        }

        if let Some(redirect_raw) = mapping
            .get(Value::String("redirect_to".into()))
            .and_then(yaml_scalar_to_string)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            let resolved = crate::knowledge_index::resolve_markdown_target(rel, &redirect_raw);
            match resolved {
                Some(target) if live_files.contains(&target) => {
                    redirects.insert(rel.clone(), target);
                }
                _ => {
                    push_issue(
                        "broken_redirect",
                        "error",
                        "MUST",
                        rel,
                        format!(
                            "'redirect_to' target '{redirect_raw}' does not resolve to an existing document."
                        ),
                    );
                }
            }
        }

        if check_links && !rel.starts_with(".cowiki/") {
            for (target, kind) in crate::knowledge_index::extract_links(body, rel) {
                let target_trimmed = target.trim_end_matches('/');
                let resolved = match kind {
                    crate::knowledge_index::LinkKind::Markdown => {
                        live_files.contains(&target) || live_dirs.contains(target_trimmed)
                    }
                    crate::knowledge_index::LinkKind::Wiki => {
                        live_files.contains(&target)
                            || file_index.resolve_wikilink(&target, rel).is_some()
                    }
                };
                if !resolved && !target.split('/').any(|part| part.starts_with('.')) {
                    push_issue(
                        "broken_link",
                        "warning",
                        "SHOULD",
                        rel,
                        format!("Unresolved internal link target '{target}'."),
                    );
                }
            }
        }
    }

    let mut sorted_redirects: Vec<(&String, &String)> = redirects.iter().collect();
    sorted_redirects.sort_by(|left, right| left.0.cmp(right.0));
    for (src, first_next) in sorted_redirects {
        let mut seen_chain: HashSet<&str> = HashSet::new();
        seen_chain.insert(src.as_str());
        let mut current = first_next.as_str();
        while let Some(next) = redirects.get(current) {
            if !seen_chain.insert(current) {
                push_issue(
                    "redirect_cycle",
                    "error",
                    "MUST",
                    src,
                    format!("Redirect chain from '{src}' forms a cycle involving '{current}'."),
                );
                break;
            }
            current = next.as_str();
        }
    }

    let mut summary_by_code: BTreeMap<String, usize> = BTreeMap::new();
    let mut error_count = 0usize;
    let mut warning_count = 0usize;
    for issue in &issues {
        *summary_by_code.entry(issue.code.clone()).or_insert(0) += 1;
        if issue.severity == "error" {
            error_count += 1;
        } else {
            warning_count += 1;
        }
    }

    Ok(OkfValidationReport {
        okf_version: declared_ver,
        valid: error_count == 0,
        checked_files: markdown_files.len(),
        error_count,
        warning_count,
        summary_by_code,
        issues,
    })
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

/// Canonical Space-root path containment check rejecting null bytes, absolute
/// paths, `..` traversal, and any intermediate or terminal symlink escaping `root`.
pub fn safe_resolve_under_space(root: &Path, page_path: &str) -> Result<(String, PathBuf), String> {
    let trimmed = page_path.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return Err("path must be a non-empty string without null bytes".to_string());
    }
    let normalized = trimmed.replace('\\', "/");
    if normalized.starts_with('/') || (normalized.len() >= 2 && normalized.as_bytes()[1] == b':') {
        return Err(format!("absolute path is outside the Space: {page_path}"));
    }
    let mut parts: Vec<&str> = Vec::new();
    for segment in normalized.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return Err(format!("path traversal is forbidden: {page_path}")),
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return Err("path must reference a file inside the Space".to_string());
    }
    let root_real = std::fs::canonicalize(root)
        .map_err(|error| format!("cannot resolve Space root: {error}"))?;
    let mut cursor = root_real.clone();
    for part in &parts {
        cursor.push(part);
        if let Ok(meta) = std::fs::symlink_metadata(&cursor) {
            if meta.file_type().is_symlink() {
                let resolved = std::fs::canonicalize(&cursor).map_err(|_| {
                    format!("symlink component cannot be resolved safely: {page_path}")
                })?;
                if !resolved.starts_with(&root_real) {
                    return Err(format!("symlink escapes Space root: {page_path}"));
                }
            }
        }
    }
    if cursor.exists() {
        let final_real = std::fs::canonicalize(&cursor)
            .map_err(|error| format!("cannot canonicalize Space path: {error}"))?;
        if !final_real.starts_with(&root_real) {
            return Err(format!("resolved path escapes Space root: {page_path}"));
        }
        let canonical_rel = final_real
            .strip_prefix(&root_real)
            .map_err(|_| format!("resolved path escapes Space root: {page_path}"))?
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        Ok((canonical_rel, final_real))
    } else {
        Ok((parts.join("/"), cursor))
    }
}

pub fn split_frontmatter(content: &str) -> Result<(Option<&str>, &str), String> {
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
    #[cfg(target_os = "macos")]
    fn refreshing_or_restoring_an_index_preserves_its_macos_acl_and_xattrs() {
        use std::process::Command;
        for (rollback, has_acl) in [(false, true), (true, true), (false, false)] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            std::fs::create_dir(root.join("child")).unwrap();
            let index = root.join("child/index.md");
            let original = "# Handwritten\n";
            std::fs::write(&index, original).unwrap();
            if has_acl {
                assert!(Command::new("/bin/chmod")
                    .args(["+a", "user:daemon deny read"])
                    .arg(&index)
                    .status()
                    .unwrap()
                    .success());
            }
            assert!(Command::new("/usr/bin/xattr")
                .args(["-w", "org.cowiki.test", "keep-me"])
                .arg(&index)
                .status()
                .unwrap()
                .success());
            // A sibling created later can inherit a broader ACL from its folder.
            assert!(Command::new("/bin/chmod")
                .args(["+a", "user:nobody allow read,file_inherit"])
                .arg(root.join("child"))
                .status()
                .unwrap()
                .success());
            if rollback {
                std::fs::write(
                    root.join("index.md"),
                    "# Root\n<!-- cowiki:generated-index:start -->\n",
                )
                .unwrap();
            }

            let result = super::refresh_review_indexes(root);
            assert_eq!(result.is_err(), rollback);
            let acl = Command::new("/bin/ls")
                .arg("-le")
                .arg(&index)
                .output()
                .unwrap();
            let acl = String::from_utf8(acl.stdout).unwrap();
            assert_eq!(
                acl.contains("user:daemon deny read"),
                has_acl,
                "changed the original deny ACL"
            );
            assert!(
                !acl.contains("user:nobody"),
                "inherited a broader ACL: {acl}"
            );
            let attr = Command::new("/usr/bin/xattr")
                .args(["-p", "org.cowiki.test"])
                .arg(&index)
                .output()
                .unwrap();
            assert_eq!(attr.stdout, b"keep-me\n");
            let contents = std::fs::read_to_string(&index).unwrap();
            assert_eq!(contents == original, rollback);
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn review_publishes_a_complete_new_index_without_truncating_the_old_file() {
        use std::io::Read;
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let path = root.join("index.md");
        let original = "# My handwritten notes\n\nKeep this paragraph.\n";
        std::fs::write(&path, original).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        let mut old_file = std::fs::File::open(&path).unwrap();

        super::refresh_review_indexes(root).unwrap();
        let mut prior_bytes = String::new();
        old_file.read_to_string(&mut prior_bytes).unwrap();
        assert_eq!(
            prior_bytes, original,
            "refresh overwrote the live index in place"
        );
        let published = std::fs::read_to_string(&path).unwrap();
        assert!(published.contains(original) && published.contains("cowiki:generated-index:end"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    #[cfg(unix)]
    fn review_refresh_refuses_linked_indexes_and_restores_prior_listings() {
        use std::os::unix::fs::symlink;
        for existing_target in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("space");
            std::fs::create_dir_all(root.join("child/deeper")).unwrap();
            let index =
                "---\nokf_version: '0.1'\n---\n\n# Notes\n<!-- cowiki:generated-index:start -->\n";
            std::fs::write(root.join("index.md"), index).unwrap();
            let target = temp.path().join("private.md");
            if existing_target {
                std::fs::write(&target, "# Private outside the Space\n").unwrap();
            }
            let link = root.join("child/index.md");
            symlink(&target, &link).unwrap();

            let error = super::refresh_review_indexes(&root).unwrap_err();
            if existing_target {
                assert_eq!(
                    std::fs::read_to_string(&target).unwrap(),
                    "# Private outside the Space\n"
                );
            } else {
                assert!(
                    !target.exists(),
                    "refresh created an external symlink target"
                );
            }
            assert_eq!(std::fs::read_link(link).unwrap(), target);
            assert!(!root.join("child/deeper/index.md").exists());
            assert_eq!(
                std::fs::read_to_string(root.join("index.md")).unwrap(),
                index
            );
            assert!(error.contains("symbolic link"));
        }
    }

    #[test]
    #[cfg(unix)]
    fn review_refresh_rolls_back_child_when_parent_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let index = root.join("index.md");
        std::fs::write(&index, root_index()).unwrap();
        std::fs::create_dir(root.join("new")).unwrap();
        std::fs::set_permissions(&index, std::fs::Permissions::from_mode(0o444)).unwrap();
        if std::fs::OpenOptions::new().write(true).open(&index).is_ok() {
            // A privileged test process bypasses the permission failure.
            return;
        }
        assert!(super::refresh_review_indexes(root).is_err());
        assert!(!root.join("new/index.md").exists());
        assert_eq!(std::fs::read_to_string(index).unwrap(), root_index());
    }

    #[test]
    fn legacy_concept_kind_is_removed_without_dropping_other_metadata() {
        let original = "---\ntype: Note\ntitle: Test\nkind: concept\nowner: docs\n---\n\nBody\n";

        let normalized = normalize_concept_document(original, "Test").unwrap();
        let (frontmatter, body) = split_frontmatter(&normalized).unwrap();
        let mapping =
            serde_yaml::from_str::<Mapping>(frontmatter.unwrap()).expect("parse normalized YAML");

        assert!(!mapping.contains_key(Value::String("kind".into())));
        assert_eq!(
            mapping
                .get(Value::String("type".into()))
                .and_then(Value::as_str),
            Some("Note")
        );
        assert_eq!(
            mapping
                .get(Value::String("owner".into()))
                .and_then(Value::as_str),
            Some("docs")
        );
        assert_eq!(body, "\nBody\n");
    }

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
        let future_index = "---\nokf_version: \"0.4\"\n---\n\n# Future bundle\n";
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
    fn binary_attachments_are_not_archived_or_treated_as_unmigrated() {
        let bundle = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(bundle.path().join("notes/assets")).unwrap();
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00];
        std::fs::write(bundle.path().join("notes/assets/figure.png"), png).unwrap();
        std::fs::write(bundle.path().join("notes/paper.pdf"), [0xff, 0xfe, 0x00]).unwrap();
        std::fs::write(
            bundle.path().join("notes/Idea.md"),
            "# Idea\n\n![[figure.png]]\n",
        )
        .unwrap();

        ensure_bundle(bundle.path()).unwrap();

        assert!(!bundle.path().join(".cowiki/legacy").exists());
        assert_eq!(
            std::fs::read(bundle.path().join("notes/assets/figure.png")).unwrap(),
            png
        );
        assert!(!needs_migration(bundle.path()).unwrap());
        assert!(!needs_content_migration(bundle.path()).unwrap());
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
        assert!(root.contains("okf_version: '0.3'") || root.contains("okf_version: \"0.3\""));
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

    #[test]
    fn okf_v01_v02_and_v03_are_supported_while_future_versions_stay_read_only() {
        for version in ["0.1", "0.2", "0.3"] {
            let bundle = tempfile::tempdir().unwrap();
            let index = format!("---\nokf_version: \"{version}\"\n---\n\n# Space\n");
            std::fs::write(bundle.path().join("index.md"), &index).unwrap();
            assert!(ensure_supported_for_write(bundle.path()).is_ok());
            assert!(!needs_migration(bundle.path()).unwrap());
            assert_eq!(
                declared_version(bundle.path()).unwrap().as_deref(),
                Some(version)
            );
        }

        let future = tempfile::tempdir().unwrap();
        std::fs::write(
            future.path().join("index.md"),
            "---\nokf_version: \"0.4\"\n---\n\n# Future\n",
        )
        .unwrap();
        assert!(ensure_supported_for_write(future.path()).is_err());
        assert!(!needs_migration(future.path()).unwrap());
    }

    #[test]
    fn parse_metadata_extracts_okf_v03_governance_fields() {
        let content = "---\ntype: Architecture\ntitle: Sync Engine\ndescription: Local replication design\ntags:\n  - sync\n  - crdt\naliases:\n  - Replication\nsources:\n  - .cowiki/sources/spec.md\nstatus: deprecated\nstale_after: \"2025-01-15\"\ntrust_tier: human_verified\nverified: true\ngenerated: false\nredirect_to: sync-v2.md\nupdated_at: \"2026-06-01\"\n---\n\n# Sync Engine\n\nDetails.\n";
        let meta = parse_metadata(content);
        assert_eq!(meta.doc_type, "Architecture");
        assert_eq!(meta.title.as_deref(), Some("Sync Engine"));
        assert_eq!(
            meta.description.as_deref(),
            Some("Local replication design")
        );
        assert_eq!(meta.tags, vec!["sync", "crdt"]);
        assert_eq!(meta.aliases, vec!["Replication"]);
        assert_eq!(meta.sources, vec![".cowiki/sources/spec.md"]);
        assert_eq!(meta.status, "deprecated");
        assert!(meta.is_deprecated());
        assert!(!meta.is_retired());
        assert!(meta.is_stale_as_of(Some("2026-01-01")));
        assert!(!meta.is_stale_as_of(Some("2025-01-01")));
        assert_eq!(meta.trust_tier, "human_verified");
        assert!(meta.verified);
        assert!(!meta.generated);
        assert_eq!(meta.redirect_to.as_deref(), Some("sync-v2.md"));
        assert_eq!(meta.updated_at.as_deref(), Some("2026-06-01"));
    }

    #[test]
    fn agent_edit_trust_reset_demotes_verified_and_preserves_custom_keys() {
        let original = "---\ntype: Guide\ntitle: Auth Policy\nverified: true\ntrust_tier: human_verified\ngenerated: false\nowner: security-team\ncustom_flag: 42\nsources:\n  - .cowiki/sources/rfc.md\n---\n\n# Auth Policy\n\nRotate tokens weekly.\n";
        let updated = apply_agent_edit_trust_reset(
            original,
            "Auth Policy",
            &[
                ".cowiki/sources/rfc.md".to_string(),
                ".cowiki/sources/audit-2026.md".to_string(),
            ],
            Some("2026-11-22"),
        )
        .unwrap();

        let meta = parse_metadata(&updated);
        assert!(!meta.verified);
        assert_eq!(meta.trust_tier, "agent_draft");
        assert!(meta.generated);
        assert_eq!(meta.updated_at.as_deref(), Some("2026-11-22"));
        assert_eq!(
            meta.sources,
            vec![".cowiki/sources/rfc.md", ".cowiki/sources/audit-2026.md"]
        );
        // Custom user frontmatter keys must be preserved verbatim.
        assert!(updated.contains("owner: security-team"));
        assert!(updated.contains("custom_flag: 42"));
        assert!(updated.contains("Rotate tokens weekly."));
    }

    #[test]
    fn validate_bundle_detects_trust_reset_staleness_and_redirect_issues() {
        let bundle = tempfile::tempdir().unwrap();
        let root = bundle.path();
        std::fs::write(root.join("index.md"), root_index()).unwrap();
        std::fs::write(
            root.join("good.md"),
            "---\ntype: Note\ntitle: Good\nstatus: active\ntrust_tier: human_verified\nverified: true\nstale_after: \"2030-01-01\"\n---\n\n# Good\n\nClean note.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("bad-trust.md"),
            "---\ntype: Note\ntitle: Bad Trust\ntrust_tier: agent_draft\nverified: true\ngenerated: true\nstale_after: \"2024-01-01\"\n---\n\n# Bad Trust\n\nSee [ghost](ghost.md).\n",
        )
        .unwrap();
        std::fs::write(
            root.join("loop-a.md"),
            "---\ntype: Note\ntitle: Loop A\nredirect_to: loop-b.md\n---\n\n# Loop A\n",
        )
        .unwrap();
        std::fs::write(
            root.join("loop-b.md"),
            "---\ntype: Note\ntitle: Loop B\nredirect_to: loop-a.md\n---\n\n# Loop B\n",
        )
        .unwrap();
        std::fs::write(
            root.join("broken-redir.md"),
            "---\ntype: Note\ntitle: Broken Redir\nredirect_to: missing-target.md\n---\n\n# Broken Redir\n",
        )
        .unwrap();

        let report = validate_bundle(root, Some("2026-11-22"), true).unwrap();
        assert_eq!(report.okf_version.as_deref(), Some("0.3"));
        assert!(!report.valid);
        assert_eq!(
            report.summary_by_code.get("trust_reset_violation"),
            Some(&1)
        );
        assert_eq!(report.summary_by_code.get("expired_stale_after"), Some(&1));
        assert_eq!(report.summary_by_code.get("broken_redirect"), Some(&1));
        assert_eq!(report.summary_by_code.get("redirect_cycle"), Some(&2));
        assert_eq!(report.summary_by_code.get("broken_link"), Some(&1));
    }

    #[test]
    fn safe_resolve_under_space_blocks_traversal_and_symlink_escapes() {
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.md");
        std::fs::write(&secret, "# Secret\n").unwrap();

        let bundle = tempfile::tempdir().unwrap();
        let root = bundle.path();
        std::fs::create_dir_all(root.join("notes")).unwrap();
        let valid_file = root.join("notes/doc.md");
        std::fs::write(&valid_file, "# Doc\n").unwrap();

        let (rel_existing, resolved) = safe_resolve_under_space(root, "notes/doc.md").unwrap();
        assert_eq!(rel_existing, "notes/doc.md");
        assert_eq!(resolved, valid_file.canonicalize().unwrap());

        let (rel_new, new_file) = safe_resolve_under_space(root, "notes/new-page.md").unwrap();
        assert_eq!(rel_new, "notes/new-page.md");
        assert_eq!(
            new_file,
            root.canonicalize().unwrap().join("notes/new-page.md")
        );

        assert!(safe_resolve_under_space(root, "../secret.md").is_err());
        assert!(safe_resolve_under_space(root, "notes/../../secret.md").is_err());
        assert!(safe_resolve_under_space(root, "/etc/passwd").is_err());
        assert!(safe_resolve_under_space(root, "notes/bad\0.md").is_err());

        #[cfg(unix)]
        {
            let link_path = root.join("notes/escape.md");
            std::os::unix::fs::symlink(&secret, &link_path).unwrap();
            assert!(safe_resolve_under_space(root, "notes/escape.md").is_err());
        }
    }
}
