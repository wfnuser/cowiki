//! Open Knowledge Format (OKF) v0.1 filesystem conventions.
//!
//! The standard intentionally does not prescribe domain directories. CoWiki
//! therefore treats the selected Space root as the bundle root and keeps its
//! own raw ingestion artifacts under the hidden `.cowiki/` namespace.

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::path::{Component, Path};

pub const OKF_VERSION: &str = "0.1";
pub const RAW_SOURCES_DIR: &str = ".cowiki/sources";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Concept,
    Index,
    Log,
    Other,
}

impl DocumentKind {
    pub fn from_path(path: &str) -> Self {
        let path = Path::new(path);
        let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
            return Self::Other;
        };
        match filename {
            "index.md" => Self::Index,
            "log.md" => Self::Log,
            _ if filename.ends_with(".md") => Self::Concept,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub path: String,
    pub rule: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct BundleFile {
    pub path: String,
    pub content: Vec<u8>,
}

impl BundleFile {
    pub fn new(path: impl Into<String>, content: &[u8]) -> Self {
        Self {
            path: path.into(),
            content: content.to_vec(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BundleValidation {
    pub concepts: usize,
    pub indexes: usize,
    pub logs: usize,
    pub issues: Vec<ValidationIssue>,
}

impl BundleValidation {
    pub fn is_conformant(&self) -> bool {
        self.issues.is_empty()
    }
}

pub fn concept_path(slug: &str) -> Result<String, String> {
    let normalized = normalize_relative_path(slug)?;
    let without_suffix = normalized.strip_suffix(".md").unwrap_or(&normalized);
    let filename = Path::new(without_suffix)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "concept slug must name a file".to_string())?;
    if filename == "index" || filename == "log" || filename == "_index" {
        return Err(format!("'{filename}' is reserved by OKF"));
    }
    Ok(format!("{without_suffix}.md"))
}

pub fn folder_index_path(folder: &str) -> Result<String, String> {
    let folder = normalize_relative_path(folder)?;
    Ok(format!("{folder}/index.md"))
}

pub fn source_path(filename: &str) -> Result<String, String> {
    if filename.is_empty()
        || filename == "."
        || filename == ".."
        || filename.contains('/')
        || filename.contains('\\')
        || filename.starts_with('.')
    {
        return Err("source filename must be a plain filename".into());
    }
    Ok(format!("{RAW_SOURCES_DIR}/{filename}"))
}

/// Maps the pre-OKF CoWiki layout into a conforming bundle without deleting
/// producer-defined directories such as `entities/` or `concepts/`.
pub fn migrate_legacy_path(path: &str) -> String {
    if let Some(relative) = path.strip_prefix("wiki/") {
        if relative == "_index.md" {
            return "index.md".into();
        }
        return relative.replace("/_index.md", "/index.md");
    }
    if let Some(relative) = path.strip_prefix("sources/") {
        return format!("{RAW_SOURCES_DIR}/{relative}");
    }
    path.to_string()
}

pub fn root_index() -> String {
    format!("---\nokf_version: \"{OKF_VERSION}\"\n---\n\n# Knowledge\n")
}

pub fn folder_index(title: &str) -> String {
    format!("# {title}\n")
}

pub fn source_document(filename: &str, body: &str) -> Result<String, String> {
    source_path(filename)?;
    let mut mapping = Mapping::new();
    mapping.insert(Value::String("type".into()), Value::String("Source".into()));
    mapping.insert(
        Value::String("title".into()),
        Value::String(filename.to_string()),
    );
    let yaml = serde_yaml::to_string(&mapping)
        .map_err(|error| format!("could not serialize source frontmatter: {error}"))?;
    Ok(format!("---\n{yaml}---\n\n{body}"))
}

pub fn source_body(document: &str) -> Result<String, String> {
    let (frontmatter, body) = split_frontmatter(document)?;
    if frontmatter.is_none() {
        return Err("source concept requires frontmatter".into());
    }
    Ok(body.trim_start_matches(['\r', '\n']).to_string())
}

pub fn normalize_index_document(path: &str, content: &str) -> Result<String, String> {
    let (_, body) = split_frontmatter(content)?;
    let body = body.trim_start_matches(['\r', '\n']);
    let fallback_title = Path::new(path)
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("Knowledge");
    let normalized_body = if body.lines().any(|line| line.starts_with("# ")) {
        body.to_string()
    } else if body.trim().is_empty() {
        folder_index(fallback_title)
    } else {
        format!("# {fallback_title}\n\n{body}")
    };
    if path == "index.md" {
        Ok(format!(
            "---\nokf_version: \"{OKF_VERSION}\"\n---\n\n{normalized_body}"
        ))
    } else {
        Ok(normalized_body)
    }
}

pub fn replacement_log(legacy_path: &str) -> String {
    let date = chrono::Utc::now().date_naive();
    format!(
        "# Directory Update Log\n\n## {date}\n* **Migration**: Preserved the legacy non-conforming log at [legacy copy](/{legacy_path}).\n"
    )
}

pub fn normalize_concept_document(content: &str, fallback_title: &str) -> Result<String, String> {
    let (frontmatter, body) = split_frontmatter(content)?;
    let mut mapping = match frontmatter {
        Some(raw) => serde_yaml::from_str::<Mapping>(raw)
            .map_err(|error| format!("invalid YAML frontmatter: {error}"))?,
        None => Mapping::new(),
    };

    set_if_missing_or_blank(&mut mapping, "type", Value::String("Note".into()));
    set_if_missing_or_blank(
        &mut mapping,
        "title",
        Value::String(fallback_title.to_string()),
    );

    let description_key = Value::String("description".into());
    if !mapping.contains_key(&description_key) {
        if let Some(summary) = mapping.get(Value::String("summary".into())).cloned() {
            mapping.insert(description_key, summary);
        }
    }

    let yaml = serde_yaml::to_string(&mapping)
        .map_err(|error| format!("could not serialize YAML frontmatter: {error}"))?;
    let body = body.trim_start_matches(['\r', '\n']);
    Ok(format!("---\n{yaml}---\n\n{body}"))
}

pub fn validate_document(path: &str, bytes: &[u8]) -> Vec<ValidationIssue> {
    let Ok(content) = std::str::from_utf8(bytes) else {
        return vec![issue(
            path,
            "utf-8",
            "Markdown documents must be valid UTF-8",
        )];
    };

    match DocumentKind::from_path(path) {
        DocumentKind::Concept => validate_concept(path, content),
        DocumentKind::Index => validate_index(path, content),
        DocumentKind::Log => validate_log(path, content),
        DocumentKind::Other => Vec::new(),
    }
}

pub fn validate_bundle(files: impl IntoIterator<Item = BundleFile>) -> BundleValidation {
    let mut result = BundleValidation::default();
    for file in files {
        match DocumentKind::from_path(&file.path) {
            DocumentKind::Concept => result.concepts += 1,
            DocumentKind::Index => result.indexes += 1,
            DocumentKind::Log => result.logs += 1,
            DocumentKind::Other => continue,
        }
        result
            .issues
            .extend(validate_document(&file.path, &file.content));
    }
    result
}

fn validate_concept(path: &str, content: &str) -> Vec<ValidationIssue> {
    let Ok((frontmatter, _)) = split_frontmatter(content) else {
        return vec![issue(
            path,
            "frontmatter",
            "Concept frontmatter must be parseable YAML",
        )];
    };
    let Some(raw) = frontmatter else {
        return vec![issue(
            path,
            "frontmatter",
            "Concept documents require YAML frontmatter",
        )];
    };
    let Ok(mapping) = serde_yaml::from_str::<Mapping>(raw) else {
        return vec![issue(
            path,
            "frontmatter",
            "Concept frontmatter must be parseable YAML",
        )];
    };
    let type_value = mapping.get(Value::String("type".into()));
    match type_value.and_then(Value::as_str).map(str::trim) {
        Some(value) if !value.is_empty() => Vec::new(),
        _ => vec![issue(
            path,
            "type",
            "Concept frontmatter requires a non-empty type",
        )],
    }
}

fn validate_index(path: &str, content: &str) -> Vec<ValidationIssue> {
    if path != "index.md" && content.starts_with("---") {
        return vec![issue(
            path,
            "index-frontmatter",
            "Only the bundle-root index.md may contain frontmatter",
        )];
    }
    let mut issues = Vec::new();
    let body = if path == "index.md" && content.starts_with("---") {
        match split_frontmatter(content) {
            Ok((Some(raw), body)) => {
                match serde_yaml::from_str::<Mapping>(raw) {
                    Ok(_) => {}
                    Err(_) => issues.push(issue(
                        path,
                        "frontmatter",
                        "Root index frontmatter must be parseable YAML",
                    )),
                }
                body
            }
            _ => {
                issues.push(issue(
                    path,
                    "frontmatter",
                    "Root index frontmatter must have a closing delimiter",
                ));
                content
            }
        }
    } else {
        content
    };
    if !body.lines().any(|line| line.starts_with("# ")) {
        issues.push(issue(
            path,
            "index-heading",
            "Index files require at least one level-one section heading",
        ));
    }
    issues
}

fn validate_log(path: &str, content: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if !content.lines().any(|line| line.starts_with("# ")) {
        issues.push(issue(
            path,
            "log-heading",
            "Log files require a level-one title",
        ));
    }

    let lines = content.lines().collect::<Vec<_>>();
    let mut dates = Vec::new();
    for (position, line) in lines.iter().enumerate() {
        let Some(raw_date) = line.strip_prefix("## ") else {
            continue;
        };
        match chrono::NaiveDate::parse_from_str(raw_date.trim(), "%Y-%m-%d") {
            Ok(date) => dates.push(date),
            Err(_) => issues.push(issue(
                path,
                "log-date",
                &format!("Log date '{raw_date}' must use YYYY-MM-DD"),
            )),
        }
        let next_heading = lines[position + 1..]
            .iter()
            .position(|candidate| candidate.starts_with("## "))
            .map(|offset| position + 1 + offset)
            .unwrap_or(lines.len());
        if !lines[position + 1..next_heading]
            .iter()
            .any(|entry| entry.starts_with("* "))
        {
            issues.push(issue(
                path,
                "log-entry",
                "Each log date group requires at least one '* ' list entry",
            ));
        }
    }
    if !dates.windows(2).all(|pair| pair[0] >= pair[1]) {
        issues.push(issue(
            path,
            "log-order",
            "Log date groups must be newest first",
        ));
    }
    if !lines.iter().any(|line| line.starts_with("## ")) {
        issues.push(issue(
            path,
            "log-date",
            "Log files require at least one ISO 8601 date group",
        ));
    }
    issues
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
        let line_without_ending = line.trim_end_matches(['\r', '\n']);
        if line_without_ending == "---" {
            let raw = &after_opening[..offset];
            let body = &after_opening[offset + line.len()..];
            return Ok((Some(raw), body));
        }
        offset += line.len();
    }
    Err("frontmatter has no closing delimiter".into())
}

fn set_if_missing_or_blank(mapping: &mut Mapping, key: &str, fallback: Value) {
    let key = Value::String(key.into());
    let missing_or_blank = mapping
        .get(&key)
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::is_empty)
        .unwrap_or(true);
    if missing_or_blank {
        mapping.insert(key, fallback);
    }
}

fn normalize_relative_path(path: &str) -> Result<String, String> {
    if path.is_empty() || Path::new(path).is_absolute() || path.contains('\\') {
        return Err("path must be a non-empty bundle-relative path".into());
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
            _ => return Err("path may not contain parent or root components".into()),
        }
    }
    if segments.is_empty() {
        return Err("path must not be empty".into());
    }
    Ok(segments.join("/"))
}

fn issue(path: &str, rule: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        path: path.into(),
        rule: rule.into(),
        message: message.into(),
    }
}
