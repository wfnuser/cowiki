//! Open Knowledge Format (OKF) v0.1 filesystem conventions.
//!
//! The standard intentionally does not prescribe domain directories. CoWiki
//! therefore treats the selected Space root as the bundle root and keeps its
//! own raw ingestion artifacts under the hidden `.cowiki/` namespace.

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
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

/// Resolve an API document ID to either a concept or an editable OKF index.
/// Update logs remain outside the page review workflow.
pub fn document_path(slug: &str) -> Result<String, String> {
    let normalized = normalize_relative_path(slug)?;
    let without_suffix = normalized.strip_suffix(".md").unwrap_or(&normalized);
    if without_suffix == "index" {
        return Ok("index.md".into());
    }
    if let Some(folder) = without_suffix
        .strip_suffix("/index")
        .or_else(|| without_suffix.strip_suffix("/_index"))
    {
        return folder_index_path(folder);
    }
    concept_path(without_suffix)
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
    Ok(source_storage_path(filename))
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
        return source_storage_path(relative);
    }
    path.to_string()
}

fn source_storage_path(filename: &str) -> String {
    let is_reserved = matches!(
        Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str()),
        Some("index.md" | "log.md")
    );
    if filename.ends_with(".md") && !is_reserved {
        return format!("{RAW_SOURCES_DIR}/{filename}");
    }
    let digest = format!("{:x}", Sha256::digest(filename.as_bytes()));
    format!("{RAW_SOURCES_DIR}/_encoded/{digest}.md")
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
    let (frontmatter, body) = split_frontmatter(content)?;
    let mut root_mapping = Mapping::new();
    let mut frontmatter_description = None;
    let frontmatter_title = if let Some(raw) = frontmatter {
        let mapping = serde_yaml::from_str::<Mapping>(raw)
            .map_err(|error| format!("invalid YAML frontmatter: {error}"))?;
        let title = mapping
            .get(Value::String("title".into()))
            .and_then(Value::as_str)
            .map(str::to_string);
        frontmatter_description = mapping
            .get(Value::String("description".into()))
            .or_else(|| mapping.get(Value::String("summary".into())))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if path == "index.md" {
            root_mapping = mapping;
        }
        title
    } else {
        None
    };
    let body = body.trim_start_matches(['\r', '\n']);
    let fallback_title = frontmatter_title.unwrap_or_else(|| {
        Path::new(path)
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(humanize_path_segment)
            .unwrap_or_else(|| "Knowledge".into())
    });
    let mut normalized_body = if body.lines().any(|line| line.starts_with("# ")) {
        body.to_string()
    } else if body.trim().is_empty() {
        folder_index(&fallback_title)
    } else {
        format!("# {fallback_title}\n\n{body}")
    };
    if path != "index.md" {
        if let Some(description) = frontmatter_description {
            if !normalized_body.contains(&description) {
                normalized_body = format!("{}\n\n{description}\n", normalized_body.trim_end());
            }
        }
    }
    if path == "index.md" {
        root_mapping.insert(
            Value::String("okf_version".into()),
            Value::String(OKF_VERSION.into()),
        );
        let yaml = serde_yaml::to_string(&root_mapping)
            .map_err(|error| format!("could not serialize root index frontmatter: {error}"))?;
        Ok(format!("---\n{yaml}---\n\n{normalized_body}"))
    } else {
        Ok(normalized_body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub title: String,
    pub target: String,
    pub description: Option<String>,
}

/// Replace CoWiki's generated listing while preserving the human-authored
/// heading and prose around it.
pub fn update_index_entries(
    path: &str,
    content: &str,
    entries: &[IndexEntry],
) -> Result<String, String> {
    const START: &str = "<!-- cowiki:generated-index:start -->";
    const END: &str = "<!-- cowiki:generated-index:end -->";

    let normalized = normalize_index_document(path, content)?;
    let (frontmatter, body) = split_frontmatter(&normalized)?;
    let mut listing = String::from(START);
    for entry in entries {
        listing.push_str("\n* [");
        listing.push_str(&escape_markdown_link_label(&entry.title));
        listing.push_str("](<");
        listing.push_str(&escape_markdown_link_target(&entry.target));
        listing.push_str(">)");
        if let Some(description) = entry
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            listing.push_str(" - ");
            listing.push_str(&escape_generated_text(description));
        }
    }
    listing.push('\n');
    listing.push_str(END);

    let body = if let Some(start) = body.find(START) {
        let after_start = &body[start..];
        let end_offset = after_start
            .find(END)
            .ok_or_else(|| "generated index block has no closing marker".to_string())?;
        let end = start + end_offset + END.len();
        format!("{}{}{}", &body[..start], listing, &body[end..])
    } else {
        format!("{}\n\n{}\n", body.trim_end(), listing)
    };

    if let Some(raw) = frontmatter {
        Ok(format!("---\n{raw}---\n\n{}", body.trim_start()))
    } else {
        Ok(body.trim_start().to_string())
    }
}

fn escape_markdown_link_label(value: &str) -> String {
    escape_generated_text(value)
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn escape_generated_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace(['\r', '\n'], " ")
}

