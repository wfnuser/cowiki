//! Rebuildable local knowledge index.
//!
//! Markdown files remain the source of truth. Every row in these tables is
//! derived data and may be dropped and rebuilt without losing user content.

use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::Path;

const SCHEMA_VERSION: i64 = 1;

const CREATE_SCHEMA: &str = "CREATE TABLE knowledge_index_metadata (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        schema_version INTEGER NOT NULL
    );
    CREATE TABLE indexed_pages (
        id INTEGER PRIMARY KEY,
        space_id TEXT NOT NULL,
        path TEXT NOT NULL,
        title TEXT NOT NULL,
        body TEXT NOT NULL,
        modified_ns INTEGER NOT NULL DEFAULT 0,
        UNIQUE(space_id, path)
    );
    CREATE VIRTUAL TABLE indexed_pages_fts USING fts5(
        title,
        body,
        content='indexed_pages',
        content_rowid='id',
        tokenize='unicode61 remove_diacritics 2'
    );
    CREATE TRIGGER indexed_pages_insert AFTER INSERT ON indexed_pages BEGIN
        INSERT INTO indexed_pages_fts(rowid, title, body)
        VALUES (new.id, new.title, new.body);
    END;
    CREATE TRIGGER indexed_pages_delete AFTER DELETE ON indexed_pages BEGIN
        INSERT INTO indexed_pages_fts(indexed_pages_fts, rowid, title, body)
        VALUES ('delete', old.id, old.title, old.body);
    END;
    CREATE TRIGGER indexed_pages_update AFTER UPDATE ON indexed_pages BEGIN
        INSERT INTO indexed_pages_fts(indexed_pages_fts, rowid, title, body)
        VALUES ('delete', old.id, old.title, old.body);
        INSERT INTO indexed_pages_fts(rowid, title, body)
        VALUES (new.id, new.title, new.body);
    END;
    CREATE TABLE page_links (
        space_id TEXT NOT NULL,
        source_path TEXT NOT NULL,
        target TEXT NOT NULL,
        UNIQUE(space_id, source_path, target)
    );
    CREATE INDEX page_links_target ON page_links(space_id, target);";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchHit {
    pub slug: String,
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub title_match: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrokenLink {
    pub source_path: String,
    pub source_title: String,
    pub target: String,
}

/// Keep only a compatible derived schema at startup.
/// Page rows are deliberately left empty after a reset; the selected Space is
/// repopulated by `refresh_space` when search, backlinks, or MCP context needs it.
pub fn initialize(connection: &mut Connection) -> Result<(), String> {
    let version = connection
        .query_row(
            "SELECT schema_version FROM knowledge_index_metadata WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .ok();
    let healthy = version == Some(SCHEMA_VERSION) && schema_is_structurally_healthy(connection);
    if healthy {
        return Ok(());
    }
    reset_schema(connection)
}

fn schema_is_structurally_healthy(connection: &Connection) -> bool {
    let required_objects = [
        ("table", "indexed_pages"),
        ("table", "indexed_pages_fts"),
        ("table", "page_links"),
        ("trigger", "indexed_pages_insert"),
        ("trigger", "indexed_pages_delete"),
        ("trigger", "indexed_pages_update"),
        ("index", "page_links_target"),
    ];
    if required_objects.iter().any(|(kind, name)| {
        !connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_schema WHERE type = ?1 AND name = ?2
                )",
                params![kind, name],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(false)
    }) {
        return false;
    }
    if connection
        .prepare(
            "SELECT id, space_id, path, title, body, modified_ns
             FROM indexed_pages LIMIT 0",
        )
        .is_err()
        || connection
            .prepare("SELECT space_id, source_path, target FROM page_links LIMIT 0")
            .is_err()
    {
        return false;
    }
    true
}

/// FTS5's external-content integrity check is proportional to indexed content,
/// so run it lazily on first index access instead of on every application open.
pub fn repair_if_corrupt(connection: &mut Connection) -> Result<(), String> {
    if connection
        .execute(
            "INSERT INTO indexed_pages_fts(indexed_pages_fts, rank)
             VALUES ('integrity-check', 1)",
            [],
        )
        .is_ok()
    {
        Ok(())
    } else {
        reset_schema(connection)
    }
}

