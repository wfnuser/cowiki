//! Read-only Obsidian vault inspection and copy-based import.
//!
//! The original vault is never modified: the import copies it into a new
//! folder, and only that copy becomes a CoWiki Space.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

const EXAMPLE_LIMIT: usize = 20;
pub const IMPORT_MARKER_PATH: &str = ".cowiki/obsidian-import.json";
const SKIPPED_NAMES: &[&str] = &[".git", ".trash", "node_modules", ".DS_Store", ".cowiki"];
/// Non-Markdown files at least this large stay in the imported folder but are
/// kept out of Git history, so importing a vault full of PDFs, recordings or
/// datasets does not double its size on disk.
pub const LARGE_FILE_BYTES: u64 = 5 * 1024 * 1024;
const LARGE_FILES_BEGIN: &str =
    "# >>> CoWiki import: large files stay in this folder but out of version history";
const LARGE_FILES_END: &str = "# <<< CoWiki import";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DuplicateName {
    pub name: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultReport {
    pub vault_path: String,
    pub vault_name: String,
    pub markdown_files: usize,
    pub attachment_files: usize,
    pub other_files: usize,
    pub folders: usize,
    pub total_bytes: u64,
    pub attachment_types: BTreeMap<String, usize>,
    pub wikilinks: usize,
    pub markdown_links: usize,
    pub embeds: usize,
    pub heading_links: usize,
    pub block_links: usize,
    pub block_ids: usize,
    pub unresolved_links: usize,
    pub unresolved_examples: Vec<String>,
    pub tags: usize,
    pub task_lines: usize,
    pub files_with_frontmatter: usize,
    pub invalid_frontmatter: Vec<String>,
    pub callouts: usize,
    pub dataview_blocks: usize,
    pub tasks_query_blocks: usize,
    pub templater_files: Vec<String>,
    pub excalidraw_files: usize,
    pub canvas_files: usize,
    pub duplicate_names: Vec<DuplicateName>,
    pub duplicate_name_count: usize,
    pub reserved_names: Vec<String>,
    pub non_utf8_files: Vec<String>,
    pub top_level_sources_folder: bool,
    pub symlinks_skipped: usize,
    pub large_files: usize,
    pub large_file_bytes: u64,
    pub large_file_examples: Vec<String>,
    pub markdown_paths: Vec<String>,
    pub daily_notes: Option<serde_json::Value>,
    pub templates_folder: Option<String>,
    pub attachment_folder: Option<String>,
    pub link_format: Option<String>,
    pub community_plugins: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CopyOutcome {
    pub copied_files: usize,
    pub copied_bytes: u64,
    pub skipped: usize,
}

fn relative_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn is_skipped(name: &str) -> bool {
    SKIPPED_NAMES.contains(&name)
}

fn push_example(examples: &mut Vec<String>, value: String) {
    if examples.len() < EXAMPLE_LIMIT {
        examples.push(value);
    }
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Fenced code ranges are skipped so examples in code are not counted.
fn prose_lines(content: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for line in content.lines() {
        let trimmed = line.trim_start();
        let marker = trimmed
            .chars()
            .next()
            .filter(|character| *character == '`' || *character == '~')
            .map(|character| {
                (
                    character,
                    trimmed.chars().take_while(|c| *c == character).count(),
                )
            })
            .filter(|(_, count)| *count >= 3);
        match (fence, marker) {
            (Some((character, count)), Some((next, next_count)))
                if next == character && next_count >= count =>
            {
                fence = None;
            }
            (Some(_), _) => {}
            (None, Some(open)) => fence = Some(open),
            (None, None) => lines.push(line),
        }
    }
    lines
}

fn frontmatter(content: &str) -> Option<&str> {
    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))?;
    let end = rest
        .find("\n---")
        .or_else(|| rest.starts_with("---").then_some(0))?;
    Some(&rest[..end])
}

struct LinkTargets {
    notes: HashSet<String>,
    files: HashSet<String>,
}

fn note_key(value: &str) -> String {
    let name = value.rsplit('/').next().unwrap_or(value);
    name.trim_end_matches(".md").to_lowercase()
}

fn link_resolves(target: &str, targets: &LinkTargets) -> bool {
    let page = target
        .split('|')
        .next()
        .unwrap_or_default()
        .split('#')
        .next()
        .unwrap_or_default()
        .trim();
    if page.is_empty() {
        return true;
    }
    let name = page.rsplit('/').next().unwrap_or(page).to_lowercase();
    if name.contains('.') && !name.ends_with(".md") {
        return targets.files.contains(&name);
    }
    targets.notes.contains(&note_key(page))
}

fn analyse_markdown(
    relative: &str,
    content: &str,
    targets: &LinkTargets,
    report: &mut VaultReport,
) {
    if let Some(raw) = frontmatter(content) {
        report.files_with_frontmatter += 1;
        if serde_yaml::from_str::<serde_yaml::Value>(raw).is_err() {
            push_example(&mut report.invalid_frontmatter, relative.to_string());
        }
    }
    if content.contains("<%") && content.contains("%>") {
        push_example(&mut report.templater_files, relative.to_string());
    }
    let mut in_frontmatter = content.starts_with("---");
    let mut first = true;
    for line in prose_lines(content) {
        if in_frontmatter {
            if !first && (line.trim() == "---" || line.trim() == "...") {
                in_frontmatter = false;
            }
            first = false;
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("> [!") {
            report.callouts += 1;
        }
        if crate::personal::is_task_candidate(line) {
            report.task_lines += 1;
        }
        if trimmed.ends_with(|c: char| c.is_ascii_alphanumeric() || c == '-')
            && trimmed.rsplit_once(" ^").is_some_and(|(_, id)| {
                !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
        {
            report.block_ids += 1;
        }
        let mut search = line;
        while let Some(start) = search.find("[[") {
            let after = &search[start + 2..];
            let Some(end) = after.find("]]") else { break };
            let target = &after[..end];
            report.wikilinks += 1;
            if start > 0 && search[..start].ends_with('!') {
                report.embeds += 1;
            }
            if let Some((_, fragment)) =
                target.split('|').next().unwrap_or_default().split_once('#')
            {
                if fragment.starts_with('^') {
                    report.block_links += 1;
                } else {
                    report.heading_links += 1;
                }
            }
            if !link_resolves(target, targets) {
                report.unresolved_links += 1;
                push_example(
                    &mut report.unresolved_examples,
                    format!("{relative}: [[{target}]]"),
                );
            }
            search = &after[end + 2..];
        }
        report.markdown_links += line.matches("](").count();
        report.tags += line
            .split_whitespace()
            .filter(|word| {
                word.len() > 1
                    && word.starts_with('#')
                    && word[1..].chars().next().is_some_and(|c| c.is_alphabetic())
            })
            .count();
    }
    let lower = content.to_lowercase();
    report.dataview_blocks += lower.matches("```dataview").count();
    report.tasks_query_blocks += lower.matches("```tasks").count();
}

/// Inspect a vault without changing anything.
pub fn scan_vault(root: &Path) -> Result<VaultReport, String> {
    if !root.is_dir() {
        return Err("choose an existing Obsidian vault folder".to_string());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut report = VaultReport {
        vault_path: root.to_string_lossy().to_string(),
        vault_name: root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        ..VaultReport::default()
    };
    let mut markdown = Vec::new();
    let mut names: HashMap<String, Vec<String>> = HashMap::new();
    let mut files = HashSet::new();
    for entry in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == root
                || !(entry.file_name().to_string_lossy().starts_with('.')
                    || is_skipped(&entry.file_name().to_string_lossy()))
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path == root {
            continue;
        }
        if entry.path_is_symlink() {
            report.symlinks_skipped += 1;
            continue;
        }
        let relative = relative_string(&root, path);
        if entry.file_type().is_dir() {
            report.folders += 1;
            if relative.eq_ignore_ascii_case("sources") {
                report.top_level_sources_folder = true;
            }
            continue;
        }
        let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        report.total_bytes += size;
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if size >= LARGE_FILE_BYTES && !lower.ends_with(".md") {
            report.large_files += 1;
            report.large_file_bytes += size;
            push_example(&mut report.large_file_examples, relative.clone());
        }
        files.insert(lower.clone());
        if lower.ends_with(".md") {
            report.markdown_files += 1;
            if lower.ends_with(".excalidraw.md") {
                report.excalidraw_files += 1;
            }
            if matches!(lower.as_str(), "index.md" | "log.md") {
                push_example(&mut report.reserved_names, relative.clone());
            }
            names
                .entry(lower.trim_end_matches(".md").to_string())
                .or_default()
                .push(relative.clone());
            report.markdown_paths.push(relative.clone());
            markdown.push((relative, path.to_path_buf()));
        } else if lower.ends_with(".canvas") {
            report.canvas_files += 1;
            report.other_files += 1;
        } else if let Some(extension) = Path::new(&lower).extension() {
            let extension = extension.to_string_lossy().to_string();
            if matches!(
                extension.as_str(),
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "svg"
                    | "bmp"
                    | "avif"
                    | "pdf"
                    | "mp3"
                    | "wav"
                    | "m4a"
                    | "ogg"
                    | "mp4"
                    | "webm"
                    | "mov"
                    | "html"
                    | "htm"
                    | "csv"
                    | "xlsx"
                    | "docx"
                    | "pptx"
                    | "zip"
                    | "tif"
                    | "tiff"
            ) {
                report.attachment_files += 1;
                *report.attachment_types.entry(extension).or_default() += 1;
            } else {
                report.other_files += 1;
            }
        } else {
            report.other_files += 1;
        }
    }
    let targets = LinkTargets {
        notes: names.keys().cloned().collect(),
        files,
    };
    for (relative, path) in &markdown {
        match std::fs::read_to_string(path) {
            Ok(content) => analyse_markdown(relative, &content, &targets, &mut report),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                push_example(&mut report.non_utf8_files, relative.clone());
            }
            Err(error) => return Err(format!("cannot read {relative}: {error}")),
        }
    }
    let mut duplicates = names
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(name, mut paths)| {
            paths.sort();
            DuplicateName { name, paths }
        })
        .collect::<Vec<_>>();
    duplicates.sort_by(|left, right| {
        right
            .paths
            .len()
            .cmp(&left.paths.len())
            .then(left.name.cmp(&right.name))
    });
    report.duplicate_name_count = duplicates.len();
    duplicates.truncate(EXAMPLE_LIMIT);
    report.duplicate_names = duplicates;
    report.markdown_paths.sort();

    let settings = root.join(".obsidian");
    let periodic = read_json(&settings.join("plugins/periodic-notes/data.json")).filter(|value| {
        value
            .get("daily")
            .is_some_and(|daily| daily.get("enabled") != Some(&serde_json::Value::Bool(false)))
    });
    report.daily_notes = periodic.or_else(|| read_json(&settings.join("daily-notes.json")));
    report.templates_folder =
        read_json(&settings.join("templates.json")).and_then(|value| json_string(&value, "folder"));
    if let Some(app) = read_json(&settings.join("app.json")) {
        report.attachment_folder = json_string(&app, "attachmentFolderPath");
        report.link_format = json_string(&app, "newLinkFormat");
    }
    report.community_plugins = read_json(&settings.join("community-plugins.json"))
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    Ok(report)
}

fn directory_is_empty(path: &Path) -> Result<bool, String> {
    Ok(std::fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .all(|entry| entry.file_name() == ".DS_Store"))
}

fn marker_source(destination: &Path) -> Option<String> {
    read_json(&destination.join(IMPORT_MARKER_PATH)).and_then(|value| json_string(&value, "source"))
}

/// Copy a vault into `destination`. The destination must be new or empty, or
/// an unfinished import of the same vault (so a failed import can be retried).
pub fn copy_vault(source: &Path, destination: &Path) -> Result<CopyOutcome, String> {
    let source = source.canonicalize().map_err(|error| error.to_string())?;
    if destination.exists() {
        let canonical = destination
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if canonical.starts_with(&source) || source.starts_with(&canonical) {
            return Err("the import folder cannot be inside the vault (or contain it)".to_string());
        }
        let same_import =
            marker_source(destination).as_deref() == Some(source.to_string_lossy().as_ref());
        if !directory_is_empty(destination)? && !same_import {
            return Err("choose an empty folder for the imported copy".to_string());
        }
    } else {
        if let Some(parent) = destination.parent() {
            let parent = parent.canonicalize().map_err(|error| error.to_string())?;
            if parent.starts_with(&source) {
                return Err("the import folder cannot be inside the vault".to_string());
            }
        }
        std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    }
    let marker = destination.join(IMPORT_MARKER_PATH);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let started = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    std::fs::write(
        &marker,
        serde_json::to_string_pretty(&serde_json::json!({
            "source": source.to_string_lossy(),
            "startedAt": started,
            "finished": false,
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let mut outcome = CopyOutcome::default();
    for entry in walkdir::WalkDir::new(&source)
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == source || !is_skipped(&entry.file_name().to_string_lossy())
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path == source {
            continue;
        }
        if entry.path_is_symlink() {
            outcome.skipped += 1;
            continue;
        }
        let relative: PathBuf = path
            .strip_prefix(&source)
            .map_err(|error| error.to_string())?
            .to_path_buf();
        let target = destination.join(&relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            continue;
        }
        if !entry.file_type().is_file() {
            outcome.skipped += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = std::fs::copy(path, &target)
            .map_err(|error| format!("cannot copy {}: {error}", relative.to_string_lossy()))?;
        outcome.copied_files += 1;
        outcome.copied_bytes += bytes;
    }
    std::fs::write(
        &marker,
        serde_json::to_string_pretty(&serde_json::json!({
            "source": source.to_string_lossy(),
            "startedAt": started,
            "finished": true,
            "copiedFiles": outcome.copied_files,
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(outcome)
}

/// Escape a path for an anchored `.gitignore` pattern.
fn gitignore_pattern(relative: &str) -> String {
    let mut pattern = String::from("/");
    for character in relative.chars() {
        if matches!(character, '\\' | '*' | '?' | '[' | ']' | '!' | '#') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    let trimmed = pattern.trim_end_matches(' ');
    let trailing = pattern.len() - trimmed.len();
    let mut escaped = trimmed.to_string();
    for _ in 0..trailing {
        escaped.push_str("\\ ");
    }
    escaped
}

/// List non-Markdown files of [`LARGE_FILE_BYTES`] or more in an imported
/// copy in a marked `.gitignore` block, replacing any earlier block. Returns
/// how many files the block lists.
pub fn exclude_large_files_from_git(root: &Path) -> Result<usize, String> {
    let mut patterns = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.path() == root || !matches!(entry.file_name().to_str(), Some(".git" | ".cowiki"))
        })
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let lower = entry.file_name().to_string_lossy().to_lowercase();
        let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        if size >= LARGE_FILE_BYTES && !lower.ends_with(".md") {
            patterns.push(gitignore_pattern(&relative_string(root, entry.path())));
        }
    }
    let path = root.join(".gitignore");
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("cannot read .gitignore: {error}")),
    };
    let mut kept = String::new();
    let mut inside = false;
    for line in existing.lines() {
        if line == LARGE_FILES_BEGIN {
            inside = true;
        } else if inside && line == LARGE_FILES_END {
            inside = false;
        } else if !inside {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    // Drop the blank separator line written before the block.
    let mut kept = match kept.trim_end_matches('\n') {
        "" => String::new(),
        text => format!("{text}\n"),
    };
    if patterns.is_empty() {
        if kept != existing {
            std::fs::write(&path, kept).map_err(|error| error.to_string())?;
        }
        return Ok(0);
    }
    if !kept.is_empty() {
        kept.push('\n');
    }
    kept.push_str(LARGE_FILES_BEGIN);
    kept.push('\n');
    for pattern in &patterns {
        kept.push_str(pattern);
        kept.push('\n');
    }
    kept.push_str(LARGE_FILES_END);
    kept.push('\n');
    std::fs::write(&path, kept).map_err(|error| format!("cannot write .gitignore: {error}"))?;
    Ok(patterns.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".obsidian/plugins/periodic-notes")).unwrap();
        std::fs::create_dir_all(root.join("Daily")).unwrap();
        std::fs::create_dir_all(root.join("Projects/A")).unwrap();
        std::fs::create_dir_all(root.join("Projects/B")).unwrap();
        std::fs::create_dir_all(root.join("attachments")).unwrap();
        std::fs::write(
            root.join(".obsidian/daily-notes.json"),
            r#"{"folder":"Daily","format":"YYYY-MM-DD"}"#,
        )
        .unwrap();
        std::fs::write(
            root.join(".obsidian/templates.json"),
            r#"{"folder":"Templates"}"#,
        )
        .unwrap();
        std::fs::write(
            root.join(".obsidian/app.json"),
            r#"{"attachmentFolderPath":"attachments","newLinkFormat":"shortest"}"#,
        )
        .unwrap();
        std::fs::write(
            root.join(".obsidian/community-plugins.json"),
            r#"["dataview","obsidian-tasks-plugin"]"#,
        )
        .unwrap();
        std::fs::write(
            root.join("Daily/2026-10-31.md"),
            "---\ntags: [daily]\n---\n- [ ] plan 📅 2026-11-01\nSee [[README]] and ![[diagram.png]] and [[Missing Note]].\n> [!note] callout\nA paragraph ^block-1\n[[README#^block-1]] [[README#Heading|alias]] #tag\n```dataview\nTABLE file.name\n```\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Projects/A/README.md"),
            "# A\n<% tp.date.now() %>\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Projects/B/README.md"),
            "---\nbad: [unclosed\n---\n# B\n",
        )
        .unwrap();
        std::fs::write(root.join("Projects/index.md"), "# Projects\n").unwrap();
        std::fs::write(root.join("attachments/diagram.png"), [0u8, 1, 2]).unwrap();
        std::fs::write(root.join("board.canvas"), "{}").unwrap();
        dir
    }

    #[test]
    fn scan_reports_links_plugins_and_risks_without_writing() {
        let dir = vault();
        let before = std::fs::read(dir.path().join("Daily/2026-10-31.md")).unwrap();
        let report = scan_vault(dir.path()).unwrap();
        assert_eq!(report.markdown_files, 4);
        assert_eq!(report.attachment_files, 1);
        assert_eq!(report.canvas_files, 1);
        assert_eq!(report.wikilinks, 5);
        assert_eq!(report.embeds, 1);
        assert_eq!(report.block_links, 1);
        assert_eq!(report.heading_links, 1);
        assert_eq!(report.block_ids, 1);
        assert_eq!(report.unresolved_links, 1);
        assert_eq!(report.task_lines, 1);
        assert_eq!(report.callouts, 1);
        assert_eq!(report.dataview_blocks, 1);
        assert_eq!(
            report.templater_files,
            vec!["Projects/A/README.md".to_string()]
        );
        assert_eq!(
            report.invalid_frontmatter,
            vec!["Projects/B/README.md".to_string()]
        );
        assert_eq!(report.duplicate_name_count, 1);
        assert_eq!(report.duplicate_names[0].name, "readme");
        assert_eq!(report.reserved_names, vec!["Projects/index.md".to_string()]);
        assert_eq!(report.templates_folder.as_deref(), Some("Templates"));
        assert_eq!(report.attachment_folder.as_deref(), Some("attachments"));
        assert_eq!(report.community_plugins.len(), 2);
        assert_eq!(
            report
                .daily_notes
                .as_ref()
                .and_then(|v| v.get("folder"))
                .and_then(|v| v.as_str()),
            Some("Daily")
        );
        assert_eq!(
            std::fs::read(dir.path().join("Daily/2026-10-31.md")).unwrap(),
            before
        );
    }

    #[test]
    fn copy_keeps_the_vault_untouched_and_supports_retry() {
        let dir = vault();
        let output = tempfile::tempdir().unwrap();
        let destination = output.path().join("Imported");
        let outcome = copy_vault(dir.path(), &destination).unwrap();
        assert!(outcome.copied_files >= 9);
        assert!(destination.join("attachments/diagram.png").is_file());
        assert!(destination.join(".obsidian/app.json").is_file());
        assert!(
            !dir.path().join(".cowiki").exists(),
            "the vault itself is not written"
        );
        // Retrying the same import into its own folder is allowed.
        copy_vault(dir.path(), &destination).unwrap();
        // A different, non-empty folder is refused.
        let other = output.path().join("Other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("keep.md"), "mine").unwrap();
        assert!(copy_vault(dir.path(), &other).is_err());
        // The copy cannot live inside the vault.
        assert!(copy_vault(dir.path(), &dir.path().join("inside")).is_err());
    }

    #[test]
    fn large_attachments_are_kept_out_of_git_history() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let large = vec![0u8; LARGE_FILE_BYTES as usize];
        std::fs::create_dir_all(root.join("Papers")).unwrap();
        std::fs::write(root.join("Papers/a [draft] #1*.pdf"), &large).unwrap();
        std::fs::write(root.join("big note.md"), &large).unwrap();
        std::fs::write(root.join("small.png"), b"png").unwrap();
        std::fs::write(root.join(".gitignore"), "workspace.json\n").unwrap();

        assert_eq!(exclude_large_files_from_git(root).unwrap(), 1);
        let first = std::fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(first.starts_with("workspace.json\n"));
        assert!(first.contains(r"/Papers/a \[draft\] \#1\*.pdf"));
        // Running again replaces the block instead of adding another one.
        assert_eq!(exclude_large_files_from_git(root).unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(root.join(".gitignore")).unwrap(),
            first
        );

        let repo = git2::Repository::init(root).unwrap();
        assert!(repo.is_path_ignored("Papers/a [draft] #1*.pdf").unwrap());
        assert!(!repo.is_path_ignored("big note.md").unwrap());
        assert!(!repo.is_path_ignored("small.png").unwrap());

        // Once the large file is gone, the block is removed.
        std::fs::remove_file(root.join("Papers/a [draft] #1*.pdf")).unwrap();
        assert_eq!(exclude_large_files_from_git(root).unwrap(), 0);
        assert_eq!(
            std::fs::read_to_string(root.join(".gitignore")).unwrap(),
            "workspace.json\n"
        );
    }

    #[test]
    fn scan_counts_large_attachments() {
        let dir = vault();
        std::fs::write(
            dir.path().join("attachments/talk.wav"),
            vec![0u8; LARGE_FILE_BYTES as usize],
        )
        .unwrap();
        let report = scan_vault(dir.path()).unwrap();
        assert_eq!(report.large_files, 1);
        assert_eq!(report.large_file_bytes, LARGE_FILE_BYTES);
        assert_eq!(report.large_file_examples, vec!["attachments/talk.wav"]);
    }
}
