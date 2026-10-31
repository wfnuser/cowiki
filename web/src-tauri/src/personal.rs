//! Personal-workspace helpers for a local Space: the Space-level settings
//! file, task discovery across pages, and Obsidian-style attachment lookup.
//!
//! Markdown stays authoritative. Nothing here keeps a second copy of tasks or
//! journal entries; the client parses the returned page text itself.

use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// Space-level settings shared by everyone who opens the Space.
pub const SPACE_CONFIG_PATH: &str = ".cowiki/space.json";
const MAX_SPACE_CONFIG_BYTES: usize = 64 * 1024;
const MAX_ASSET_BYTES: u64 = 25 * 1024 * 1024;
const MAX_TASK_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub fn read_space_config(root: &Path) -> Result<Option<serde_json::Value>, String> {
    let path = root.join(SPACE_CONFIG_PATH);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read Space settings: {error}")),
    };
    if bytes.len() > MAX_SPACE_CONFIG_BYTES {
        return Err("Space settings file is too large".to_string());
    }
    // A hand-edited, malformed file must not lock the Space: report it as
    // missing so the client falls back to defaults, and keep the bytes.
    Ok(serde_json::from_slice(&bytes).ok())
}

pub fn write_space_config(root: &Path, value: &serde_json::Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("Space settings must be a JSON object".to_string());
    }
    let mut text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    text.push('\n');
    if text.len() > MAX_SPACE_CONFIG_BYTES {
        return Err("Space settings are too large".to_string());
    }
    let path = root.join(SPACE_CONFIG_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4().simple()));
    std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("cannot save Space settings: {error}")
    })
}

/// One page that contains at least one task-like line.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TaskSource {
    pub path: String,
    pub content: String,
}

fn visible_relative(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .map(|relative| {
            relative.components().all(|component| {
                let part = component.as_os_str().to_string_lossy();
                !part.starts_with('.') && part != "node_modules"
            })
        })
        .unwrap_or(false)
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Cheap pre-filter; the client applies the exact task grammar.
pub fn is_task_candidate(line: &str) -> bool {
    let trimmed = line.trim_start();
    let rest = if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
    {
        rest
    } else {
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            return false;
        }
        match trimmed[digits..]
            .strip_prefix(". ")
            .or_else(|| trimmed[digits..].strip_prefix(") "))
        {
            Some(rest) => rest,
            None => return false,
        }
    };
    let rest = rest.trim_start();
    let bytes = rest.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'[' && bytes[2] == b']' {
        return matches!(bytes[1], b' ' | b'x' | b'X' | b'/' | b'-');
    }
    [
        "TODO ",
        "DOING ",
        "NOW ",
        "LATER ",
        "DONE ",
        "CANCELED ",
        "CANCELLED ",
    ]
    .iter()
    .any(|marker| rest.starts_with(marker))
}

/// Every visible Markdown page that contains a task-like line, with its text.
pub fn task_sources(root: &Path) -> Result<Vec<TaskSource>, String> {
    let mut sources = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| entry.path() == root || visible_relative(root, entry.path()))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !entry.file_type().is_file() || !is_markdown(path) {
            continue;
        }
        if entry
            .metadata()
            .map(|metadata| metadata.len() > MAX_TASK_FILE_BYTES)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        if !content.lines().any(is_task_candidate) {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        sources.push(TaskSource {
            path: relative,
            content,
        });
    }
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(sources)
}

/// An attachment resolved inside a Space.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AssetBytes {
    pub path: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("tif" | "tiff") => "image/tiff",
        Some("pdf") => "application/pdf",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("m4a") => "audio/mp4",
        Some("ogg") => "audio/ogg",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        _ => "application/octet-stream",
    }
}

fn clean_target(target: &str) -> Result<String, String> {
    let trimmed = target.trim().trim_start_matches('<').trim_end_matches('>');
    let without_fragment = trimmed.split(['#', '?']).next().unwrap_or_default();
    let decoded = percent_encoding::percent_decode_str(without_fragment)
        .decode_utf8()
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| without_fragment.to_string());
    let value = decoded.replace('\\', "/");
    if value.is_empty() {
        return Err("attachment path is empty".to_string());
    }
    if value.contains("://") || value.starts_with("data:") || value.starts_with("//") {
        return Err("remote attachments are loaded directly, not from the Space".to_string());
    }
    Ok(value)
}