fn escape_markdown_link_target(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('?', "%3F")
        .replace('#', "%23")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

pub fn display_metadata(content: &str) -> (Option<String>, Option<String>) {
    let Ok((Some(raw), _)) = split_frontmatter(content) else {
        return (None, None);
    };
    let Ok(mapping) = serde_yaml::from_str::<Mapping>(raw) else {
        return (None, None);
    };
    let value = |key: &str| {
        mapping
            .get(Value::String(key.into()))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    (
        value("title"),
        value("description").or_else(|| value("summary")),
    )
}

pub fn index_title(path: &str, content: &str) -> String {
    let body = split_frontmatter(content)
        .map(|(_, body)| body)
        .unwrap_or(content);
    body.lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            Path::new(path)
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map(humanize_path_segment)
                .unwrap_or_else(|| "Knowledge".into())
        })
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
    let files = files.into_iter().collect::<Vec<_>>();
    let mut result = BundleValidation::default();
    for file in &files {
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
    result.issues.extend(validate_index_coverage(&files));
    result
}

fn validate_index_coverage(files: &[BundleFile]) -> Vec<ValidationIssue> {
    let by_path = files
        .iter()
        .map(|file| (file.path.as_str(), file.content.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let mut issues = Vec::new();
    for file in files {
        if DocumentKind::from_path(&file.path) != DocumentKind::Index {
            continue;
        }
        let Some(content) = std::str::from_utf8(&file.content).ok() else {
            continue;
        };
        let directory = Path::new(&file.path)
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let prefix = if directory.is_empty() {
            String::new()
        } else {
            format!("{directory}/")
        };
        let mut expected = BTreeSet::new();
        for path in by_path.keys() {
            let Some(relative) = path.strip_prefix(&prefix) else {
                continue;
            };
            if relative == "index.md" || relative.starts_with('.') {
                continue;
            }
            if DocumentKind::from_path(path) == DocumentKind::Other {
                continue;
            }
            if let Some((child, _)) = relative.split_once('/') {
                if !child.starts_with('.') {
                    expected.insert(format!("{child}/"));
                }
            } else if DocumentKind::from_path(path) == DocumentKind::Concept {
                expected.insert(relative.to_string());
            }
        }
        let link_targets = markdown_link_targets(content);
        for target in expected {
            let absolute = if directory.is_empty() {
                format!("/{target}")
            } else {
                format!("/{directory}/{target}")
            };
            let covered = link_targets.iter().any(|candidate| {
                let candidate = candidate
                    .split(['#', '?'])
                    .next()
                    .unwrap_or(candidate.as_str());
                let candidate = percent_decode_target(candidate);
                candidate == target || candidate == format!("./{target}") || candidate == absolute
            });
            if !covered {
                issues.push(issue(
                    &file.path,
                    "index-entry",
                    &format!("Index is missing a progressive-disclosure entry for '{target}'"),
                ));
            }
        }
    }
    issues
}

fn percent_decode_target(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] == b'%' && position + 2 < bytes.len() {
            let high = hex_value(bytes[position + 1]);
            let low = hex_value(bytes[position + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push(high * 16 + low);
                position += 3;
                continue;
            }
        }
        decoded.push(bytes[position]);
        position += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn markdown_link_targets(content: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut remaining = content;
    while let Some(start) = remaining.find("](") {
        let after = &remaining[start + 2..];
        let (target, consumed) = if let Some(angle) = after.trim_start().strip_prefix('<') {
            let Some(end_angle) = angle.find('>') else {
                break;
            };
            let after_angle = &angle[end_angle + 1..];
            let Some(end_paren) = after_angle.find(')') else {
                break;
            };
            (
                Some(&angle[..end_angle]),
                after.len() - after_angle.len() + end_paren + 1,
            )
        } else {
            let Some(end) = after.find(')') else {
                break;
            };
            (after[..end].split_whitespace().next(), end + 1)
        };
        if let Some(target) = target.filter(|target| !target.is_empty()) {
            targets.push(target.to_string());
        }
        remaining = &after[consumed..];
    }
    for line in content.lines() {
        let line = line.trim_start();
        let Some((_, destination)) = line.split_once("]:") else {
            continue;
        };
        if !line.starts_with('[') {
            continue;
        }
        let destination = destination.trim();
        let target = if let Some(destination) = destination.strip_prefix('<') {
            destination.split_once('>').map(|(target, _)| target)
        } else {
            destination.split_whitespace().next()
        };
        if let Some(target) = target.filter(|target| !target.is_empty()) {
            targets.push(target.to_string());
        }
    }
    targets
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
    let body = if content.starts_with("---") {
        issues.push(issue(
            path,
            "log-frontmatter",
            "log.md must not contain frontmatter",
        ));
        split_frontmatter(content)
            .ok()
            .map(|(_, body)| body)
            .unwrap_or(content)
    } else {
        content
    };
    if !body.lines().any(|line| line.starts_with("# ")) {
        issues.push(issue(
            path,
            "log-heading",
            "Log files require a level-one title",
        ));
    }

    let lines = body.lines().collect::<Vec<_>>();
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
        if !lines[position + 1..next_heading].iter().any(|entry| {
            entry.starts_with("* ") || entry.starts_with("- ") || entry.starts_with("+ ")
        }) {
            issues.push(issue(
                path,
                "log-entry",
                "Each log date group requires at least one Markdown list entry",
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

fn humanize_path_segment(segment: &str) -> String {
    let words = segment.replace(['-', '_'], " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Knowledge".into(),
    }
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