fn reset_schema(connection: &mut Connection) -> Result<(), String> {
    let transaction = connection
        .transaction()
        .map_err(|error| format!("cannot reset the disposable local search index: {error}"))?;
    transaction
        .execute_batch(
            "DROP TRIGGER IF EXISTS indexed_pages_insert;
             DROP TRIGGER IF EXISTS indexed_pages_delete;
             DROP TRIGGER IF EXISTS indexed_pages_update;
             DROP TABLE IF EXISTS indexed_pages_fts;
             DROP TABLE IF EXISTS page_links;
             DROP TABLE IF EXISTS indexed_pages;
             DROP TABLE IF EXISTS knowledge_index_metadata;",
        )
        .and_then(|_| transaction.execute_batch(CREATE_SCHEMA))
        .map_err(|error| format!("cannot reset the disposable local search index: {error}"))?;
    transaction
        .execute(
            "INSERT INTO knowledge_index_metadata (singleton, schema_version) VALUES (1, ?1)",
            [SCHEMA_VERSION],
        )
        .map_err(|error| format!("cannot version the local search index: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("cannot initialize local search index: {error}"))
}

pub fn rebuild_space(
    connection: &mut Connection,
    space_id: &str,
    root: &Path,
) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction
        .execute("DELETE FROM page_links WHERE space_id = ?1", [space_id])
        .map_err(|e| e.to_string())?;
    transaction
        .execute("DELETE FROM indexed_pages WHERE space_id = ?1", [space_id])
        .map_err(|e| e.to_string())?;

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_walk(entry.path(), root))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if entry.file_type().is_file() && is_markdown(path) {
            index_file_in(&transaction, space_id, root, path)?;
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

/// Reconcile the derived index with files on disk without re-reading pages
/// whose modification timestamp has not changed. This is used before search
/// so edits made directly by Codex, Claude Code, or another editor appear in
/// retrieval without making SQLite authoritative.
pub fn refresh_space(
    connection: &mut Connection,
    space_id: &str,
    root: &Path,
) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let existing = {
        let mut statement = transaction
            .prepare("SELECT path, modified_ns FROM indexed_pages WHERE space_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([space_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<HashMap<_, _>, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut seen = HashSet::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| should_walk(entry.path(), root))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !entry.file_type().is_file() || !is_markdown(path) {
            continue;
        }
        let relative = relative_path(root, path)?;
        let modified_ns = file_modified_ns(path);
        seen.insert(relative.clone());
        if existing.get(&relative).copied() != Some(modified_ns) {
            index_file_in(&transaction, space_id, root, path)?;
        }
    }
    for deleted in existing.keys().filter(|path| !seen.contains(*path)) {
        transaction
            .execute(
                "DELETE FROM page_links WHERE space_id = ?1 AND source_path = ?2",
                params![space_id, deleted],
            )
            .map_err(|e| e.to_string())?;
        transaction
            .execute(
                "DELETE FROM indexed_pages WHERE space_id = ?1 AND path = ?2",
                params![space_id, deleted],
            )
            .map_err(|e| e.to_string())?;
    }
    transaction.commit().map_err(|e| e.to_string())
}

/// Re-derive one Space's relationship graph after its Markdown rows have been
/// refreshed. This runs lazily before the first backlinks query in a process,
/// so corrupt link rows cannot make SQLite authoritative at startup.
pub fn rebuild_links(connection: &mut Connection, space_id: &str) -> Result<(), String> {
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let pages = {
        let mut statement = transaction
            .prepare("SELECT path, body FROM indexed_pages WHERE space_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([space_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    transaction
        .execute("DELETE FROM page_links WHERE space_id = ?1", [space_id])
        .map_err(|e| e.to_string())?;
    for (source_path, body) in pages {
        for target in extract_links(&body, &source_path) {
            transaction
                .execute(
                    "INSERT INTO page_links (space_id, source_path, target)
                     VALUES (?1, ?2, ?3)",
                    params![space_id, source_path, target],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

fn index_file_in(
    connection: &Connection,
    space_id: &str,
    root: &Path,
    path: &Path,
) -> Result<(), String> {
    let relative = relative_path(root, path)?;
    let body = match std::fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            connection
                .execute(
                    "DELETE FROM page_links WHERE space_id = ?1 AND source_path = ?2",
                    params![space_id, relative],
                )
                .map_err(|e| e.to_string())?;
            connection
                .execute(
                    "DELETE FROM indexed_pages WHERE space_id = ?1 AND path = ?2",
                    params![space_id, relative],
                )
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };
    let title = crate::local_engine::markdown_title(&body).unwrap_or_else(|| {
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });
    let modified_ns = file_modified_ns(path);

    connection
        .execute(
            "INSERT INTO indexed_pages (space_id, path, title, body, modified_ns)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(space_id, path) DO UPDATE SET
               title = excluded.title,
               body = excluded.body,
               modified_ns = excluded.modified_ns",
            params![space_id, relative, title, body, modified_ns],
        )
        .map_err(|e| e.to_string())?;
    connection
        .execute(
            "DELETE FROM page_links WHERE space_id = ?1 AND source_path = ?2",
            params![space_id, relative],
        )
        .map_err(|e| e.to_string())?;
    for target in extract_links(&body, &relative) {
        connection
            .execute(
                "INSERT OR IGNORE INTO page_links (space_id, source_path, target)
                 VALUES (?1, ?2, ?3)",
                params![space_id, relative, target],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn search(
    connection: &Connection,
    space_id: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let fts_query = fts_query(query);
    if fts_query.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let title_needle = format!("%{}%", query.trim().to_lowercase());
    let mut statement = connection
        .prepare(
            "SELECT page.path,
                    page.title,
                    snippet(indexed_pages_fts, 1, '', '', ' … ', 24),
                    lower(page.title) LIKE ?3 AS title_match
             FROM indexed_pages_fts
             JOIN indexed_pages AS page ON page.id = indexed_pages_fts.rowid
             WHERE indexed_pages_fts MATCH ?1 AND page.space_id = ?2
             ORDER BY title_match DESC, bm25(indexed_pages_fts, 8.0, 1.0) ASC, page.path ASC
             LIMIT ?4",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(
            params![fts_query, space_id, title_needle, limit.min(100) as i64],
            |row| {
                let path: String = row.get(0)?;
                Ok(SearchHit {
                    slug: slug_for_path(&path),
                    path,
                    title: row.get(1)?,
                    snippet: row.get::<_, String>(2)?.replace('\n', " "),
                    title_match: row.get(3)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;
    let mut results = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    results.retain(|hit| {
        hit.path != "index.md"
            && hit.path != "log.md"
            && !hit.path.ends_with("/index.md")
            && !hit.path.ends_with("/log.md")
    });
    Ok(results)
}

pub fn backlinks(
    connection: &Connection,
    space_id: &str,
    target_path: &str,
) -> Result<Vec<SearchHit>, String> {
    let Some(target) = resolve_wikilink_target(target_path) else {
        return Ok(Vec::new());
    };
    let mut statement = connection
        .prepare(
            "SELECT page.path, page.title, page.body
             FROM page_links AS link
             JOIN indexed_pages AS page
               ON page.space_id = link.space_id AND page.path = link.source_path
             WHERE link.space_id = ?1 AND link.target = ?2
             ORDER BY page.title COLLATE NOCASE, page.path",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params![space_id, target], |row| {
            let path: String = row.get(0)?;
            let body: String = row.get(2)?;
            Ok(SearchHit {
                slug: slug_for_path(&path),
                path,
                title: row.get(1)?,
                snippet: first_content_line(&body),
                title_match: false,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut results = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    results.retain(|hit| {
        hit.path != "index.md"
            && hit.path != "log.md"
            && !hit.path.ends_with("/index.md")
            && !hit.path.ends_with("/log.md")
    });
    Ok(results)
}

pub fn broken_links(
    connection: &Connection,
    space_id: &str,
    root: &Path,
) -> Result<Vec<BrokenLink>, String> {
    let live_targets = live_space_targets(root)?;
    let mut statement = connection
        .prepare(
            "SELECT link.source_path, page.title, link.target
             FROM page_links AS link
             JOIN indexed_pages AS page
               ON page.space_id = link.space_id AND page.path = link.source_path
             WHERE link.space_id = ?1
             ORDER BY link.source_path, link.target",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([space_id], |row| {
            Ok(BrokenLink {
                source_path: row.get(0)?,
                source_title: row.get(1)?,
                target: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let results = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|link| {
            is_diagnostic_source(&link.source_path)
                && is_diagnostic_target(&link.target)
                && !live_targets.contains(&link.target)
        })
        .collect();
    Ok(results)
}

fn live_space_targets(root: &Path) -> Result<HashSet<String>, String> {
    let mut targets = HashSet::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_walk(entry.path(), root))
    {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.path() != root {
            targets.insert(relative_path(root, entry.path())?);
        }
    }
    Ok(targets)
}

pub fn page_count(connection: &Connection, space_id: &str) -> Result<usize, String> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM indexed_pages WHERE space_id = ?1",
            [space_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count.max(0) as usize)
        .map_err(|e| e.to_string())
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn file_modified_ns(path: &Path) -> i64 {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn should_walk(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    if relative.as_os_str().is_empty() {
        return true;
    }
    let parts = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>();
    if parts.first().is_some_and(|part| part == ".cowiki") {
        return parts.len() == 1
            || (parts.get(1).is_some_and(|part| part == "sources")
                && parts[2..].iter().all(|part| !part.starts_with('.')));
    }
    parts.iter().all(|part| !part.starts_with('.'))
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(normalize_path)
        .map_err(|_| "indexed page is outside its Space".to_string())
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn slug_for_path(path: &str) -> String {
    path.strip_suffix(".md").unwrap_or(path).to_string()
}

fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| word.trim_matches('"').replace('"', "\"\""))
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{word}\""))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn extract_links(body: &str, source_path: &str) -> Vec<String> {
    let mut links = Vec::new();
    let excluded = code_ranges(body);
    let wikilinks = extract_wikilinks(body, &excluded);
    for (_, target) in &wikilinks {
        if !links.contains(target) {
            links.push(target.clone());
        }
    }

    let mut code_block_depth = 0;
    for (event, range) in Parser::new_ext(body, Options::all()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code_block_depth += 1,
            Event::End(TagEnd::CodeBlock) => code_block_depth -= 1,
            Event::Start(Tag::Link { dest_url, .. })
                if code_block_depth == 0
                    && !wikilinks
                        .iter()
                        .any(|(wikilink, _)| ranges_overlap(wikilink, &range)) =>
            {
                if let Some(target) = resolve_markdown_target(source_path, dest_url.as_ref()) {
                    if !links.contains(&target) {
                        links.push(target);
                    }
                }
            }
            _ => {}
        }
    }
    links
}

fn code_ranges(body: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut code_block_depth = 0;
    for (event, range) in Parser::new_ext(body, Options::all()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                code_block_depth += 1;
                ranges.push(range);
            }
            Event::End(TagEnd::CodeBlock) => {
                ranges.push(range);
                code_block_depth -= 1;
            }
            Event::Code(_) => ranges.push(range),
            _ if code_block_depth > 0 => ranges.push(range),
            _ => {}
        }
    }
    ranges
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn extract_wikilinks(text: &str, excluded: &[Range<usize>]) -> Vec<(Range<usize>, String)> {
    let mut links = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = text[cursor..].find("[[") {
        let start = cursor + relative_start;
        let content_start = start + 2;
        let Some(relative_end) = text[content_start..].find("]]") else {
            break;
        };
        let content_end = content_start + relative_end;
        let span = start..content_end + 2;
        let raw = &text[content_start..content_end];
        if !raw.contains('\n') && !excluded.iter().any(|range| ranges_overlap(&span, range)) {
            let raw_target = raw.split('|').next().unwrap_or_default();
            if let Some(target) = resolve_wikilink_target(raw_target) {
                links.push((span.clone(), target));
            }
        }
        cursor = span.end;
    }
    links
}

fn local_link_target(target: &str) -> Option<&str> {
    let target = target.trim();
    if target.is_empty()
        || target.starts_with(['#', '?'])
        || target.starts_with("//")
        || has_uri_scheme(target)
    {
        return None;
    }
    target
        .split(['#', '?'])
        .next()
        .map(str::trim)
        .filter(|target| !target.is_empty())
}

fn has_uri_scheme(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.chars().enumerate().all(|(index, character)| {
            if index == 0 {
                character.is_ascii_alphabetic()
            } else {
                character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
            }
        })
}

fn is_diagnostic_source(path: &str) -> bool {
    !path.split('/').any(|component| component.starts_with('.'))
        && !matches!(path.rsplit('/').next(), Some("index.md" | "log.md"))
}

fn is_diagnostic_target(target: &str) -> bool {
    let parts = target.split('/').collect::<Vec<_>>();
    if parts.first() == Some(&".cowiki") {
        return parts.get(1) == Some(&"sources")
            && parts.len() > 2
            && parts[2..].iter().all(|part| !part.starts_with('.'));
    }
    parts.iter().all(|part| !part.starts_with('.'))
}

fn resolve_markdown_target(source_path: &str, target: &str) -> Option<String> {
    let target = local_link_target(target)?;
    let target = percent_decode_str(target).decode_utf8().ok()?;
    let target = target.as_ref();
    let joined = if target.starts_with('/') {
        target.trim_start_matches('/').to_string()
    } else {
        let parent = source_path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        if parent.is_empty() {
            target.to_string()
        } else {
            format!("{parent}/{target}")
        }
    };
    let mut components = Vec::new();
    for component in joined.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            component => components.push(component),
        }
    }
    Some(components.join("/"))
}

fn resolve_wikilink_target(target: &str) -> Option<String> {
    let target = local_link_target(target)?;
    let target = percent_decode_str(target).decode_utf8().ok()?;
    let mut components = Vec::new();
    for component in target.trim_start_matches('/').split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            component => components.push(component),
        }
    }
    let mut normalized = components.join("/");
    if normalized.is_empty() {
        return None;
    }
    if !normalized
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("md"))
    {
        normalized.push_str(".md");
    }
    Some(normalized)
}

fn first_content_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "---" && !line.starts_with("title:"))
        .unwrap_or_default()
        .trim_start_matches('#')
        .trim()
        .to_string()
}