fn safe_join(root: &Path, base: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = base.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !path.pop() || !path.starts_with(root) {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    path.starts_with(root).then_some(path)
}

fn depth(path: &Path) -> usize {
    path.components().count()
}

/// Resolve an attachment reference the way Obsidian does: relative to the
/// page, then relative to the Space root, then by file name anywhere in the
/// Space (preferring the page's folder, then the shortest path).
pub fn resolve_asset(root: &Path, from_path: &str, target: &str) -> Result<PathBuf, String> {
    let target = clean_target(target)?;
    let from_directory = Path::new(from_path)
        .parent()
        .map(|parent| root.join(parent))
        .unwrap_or_else(|| root.to_path_buf());
    let rooted = target.trim_start_matches('/');
    let mut candidates = Vec::new();
    if !target.starts_with('/') {
        if let Some(path) = safe_join(root, &from_directory, &target) {
            candidates.push(path);
        }
    }
    if let Some(path) = safe_join(root, root, rooted) {
        candidates.push(path);
    }
    for candidate in candidates {
        if candidate.is_file() && visible_relative(root, &candidate) {
            return ensure_contained(root, candidate);
        }
    }
    let name = Path::new(rooted)
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .ok_or_else(|| "attachment path has no file name".to_string())?;
    let suffix = rooted.to_lowercase();
    let mut matches = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| entry.path() == root || visible_relative(root, entry.path()))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|file| file.to_string_lossy().to_lowercase() == name)
                && path
                    .strip_prefix(root)
                    .map(|relative| {
                        relative
                            .to_string_lossy()
                            .replace('\\', "/")
                            .to_lowercase()
                            .ends_with(&suffix)
                    })
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|path| {
        (
            usize::from(path.parent() != Some(from_directory.as_path())),
            depth(path),
            path.to_string_lossy().to_string(),
        )
    });
    match matches.into_iter().next() {
        Some(path) => ensure_contained(root, path),
        None => Err(format!("attachment '{target}' was not found in this Space")),
    }
}

fn ensure_contained(root: &Path, path: PathBuf) -> Result<PathBuf, String> {
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let canonical = path.canonicalize().map_err(|error| error.to_string())?;
    if !canonical.starts_with(&canonical_root) {
        return Err("attachment resolves outside this Space".to_string());
    }
    Ok(path)
}

pub fn read_asset(root: &Path, from_path: &str, target: &str) -> Result<AssetBytes, String> {
    let path = resolve_asset(root, from_path, target)?;
    let size = path.metadata().map_err(|error| error.to_string())?.len();
    if size > MAX_ASSET_BYTES {
        return Err("attachment is too large to preview".to_string());
    }
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    let relative = path
        .strip_prefix(root)
        .map_err(|error| error.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(AssetBytes {
        path: relative,
        mime: mime_for(&path).to_string(),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_config_round_trips_and_tolerates_bad_json() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_space_config(dir.path()).unwrap(), None);
        let value = serde_json::json!({ "features": { "modules": { "journal": true } } });
        write_space_config(dir.path(), &value).unwrap();
        assert_eq!(read_space_config(dir.path()).unwrap(), Some(value));
        std::fs::write(dir.path().join(SPACE_CONFIG_PATH), "{broken").unwrap();
        assert_eq!(read_space_config(dir.path()).unwrap(), None);
        assert!(write_space_config(dir.path(), &serde_json::json!([1])).is_err());
    }

    #[test]
    fn task_candidates_cover_checkboxes_and_logseq_markers() {
        assert!(is_task_candidate("- [ ] write"));
        assert!(is_task_candidate("  * [x] done"));
        assert!(is_task_candidate("1. [/] doing"));
        assert!(is_task_candidate("- TODO read"));
        assert!(!is_task_candidate("- plain"));
        assert!(!is_task_candidate("[ ] not a list"));
        assert!(!is_task_candidate("- [link](x)"));
    }

    #[test]
    fn task_sources_skip_hidden_folders_and_pages_without_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("journal")).unwrap();
        std::fs::create_dir_all(root.join(".obsidian")).unwrap();
        std::fs::write(root.join("journal/2026-10-31.md"), "- [ ] plan\n").unwrap();
        std::fs::write(root.join("notes.md"), "no tasks here\n").unwrap();
        std::fs::write(root.join(".obsidian/hidden.md"), "- [ ] hidden\n").unwrap();
        let sources = task_sources(root).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, "journal/2026-10-31.md");
    }

    #[test]
    fn assets_resolve_relative_rooted_and_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("notes/img")).unwrap();
        std::fs::create_dir_all(root.join("attachments")).unwrap();
        std::fs::write(root.join("notes/img/a.png"), b"a").unwrap();
        std::fs::write(root.join("attachments/Pasted image 1.png"), b"b").unwrap();
        std::fs::write(root.join("notes/b.png"), b"near").unwrap();
        std::fs::write(root.join("attachments/b.png"), b"far").unwrap();

        let relative = read_asset(root, "notes/page.md", "img/a.png").unwrap();
        assert_eq!(relative.path, "notes/img/a.png");
        assert_eq!(relative.mime, "image/png");
        assert_eq!(
            read_asset(root, "page.md", "/notes/img/a.png")
                .unwrap()
                .bytes,
            b"a"
        );
        assert_eq!(
            read_asset(root, "notes/page.md", "Pasted%20image%201.png")
                .unwrap()
                .path,
            "attachments/Pasted image 1.png"
        );
        assert_eq!(
            read_asset(root, "notes/page.md", "b.png").unwrap().bytes,
            b"near"
        );
        assert_eq!(
            read_asset(root, "other/page.md", "b.png").unwrap().bytes,
            b"far"
        );
        assert!(read_asset(root, "notes/page.md", "../../etc/passwd").is_err());
        assert!(read_asset(root, "notes/page.md", "https://example.com/x.png").is_err());
        assert!(read_asset(root, "notes/page.md", "missing.png").is_err());
    }
}
