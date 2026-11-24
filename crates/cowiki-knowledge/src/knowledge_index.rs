//! Rebuildable local knowledge index.
//!
//! Markdown files remain the source of truth. Every row in these tables is
//! derived data and may be dropped and rebuilt without losing user content.

use crate::embeddings::{self, EmbeddingProvider};
use crate::okf;
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const SCHEMA_VERSION: i64 = 4;

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
        doc_type TEXT NOT NULL DEFAULT 'Note',
        status TEXT NOT NULL DEFAULT 'active',
        trust_tier TEXT NOT NULL DEFAULT 'unverified',
        verified INTEGER NOT NULL DEFAULT 0,
        generated INTEGER NOT NULL DEFAULT 0,
        stale_after TEXT,
        redirect_to TEXT,
        sources_json TEXT NOT NULL DEFAULT '[]',
        aliases_json TEXT NOT NULL DEFAULT '[]',
        updated_at TEXT,
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
        kind INTEGER NOT NULL DEFAULT 0,
        UNIQUE(space_id, source_path, target, kind)
    );
    CREATE INDEX page_links_target ON page_links(space_id, target);
    CREATE INDEX page_links_source ON page_links(space_id, source_path);
    CREATE TABLE knowledge_units (
        id INTEGER PRIMARY KEY,
        space_id TEXT NOT NULL,
        unit_id TEXT NOT NULL,
        page_path TEXT NOT NULL,
        page_title TEXT NOT NULL,
        section_heading TEXT NOT NULL,
        heading_path TEXT NOT NULL,
        line_start INTEGER NOT NULL,
        line_end INTEGER NOT NULL,
        body TEXT NOT NULL,
        token_count INTEGER NOT NULL,
        content_sha256 TEXT NOT NULL,
        aliases_text TEXT NOT NULL DEFAULT '',
        sources_json TEXT NOT NULL DEFAULT '[]',
        doc_type TEXT NOT NULL DEFAULT 'Note',
        status TEXT NOT NULL DEFAULT 'active',
        trust_tier TEXT NOT NULL DEFAULT 'unverified',
        verified INTEGER NOT NULL DEFAULT 0,
        generated INTEGER NOT NULL DEFAULT 0,
        stale_after TEXT,
        redirect_to TEXT,
        scope TEXT NOT NULL DEFAULT 'default',
        embedding_model TEXT NOT NULL DEFAULT '',
        embedding_blob BLOB NOT NULL DEFAULT X'',
        UNIQUE(space_id, unit_id)
    );
    CREATE INDEX knowledge_units_space_page ON knowledge_units(space_id, page_path);
    CREATE VIRTUAL TABLE knowledge_units_fts USING fts5(
        page_title,
        section_heading,
        aliases_text,
        body,
        content='knowledge_units',
        content_rowid='id',
        tokenize='unicode61 remove_diacritics 2'
    );
    CREATE TRIGGER knowledge_units_insert AFTER INSERT ON knowledge_units BEGIN
        INSERT INTO knowledge_units_fts(rowid, page_title, section_heading, aliases_text, body)
        VALUES (new.id, new.page_title, new.section_heading, new.aliases_text, new.body);
    END;
    CREATE TRIGGER knowledge_units_delete AFTER DELETE ON knowledge_units BEGIN
        INSERT INTO knowledge_units_fts(knowledge_units_fts, rowid, page_title, section_heading, aliases_text, body)
        VALUES ('delete', old.id, old.page_title, old.section_heading, old.aliases_text, old.body);
    END;
    CREATE TRIGGER knowledge_units_update AFTER UPDATE ON knowledge_units BEGIN
        INSERT INTO knowledge_units_fts(knowledge_units_fts, rowid, page_title, section_heading, aliases_text, body)
        VALUES ('delete', old.id, old.page_title, old.section_heading, old.aliases_text, old.body);
        INSERT INTO knowledge_units_fts(rowid, page_title, section_heading, aliases_text, body)
        VALUES (new.id, new.page_title, new.section_heading, new.aliases_text, new.body);
    END;
    CREATE TABLE unit_links (
        space_id TEXT NOT NULL,
        source_unit_id TEXT NOT NULL,
        source_path TEXT NOT NULL,
        target TEXT NOT NULL,
        target_anchor TEXT,
        kind INTEGER NOT NULL DEFAULT 0,
        UNIQUE(space_id, source_unit_id, target, kind)
    );
    CREATE INDEX unit_links_source_unit ON unit_links(space_id, source_unit_id);
    CREATE INDEX unit_links_source_path ON unit_links(space_id, source_path);
    CREATE INDEX unit_links_target ON unit_links(space_id, target);
    CREATE TABLE embedding_cache (
        model TEXT NOT NULL,
        content_sha256 TEXT NOT NULL,
        dim INTEGER NOT NULL,
        vector_blob BLOB NOT NULL,
        PRIMARY KEY (model, content_sha256)
    );";

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub schema_version: i64,
    pub indexed_pages: usize,
    pub knowledge_units: usize,
    pub page_links: usize,
    pub unit_links: usize,
    pub cached_embeddings: usize,
    pub embedding_model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HybridSearchOptions {
    pub token_budget: Option<usize>,
    pub max_hops: Option<usize>,
    pub seed_top_k: Option<usize>,
    pub include_backlinks: Option<bool>,
    pub exclude_retired: Option<bool>,
    pub follow_redirects: Option<bool>,
    pub use_okf_governance: Option<bool>,
    pub use_lexical: Option<bool>,
    pub use_vector: Option<bool>,
    pub use_graph_expansion: Option<bool>,
    pub scope_filter: Option<String>,
    pub as_of_date: Option<String>,
}

impl Default for HybridSearchOptions {
    fn default() -> Self {
        Self {
            token_budget: Some(1200),
            max_hops: Some(2),
            seed_top_k: Some(6),
            include_backlinks: Some(true),
            exclude_retired: Some(true),
            follow_redirects: Some(true),
            use_okf_governance: Some(true),
            use_lexical: Some(true),
            use_vector: Some(true),
            use_graph_expansion: Some(true),
            scope_filter: None,
            as_of_date: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HybridEvidenceItem {
    pub unit_id: String,
    pub page_path: String,
    pub slug: String,
    pub page_title: String,
    pub section_heading: String,
    pub heading_path: String,
    pub citation: String,
    pub text: String,
    pub token_count: usize,
    pub score: f64,
    pub retrieval_channel: String,
    pub via_link_from: Option<String>,
    pub doc_type: String,
    pub status: String,
    pub trust_tier: String,
    pub verified: bool,
    pub stale_after: Option<String>,
    pub is_stale: bool,
    pub redirect_to: Option<String>,
    pub redirected_from: Option<String>,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExploredEdge {
    pub from_page: String,
    pub to_page: String,
    pub direction: String,
    pub hop: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FollowedRedirect {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HybridSearchResult {
    pub query: String,
    pub token_budget: usize,
    pub tokens_used: usize,
    pub status: String,
    pub latency_ms: f64,
    pub embedding_model: String,
    pub filtered_retired_count: usize,
    pub followed_redirects: Vec<FollowedRedirect>,
    pub explored_edges: Vec<ExploredEdge>,
    pub evidence: Vec<HybridEvidenceItem>,
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
        ("table", "knowledge_units"),
        ("table", "knowledge_units_fts"),
        ("table", "unit_links"),
        ("table", "embedding_cache"),
        ("trigger", "indexed_pages_insert"),
        ("trigger", "indexed_pages_delete"),
        ("trigger", "indexed_pages_update"),
        ("trigger", "knowledge_units_insert"),
        ("trigger", "knowledge_units_delete"),
        ("trigger", "knowledge_units_update"),
        ("index", "page_links_target"),
        ("index", "page_links_source"),
        ("index", "knowledge_units_space_page"),
        ("index", "unit_links_source_unit"),
        ("index", "unit_links_source_path"),
        ("index", "unit_links_target"),
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
            "SELECT id, space_id, path, title, body, modified_ns, doc_type, status,
                    trust_tier, verified, generated, stale_after, redirect_to,
                    sources_json, aliases_json, updated_at
             FROM indexed_pages LIMIT 0",
        )
        .is_err()
        || connection
            .prepare("SELECT space_id, source_path, target, kind FROM page_links LIMIT 0")
            .is_err()
        || connection
            .prepare(
                "SELECT id, space_id, unit_id, page_path, page_title, section_heading,
                        heading_path, line_start, line_end, body, token_count, content_sha256,
                        aliases_text, sources_json, doc_type, status, trust_tier,
                        verified, generated, stale_after, redirect_to, scope,
                        embedding_model, embedding_blob
                 FROM knowledge_units LIMIT 0",
            )
            .is_err()
        || connection
            .prepare(
                "SELECT space_id, source_unit_id, source_path, target, target_anchor, kind
                 FROM unit_links LIMIT 0",
            )
            .is_err()
        || connection
            .prepare("SELECT model, content_sha256, dim, vector_blob FROM embedding_cache LIMIT 0")
            .is_err()
    {
        return false;
    }
    true
}

/// FTS5's external-content integrity check is proportional to indexed content,
/// so run it lazily on first index access instead of on every application open.
pub fn repair_if_corrupt(connection: &mut Connection) -> Result<(), String> {
    let pages_ok = connection
        .execute(
            "INSERT INTO indexed_pages_fts(indexed_pages_fts, rank)
             VALUES ('integrity-check', 1)",
            [],
        )
        .is_ok();
    let units_ok = pages_ok
        && connection
            .execute(
                "INSERT INTO knowledge_units_fts(knowledge_units_fts, rank)
                 VALUES ('integrity-check', 1)",
                [],
            )
            .is_ok();
    if units_ok {
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
            "DROP TABLE IF EXISTS embedding_cache;
             DROP TABLE IF EXISTS unit_links;
             DROP TRIGGER IF EXISTS knowledge_units_insert;
             DROP TRIGGER IF EXISTS knowledge_units_delete;
             DROP TRIGGER IF EXISTS knowledge_units_update;
             DROP TABLE IF EXISTS knowledge_units_fts;
             DROP TABLE IF EXISTS knowledge_units;
             DROP TRIGGER IF EXISTS indexed_pages_insert;
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
    let embedder = embeddings::default_embedder();
    rebuild_space_with_embedder(connection, space_id, root, embedder.as_ref())
}

pub fn rebuild_space_with_embedder(
    connection: &mut Connection,
    space_id: &str,
    root: &Path,
    embedder: &dyn EmbeddingProvider,
) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction
        .execute("DELETE FROM unit_links WHERE space_id = ?1", [space_id])
        .map_err(|e| e.to_string())?;
    transaction
        .execute(
            "DELETE FROM knowledge_units WHERE space_id = ?1",
            [space_id],
        )
        .map_err(|e| e.to_string())?;
    transaction
        .execute("DELETE FROM page_links WHERE space_id = ?1", [space_id])
        .map_err(|e| e.to_string())?;
    transaction
        .execute("DELETE FROM indexed_pages WHERE space_id = ?1", [space_id])
        .map_err(|e| e.to_string())?;

    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_walk(entry.path(), root))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if entry.file_type().is_file() && is_markdown(path) {
            index_file_in(&transaction, space_id, root, path, embedder)?;
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
    let embedder = embeddings::default_embedder();
    refresh_space_with_embedder(connection, space_id, root, embedder.as_ref())
}

pub fn refresh_space_with_embedder(
    connection: &mut Connection,
    space_id: &str,
    root: &Path,
    embedder: &dyn EmbeddingProvider,
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
        .follow_links(false)
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
            index_file_in(&transaction, space_id, root, path, embedder)?;
        }
    }
    for deleted in existing.keys().filter(|path| !seen.contains(*path)) {
        delete_page_rows(&transaction, space_id, deleted)?;
    }
    transaction.commit().map_err(|e| e.to_string())
}

/// Re-index only the given Space-relative Markdown paths. A product write
/// touches one page, so re-reading every page in a large vault on each save
/// would make autosave cost grow with the vault size. Missing files are
/// removed from the index.
pub fn index_paths(
    connection: &mut Connection,
    space_id: &str,
    root: &Path,
    paths: &[PathBuf],
) -> Result<(), String> {
    let embedder = embeddings::default_embedder();
    index_paths_with_embedder(connection, space_id, root, paths, embedder.as_ref())
}

pub fn index_paths_with_embedder(
    connection: &mut Connection,
    space_id: &str,
    root: &Path,
    paths: &[PathBuf],
    embedder: &dyn EmbeddingProvider,
) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for relative in paths {
        let path = root.join(relative);
        if !is_markdown(&path) || !should_walk(&path, root) {
            continue;
        }
        let is_regular_file = std::fs::symlink_metadata(&path)
            .map(|meta| meta.file_type().is_file())
            .unwrap_or(false);
        if is_regular_file {
            index_file_in(&transaction, space_id, root, &path, embedder)?;
        } else {
            let relative = normalize_path(relative);
            delete_page_rows(&transaction, space_id, &relative)?;
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

fn delete_page_rows(connection: &Connection, space_id: &str, relative: &str) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM unit_links WHERE space_id = ?1 AND source_path = ?2",
            params![space_id, relative],
        )
        .map_err(|e| e.to_string())?;
    connection
        .execute(
            "DELETE FROM knowledge_units WHERE space_id = ?1 AND page_path = ?2",
            params![space_id, relative],
        )
        .map_err(|e| e.to_string())?;
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
    Ok(())
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
        for (target, kind) in extract_links(&body, &source_path) {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO page_links (space_id, source_path, target, kind)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![space_id, source_path, target, kind as i64],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

pub fn index_stats(connection: &Connection, space_id: &str) -> Result<IndexStats, String> {
    let count_for = |sql: &str, with_space: bool| -> Result<usize, String> {
        let count: i64 = if with_space {
            connection
                .query_row(sql, [space_id], |row| row.get(0))
                .map_err(|e| e.to_string())?
        } else {
            connection
                .query_row(sql, [], |row| row.get(0))
                .map_err(|e| e.to_string())?
        };
        Ok(count.max(0) as usize)
    };
    let model: String = connection
        .query_row(
            "SELECT embedding_model FROM knowledge_units
             WHERE space_id = ?1 AND embedding_model != '' LIMIT 1",
            [space_id],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| embeddings::default_embedder().model_name().to_string());
    Ok(IndexStats {
        schema_version: SCHEMA_VERSION,
        indexed_pages: count_for(
            "SELECT COUNT(*) FROM indexed_pages WHERE space_id = ?1",
            true,
        )?,
        knowledge_units: count_for(
            "SELECT COUNT(*) FROM knowledge_units WHERE space_id = ?1",
            true,
        )?,
        page_links: count_for("SELECT COUNT(*) FROM page_links WHERE space_id = ?1", true)?,
        unit_links: count_for("SELECT COUNT(*) FROM unit_links WHERE space_id = ?1", true)?,
        cached_embeddings: count_for("SELECT COUNT(*) FROM embedding_cache", false)?,
        embedding_model: model,
    })
}

fn index_file_in(
    connection: &Connection,
    space_id: &str,
    root: &Path,
    path: &Path,
    embedder: &dyn EmbeddingProvider,
) -> Result<(), String> {
    let relative = relative_path(root, path)?;
    let body = match std::fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            delete_page_rows(connection, space_id, &relative)?;
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };
    let meta = okf::parse_metadata(&body);
    let title = meta
        .title
        .clone()
        .or_else(|| okf::display_metadata(&body).0)
        .unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
    let resolved_redirect = meta
        .redirect_to
        .as_deref()
        .and_then(|target| resolve_markdown_target(&relative, target));
    let sources_json = serde_json::to_string(&meta.sources).unwrap_or_else(|_| "[]".to_string());
    let aliases_json = serde_json::to_string(&meta.aliases).unwrap_or_else(|_| "[]".to_string());
    let aliases_text = meta.aliases.join(" ");
    let modified_ns = file_modified_ns(path);

    connection
        .execute(
            "INSERT INTO indexed_pages (
                space_id, path, title, body, modified_ns, doc_type, status,
                trust_tier, verified, generated, stale_after, redirect_to,
                sources_json, aliases_json, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(space_id, path) DO UPDATE SET
               title = excluded.title,
               body = excluded.body,
               modified_ns = excluded.modified_ns,
               doc_type = excluded.doc_type,
               status = excluded.status,
               trust_tier = excluded.trust_tier,
               verified = excluded.verified,
               generated = excluded.generated,
               stale_after = excluded.stale_after,
               redirect_to = excluded.redirect_to,
               sources_json = excluded.sources_json,
               aliases_json = excluded.aliases_json,
               updated_at = excluded.updated_at",
            params![
                space_id,
                relative,
                title,
                body,
                modified_ns,
                meta.doc_type,
                meta.status,
                meta.trust_tier,
                i64::from(meta.verified),
                i64::from(meta.generated),
                meta.stale_after,
                resolved_redirect,
                sources_json,
                aliases_json,
                meta.updated_at,
            ],
        )
        .map_err(|e| e.to_string())?;
    connection
        .execute(
            "DELETE FROM page_links WHERE space_id = ?1 AND source_path = ?2",
            params![space_id, relative],
        )
        .map_err(|e| e.to_string())?;
    for (target, kind) in extract_links(&body, &relative) {
        connection
            .execute(
                "INSERT OR IGNORE INTO page_links (space_id, source_path, target, kind)
                 VALUES (?1, ?2, ?3, ?4)",
                params![space_id, relative, target, kind as i64],
            )
            .map_err(|e| e.to_string())?;
    }
    connection
        .execute(
            "DELETE FROM unit_links WHERE space_id = ?1 AND source_path = ?2",
            params![space_id, relative],
        )
        .map_err(|e| e.to_string())?;
    connection
        .execute(
            "DELETE FROM knowledge_units WHERE space_id = ?1 AND page_path = ?2",
            params![space_id, relative],
        )
        .map_err(|e| e.to_string())?;
    if !is_reserved_nav_doc(&relative) {
        let chunks = chunk_markdown_page(&relative, &title, meta.description.as_deref(), &body);
        if !chunks.is_empty() {
            let model = embedder.model_name();
            let mut blobs: Vec<Option<Vec<u8>>> = vec![None; chunks.len()];
            let mut missing_indices: Vec<usize> = Vec::new();
            let mut missing_texts: Vec<String> = Vec::new();
            let mut cache_keys: Vec<String> = Vec::with_capacity(chunks.len());

            for (idx, chunk) in chunks.iter().enumerate() {
                let embed_text = format!(
                    "{} | {} | {}\n{}",
                    title, chunk.section_heading, aliases_text, chunk.body
                );
                let cache_key = format!("{:x}", Sha256::digest(embed_text.as_bytes()));
                if let Ok(cached_blob) = connection.query_row(
                    "SELECT vector_blob FROM embedding_cache WHERE model = ?1 AND content_sha256 = ?2",
                    params![model, cache_key],
                    |row| row.get::<_, Vec<u8>>(0),
                ) {
                    blobs[idx] = Some(cached_blob);
                } else {
                    missing_indices.push(idx);
                    missing_texts.push(embed_text);
                }
                cache_keys.push(cache_key);
            }

            if !missing_texts.is_empty() {
                let vectors = embedder.embed_batch(&missing_texts)?;
                for (&idx, vec) in missing_indices.iter().zip(vectors) {
                    let blob = embeddings::encode_vector(&vec);
                    let dim = vec.len() as i64;
                    connection
                        .execute(
                            "INSERT OR REPLACE INTO embedding_cache (model, content_sha256, dim, vector_blob)
                             VALUES (?1, ?2, ?3, ?4)",
                            params![model, cache_keys[idx], dim, blob],
                        )
                        .map_err(|e| e.to_string())?;
                    blobs[idx] = Some(blob);
                }
            }

            for (idx, chunk) in chunks.into_iter().enumerate() {
                let blob = blobs[idx].take().unwrap_or_default();
                connection
                    .execute(
                        "INSERT INTO knowledge_units (
                            space_id, unit_id, page_path, page_title, section_heading,
                            heading_path, line_start, line_end, body, token_count,
                            content_sha256, aliases_text, sources_json, doc_type, status,
                            trust_tier, verified, generated, stale_after, redirect_to,
                            scope, embedding_model, embedding_blob
                         )
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                        params![
                            space_id,
                            chunk.unit_id,
                            relative,
                            title,
                            chunk.section_heading,
                            chunk.heading_path,
                            chunk.line_start as i64,
                            chunk.line_end as i64,
                            chunk.body,
                            chunk.token_count as i64,
                            chunk.content_sha256,
                            aliases_text,
                            sources_json,
                            meta.doc_type,
                            meta.status,
                            meta.trust_tier,
                            i64::from(meta.verified),
                            i64::from(meta.generated),
                            meta.stale_after,
                            resolved_redirect,
                            meta.scope,
                            model,
                            blob,
                        ],
                    )
                    .map_err(|e| e.to_string())?;

                for (target, anchor, kind) in extract_links_with_anchors(&chunk.body, &relative) {
                    connection
                        .execute(
                            "INSERT OR IGNORE INTO unit_links (space_id, source_unit_id, source_path, target, target_anchor, kind)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![space_id, chunk.unit_id, relative, target, anchor, kind as i64],
                        )
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}

pub fn search(
    connection: &Connection,
    space_id: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let and_query = fts_query(query);
    if and_query.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let lim = limit.min(100);
    let fetch_limit = (lim * 4).clamp(24, 200) as i64;
    let title_needle = format!("%{}%", query.trim().to_lowercase());

    let mut raw_rows = run_page_fts(connection, space_id, &and_query, &title_needle, fetch_limit)?;
    if raw_rows.is_empty() {
        let or_query = fts_or_query(query);
        if !or_query.is_empty() && or_query != and_query {
            raw_rows = run_page_fts(connection, space_id, &or_query, &title_needle, fetch_limit)?;
        }
    }
    if raw_rows.is_empty() {
        return Ok(Vec::new());
    }

    let page_gov = load_page_governance(connection, space_id)?;
    let today = okf::utc_today_iso();
    let mut best_by_path: HashMap<String, (f64, SearchHit)> = HashMap::new();

    for (rank_idx, row) in raw_rows.into_iter().enumerate() {
        if is_reserved_nav_doc(&row.path) {
            continue;
        }
        if row.status.eq_ignore_ascii_case("retired") {
            continue;
        }
        let base_score =
            (if row.title_match { 0.25 } else { 0.0 }) + (1.0 / (12.0 + rank_idx as f64));
        let (mult, _) = okf_governance_multiplier(
            &row.status,
            &row.trust_tier,
            row.verified,
            row.stale_after.as_deref(),
            &today,
            true,
        );
        if mult <= 0.0 {
            continue;
        }
        let mut final_score = base_score * mult;
        let mut final_hit = SearchHit {
            slug: slug_for_path(&row.path),
            path: row.path.clone(),
            title: row.title,
            snippet: row.snippet,
            title_match: row.title_match,
        };

        if row.redirect_to.is_some() {
            if let Some(target_path) = resolve_redirect_chain(&row.path, &page_gov, 4) {
                if let Some(target_gov) = page_gov.get(&target_path) {
                    if !target_gov.status.eq_ignore_ascii_case("retired") {
                        let (t_mult, _) = okf_governance_multiplier(
                            &target_gov.status,
                            &target_gov.trust_tier,
                            target_gov.verified,
                            target_gov.stale_after.as_deref(),
                            &today,
                            true,
                        );
                        if t_mult > 0.0 {
                            let target_title_match = target_gov
                                .title
                                .to_lowercase()
                                .contains(&query.trim().to_lowercase());
                            final_score = base_score * t_mult;
                            final_hit = SearchHit {
                                slug: slug_for_path(&target_path),
                                path: target_path,
                                title: target_gov.title.clone(),
                                snippet: target_gov.snippet.clone(),
                                title_match: target_title_match,
                            };
                        }
                    } else {
                        continue;
                    }
                }
            }
        }

        let replace = match best_by_path.get(&final_hit.path) {
            None => true,
            Some((prev_score, prev_hit)) => {
                final_score > *prev_score || (!prev_hit.title_match && final_hit.title_match)
            }
        };
        if replace {
            best_by_path.insert(final_hit.path.clone(), (final_score, final_hit));
        }
    }

    let mut ranked: Vec<(f64, SearchHit)> = best_by_path.into_values().collect();
    ranked.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.path.cmp(&right.1.path))
    });
    ranked.truncate(lim);
    Ok(ranked.into_iter().map(|(_, hit)| hit).collect())
}

struct RawPageHit {
    path: String,
    title: String,
    snippet: String,
    title_match: bool,
    status: String,
    trust_tier: String,
    verified: bool,
    stale_after: Option<String>,
    redirect_to: Option<String>,
}

fn run_page_fts(
    connection: &Connection,
    space_id: &str,
    fts_expr: &str,
    title_needle: &str,
    fetch_limit: i64,
) -> Result<Vec<RawPageHit>, String> {
    let mut statement = connection
        .prepare(
            "SELECT page.path,
                    page.title,
                    snippet(indexed_pages_fts, 1, '', '', ' … ', 24),
                    lower(page.title) LIKE ?3 AS title_match,
                    page.status,
                    page.trust_tier,
                    page.verified,
                    page.stale_after,
                    page.redirect_to
             FROM indexed_pages_fts
             JOIN indexed_pages AS page ON page.id = indexed_pages_fts.rowid
             WHERE indexed_pages_fts MATCH ?1 AND page.space_id = ?2
             ORDER BY title_match DESC, bm25(indexed_pages_fts, 8.0, 1.0) ASC, page.path ASC
             LIMIT ?4",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(
            params![fts_expr, space_id, title_needle, fetch_limit],
            |row| {
                Ok(RawPageHit {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    snippet: row.get::<_, String>(2)?.replace('\n', " "),
                    title_match: row.get(3)?,
                    status: row.get(4)?,
                    trust_tier: row.get(5)?,
                    verified: row.get::<_, i64>(6)? != 0,
                    stale_after: row.get(7)?,
                    redirect_to: row.get(8)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

struct PageGovernanceInfo {
    title: String,
    snippet: String,
    status: String,
    trust_tier: String,
    verified: bool,
    stale_after: Option<String>,
    redirect_to: Option<String>,
}

fn load_page_governance(
    connection: &Connection,
    space_id: &str,
) -> Result<HashMap<String, PageGovernanceInfo>, String> {
    let mut statement = connection
        .prepare(
            "SELECT path, title, body, status, trust_tier, verified, stale_after, redirect_to
             FROM indexed_pages WHERE space_id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([space_id], |row| {
            let body: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                PageGovernanceInfo {
                    title: row.get(1)?,
                    snippet: first_content_line(&body),
                    status: row.get(3)?,
                    trust_tier: row.get(4)?,
                    verified: row.get::<_, i64>(5)? != 0,
                    stale_after: row.get(6)?,
                    redirect_to: row.get(7)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<HashMap<_, _>, _>>()
        .map_err(|e| e.to_string())
}

fn resolve_redirect_chain(
    start_path: &str,
    pages: &HashMap<String, PageGovernanceInfo>,
    max_depth: usize,
) -> Option<String> {
    let mut current = start_path;
    let mut visited: HashSet<&str> = HashSet::new();
    visited.insert(current);
    for _ in 0..max_depth {
        let Some(next) = pages
            .get(current)
            .and_then(|info| info.redirect_to.as_deref())
            .filter(|value| !value.is_empty())
        else {
            break;
        };
        if !visited.insert(next) {
            return None;
        }
        current = next;
    }
    if current != start_path {
        Some(current.to_string())
    } else {
        None
    }
}

fn okf_governance_multiplier(
    status: &str,
    trust_tier: &str,
    verified: bool,
    stale_after: Option<&str>,
    reference_date: &str,
    exclude_retired: bool,
) -> (f64, bool) {
    let meta = okf::OkfMetadata {
        status: status.to_string(),
        trust_tier: trust_tier.to_string(),
        verified,
        stale_after: stale_after.map(str::to_string),
        ..okf::OkfMetadata::default()
    };
    meta.governance_multiplier(Some(reference_date), exclude_retired)
}

/// Budgeted section-level hybrid retrieval (BM25 + dense semantic vectors via RRF +
/// anchor-aware hub-dampened link-graph expansion) with OKF v0.3 governance filtering,
/// redirect resolution, and line-range citations.
pub fn hybrid_search(
    connection: &Connection,
    space_id: &str,
    root: &Path,
    query: &str,
    options: &HybridSearchOptions,
) -> Result<HybridSearchResult, String> {
    let embedder = embeddings::default_embedder();
    hybrid_search_with_embedder(
        connection,
        space_id,
        root,
        query,
        options,
        embedder.as_ref(),
    )
}

pub fn hybrid_search_with_embedder(
    connection: &Connection,
    space_id: &str,
    root: &Path,
    query: &str,
    options: &HybridSearchOptions,
    embedder: &dyn EmbeddingProvider,
) -> Result<HybridSearchResult, String> {
    hybrid_search_subset(connection, space_id, root, query, options, embedder, false)
}

// The historical pass filters *before* the lexical/vector candidate limits.
// Otherwise active copies of a popular claim can hide its withdrawn version.
pub(crate) fn hybrid_search_subset(
    connection: &Connection,
    space_id: &str,
    root: &Path,
    query: &str,
    options: &HybridSearchOptions,
    embedder: &dyn EmbeddingProvider,
    retired_only: bool,
) -> Result<HybridSearchResult, String> {
    let started = Instant::now();
    let token_budget = options.token_budget.unwrap_or(1200).clamp(64, 16_000);
    let max_hops = options.max_hops.unwrap_or(2).min(2);
    let seed_top_k = options.seed_top_k.unwrap_or(6).clamp(1, 20);
    let include_backlinks = options.include_backlinks.unwrap_or(true);
    let exclude_retired = options.exclude_retired.unwrap_or(true);
    let follow_redirects = options.follow_redirects.unwrap_or(true);
    let use_governance = options.use_okf_governance.unwrap_or(true);
    let use_lexical = options.use_lexical.unwrap_or(true);
    let use_vector = options.use_vector.unwrap_or(true);
    let use_graph_expansion = options.use_graph_expansion.unwrap_or(true);
    let scope_filter = options
        .scope_filter
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let reference_date = options
        .as_of_date
        .as_deref()
        .map(str::trim)
        .filter(|value| okf::valid_iso_date(value))
        .map(str::to_string)
        .unwrap_or_else(okf::utc_today_iso);
    let model_name = embedder.model_name().to_string();

    if query.trim().is_empty() || (!use_lexical && !use_vector) {
        return Ok(HybridSearchResult {
            query: query.to_string(),
            token_budget,
            tokens_used: 0,
            status: "empty".to_string(),
            latency_ms: round_ms(started.elapsed().as_secs_f64() * 1000.0),
            embedding_model: model_name,
            filtered_retired_count: 0,
            followed_redirects: Vec::new(),
            explored_edges: Vec::new(),
            evidence: Vec::new(),
        });
    }

    let page_gov = load_page_governance(connection, space_id)?;
    let query_terms = extract_query_terms(query);
    let query_lower = query.trim().to_lowercase();

    // 1. Lexical BM25 candidate ranks over knowledge_units_fts.
    let mut lexical_ranks: HashMap<String, usize> = HashMap::new();
    if use_lexical {
        let or_query = fts_or_query(query);
        if !or_query.is_empty() {
            let mut seed_stmt = connection
                .prepare(
                    "SELECT u.unit_id,
                            bm25(knowledge_units_fts, 3.0, 2.2, 2.5, 1.0) AS rank_score
                     FROM knowledge_units_fts
                     JOIN knowledge_units AS u ON u.id = knowledge_units_fts.rowid
                     WHERE knowledge_units_fts MATCH ?1 AND u.space_id = ?2 AND (?3 = 0 OR u.status = 'retired')
                     ORDER BY rank_score ASC, u.unit_id ASC
                     LIMIT 64",
                )
                .map_err(|e| e.to_string())?;
            let rows = seed_stmt
                .query_map(params![or_query, space_id, retired_only], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for (idx, uid) in rows.into_iter().enumerate() {
                lexical_ranks.insert(uid, idx + 1);
            }
        }
    }

    // 2. Dense semantic vector similarity over stored knowledge_units embeddings.
    let mut vector_ranks: HashMap<String, usize> = HashMap::new();
    let mut vector_sims: HashMap<String, f64> = HashMap::new();
    if use_vector {
        let query_vec = embedder.embed_one(query)?;
        let mut vec_stmt = connection
            .prepare(
                "SELECT unit_id, embedding_model, embedding_blob
                 FROM knowledge_units
                 WHERE space_id = ?1 AND (?2 = 0 OR status = 'retired')",
            )
            .map_err(|e| e.to_string())?;
        let rows = vec_stmt
            .query_map(params![space_id, retired_only], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut scored: Vec<(f64, String)> = Vec::with_capacity(rows.len());
        for (uid, unit_model, blob) in rows {
            if unit_model != model_name || blob.is_empty() {
                continue;
            }
            let unit_vec = embeddings::decode_vector(&blob);
            let sim = f64::from(embeddings::cosine_similarity(&query_vec, &unit_vec));
            vector_sims.insert(uid.clone(), sim);
            scored.push((sim, uid));
        }
        scored.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        for (idx, (sim, uid)) in scored.into_iter().take(36).enumerate() {
            if sim >= 0.14 || (idx < 3 && sim >= 0.10) {
                vector_ranks.insert(uid, idx + 1);
            }
        }
    }

    // 3. Reciprocal Rank Fusion (RRF) + OKF v0.3 governance + redirect resolution.
    let mut candidate_ids: Vec<String> = lexical_ranks
        .keys()
        .chain(vector_ranks.keys())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    candidate_ids.sort();
    let candidate_units = load_units_by_ids(connection, space_id, &candidate_ids)?;

    let mut filtered_retired_count = 0usize;
    let mut followed_redirects: Vec<FollowedRedirect> = Vec::new();
    let mut seen_redirect_pairs: HashSet<(String, String)> = HashSet::new();
    // unit_id -> (score, channel, redirected_from)
    let mut raw_seed_scores: HashMap<String, (f64, String, Option<String>)> = HashMap::new();

    for uid in &candidate_ids {
        let Some(unit) = candidate_units.get(uid) else {
            continue;
        };
        if is_reserved_nav_doc(&unit.page_path) {
            continue;
        }
        if let Some(scope) = scope_filter {
            if unit.scope != scope && unit.scope != "default" {
                continue;
            }
        }
        if use_governance && exclude_retired && unit.status.eq_ignore_ascii_case("retired") {
            filtered_retired_count += 1;
            continue;
        }

        let lex_rank = lexical_ranks.get(uid).copied();
        let vec_rank = vector_ranks.get(uid).copied();
        let vec_sim = vector_sims.get(uid).copied().unwrap_or(0.0).max(0.0);
        let mut raw_score = 0.0_f64;
        if let Some(rank) = lex_rank {
            raw_score += 1.0 / (16.0 + rank as f64);
        }
        if let Some(rank) = vec_rank {
            raw_score += 1.05 / (16.0 + rank as f64) + 0.14 * vec_sim;
        }
        let channel = match (lex_rank.is_some(), vec_rank.is_some()) {
            (true, true) => {
                raw_score += 0.018;
                "seed_hybrid"
            }
            (true, false) => "seed_lexical",
            (false, true) => "seed_vector",
            (false, false) => continue,
        };

        let haystack =
            format!("{} {} {}", unit.page_title, unit.section_heading, unit.body).to_lowercase();
        let coverage = query_term_coverage(&query_terms, &haystack);
        raw_score += 0.05 * coverage;
        if !query_terms.is_empty() && (coverage - 1.0).abs() < f64::EPSILON {
            raw_score += 0.025;
        }
        let title_low = unit.page_title.trim().to_lowercase();
        if title_low.len() >= 4 && query_lower.contains(&title_low) {
            raw_score += 0.045;
        }

        if use_governance && follow_redirects && unit.redirect_to.is_some() {
            if let Some(target_page) = resolve_redirect_chain(&unit.page_path, &page_gov, 4) {
                let target_retired = page_gov
                    .get(&target_page)
                    .is_some_and(|info| info.status.eq_ignore_ascii_case("retired"));
                if target_retired && exclude_retired {
                    filtered_retired_count += 1;
                    continue;
                }
                let target_units = load_units_for_page(connection, space_id, &target_page)?;
                if !target_units.is_empty() {
                    let pair = (unit.page_path.clone(), target_page.clone());
                    if seen_redirect_pairs.insert(pair) {
                        followed_redirects.push(FollowedRedirect {
                            from: unit.page_path.clone(),
                            to: target_page.clone(),
                        });
                    }
                    for target_unit in target_units {
                        if let Some(scope) = scope_filter {
                            if target_unit.scope != scope && target_unit.scope != "default" {
                                continue;
                            }
                        }
                        let (t_mult, _) = okf_governance_multiplier(
                            &target_unit.status,
                            &target_unit.trust_tier,
                            target_unit.verified,
                            target_unit.stale_after.as_deref(),
                            &reference_date,
                            exclude_retired,
                        );
                        if t_mult <= 0.0 {
                            continue;
                        }
                        let t_score = (raw_score * 1.02) * t_mult;
                        let update = raw_seed_scores
                            .get(&target_unit.unit_id)
                            .is_none_or(|prev| t_score > prev.0);
                        if update {
                            raw_seed_scores.insert(
                                target_unit.unit_id,
                                (
                                    t_score,
                                    "redirect_target".to_string(),
                                    Some(unit.page_path.clone()),
                                ),
                            );
                        }
                    }
                    continue;
                }
            }
        }

        let mut score = raw_score;
        if use_governance {
            let (mult, _) = okf_governance_multiplier(
                &unit.status,
                &unit.trust_tier,
                unit.verified,
                unit.stale_after.as_deref(),
                &reference_date,
                exclude_retired,
            );
            if mult <= 0.0 {
                continue;
            }
            score *= mult;
        }

        let update = raw_seed_scores.get(uid).is_none_or(|prev| score > prev.0);
        if update {
            raw_seed_scores.insert(uid.clone(), (score, channel.to_string(), None));
        }
    }

    let mut seed_list: Vec<(f64, String, String, Option<String>)> = raw_seed_scores
        .into_iter()
        .map(|(uid, (score, ch, redir))| (score, uid, ch, redir))
        .collect();
    seed_list.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    seed_list.truncate(seed_top_k);
    let seed_uid_set: HashSet<String> = seed_list.iter().map(|item| item.1.clone()).collect();

    // 4. Anchor-aware, hub-dampened 1-hop / 2-hop link-graph expansion.
    let mut graph_candidates: HashMap<String, (f64, String, String, Option<String>)> =
        HashMap::new();
    let mut explored_edges: Vec<ExploredEdge> = Vec::new();

    if use_graph_expansion && max_hops > 0 && !seed_list.is_empty() {
        let link_graph = build_resolved_link_graph(connection, space_id, root)?;
        let (heading_df, total_pages) = load_heading_stats(connection, space_id)?;
        let seed_pages = load_unit_pages(connection, space_id, &seed_uid_set)?;

        struct FrontierEntry {
            page_path: String,
            source_unit_id: Option<String>,
            parent_score: f64,
            hop: usize,
        }

        let mut frontier: Vec<FrontierEntry> = Vec::new();
        let mut expanded_pages: HashSet<String> = HashSet::new();
        for (score, uid, _, _) in seed_list.iter().take(5) {
            if let Some(page) = seed_pages.get(uid) {
                if expanded_pages.insert(page.clone()) {
                    frontier.push(FrontierEntry {
                        page_path: page.clone(),
                        source_unit_id: Some(uid.clone()),
                        parent_score: *score,
                        hop: 1,
                    });
                }
            }
        }

        for hop in 1..=max_hops {
            let current_batch: Vec<(String, Option<String>, f64)> = frontier
                .iter()
                .filter(|item| item.hop == hop)
                .map(|item| {
                    (
                        item.page_path.clone(),
                        item.source_unit_id.clone(),
                        item.parent_score,
                    )
                })
                .collect();
            if current_batch.is_empty() {
                break;
            }
            let mut next_frontier: Vec<(String, f64)> = Vec::new();
            for (src_page, src_unit_opt, parent_score) in current_batch {
                // Target page -> (edge_weight, direction, optional_anchor)
                let mut neighbors: Vec<(String, f64, &'static str, Option<String>)> = Vec::new();
                let mut seen_targets: HashSet<String> = HashSet::new();

                // Prefer section-level outgoing links (carrying `#anchor` hints) when available.
                if let Some(src_uid) = src_unit_opt.as_deref() {
                    if let Some(unit_outs) = link_graph.unit_outgoing.get(src_uid) {
                        for (target, anchor) in unit_outs {
                            if seen_targets.insert(target.clone()) {
                                neighbors.push((target.clone(), 1.05, "outgoing", anchor.clone()));
                            }
                        }
                    }
                }
                if let Some(outs) = link_graph.outgoing.get(&src_page) {
                    for target in outs {
                        if seen_targets.insert(target.clone()) {
                            let anchor = link_graph
                                .page_anchors
                                .get(&(src_page.clone(), target.clone()))
                                .cloned();
                            neighbors.push((target.clone(), 1.0, "outgoing", anchor));
                        }
                    }
                }
                if include_backlinks && hop == 1 {
                    if let Some(backs) = link_graph.backlinks.get(&src_page) {
                        for source in backs.iter().take(18) {
                            if seen_targets.insert(source.clone()) {
                                neighbors.push((source.clone(), 0.88, "backlink", None));
                            }
                        }
                    }
                }

                for (raw_target, edge_weight, direction, target_anchor) in neighbors {
                    if is_reserved_nav_doc(&raw_target) || raw_target == src_page {
                        continue;
                    }
                    let mut target_page = raw_target.clone();
                    let mut redir_origin: Option<String> = None;
                    if use_governance && follow_redirects {
                        if let Some(resolved) = resolve_redirect_chain(&target_page, &page_gov, 4) {
                            let pair = (target_page.clone(), resolved.clone());
                            if seen_redirect_pairs.insert(pair) {
                                followed_redirects.push(FollowedRedirect {
                                    from: target_page.clone(),
                                    to: resolved.clone(),
                                });
                            }
                            redir_origin = Some(target_page);
                            target_page = resolved;
                        }
                    }
                    if use_governance
                        && exclude_retired
                        && page_gov
                            .get(&target_page)
                            .is_some_and(|info| info.status.eq_ignore_ascii_case("retired"))
                    {
                        filtered_retired_count += 1;
                        continue;
                    }

                    let target_units = load_units_for_page(connection, space_id, &target_page)?;
                    if target_units.is_empty() {
                        continue;
                    }
                    if explored_edges.len() < 32 {
                        explored_edges.push(ExploredEdge {
                            from_page: src_page.clone(),
                            to_page: target_page.clone(),
                            direction: direction.to_string(),
                            hop,
                        });
                    }

                    let out_deg = link_graph
                        .outgoing
                        .get(&target_page)
                        .map(|list| list.len())
                        .unwrap_or(0);
                    let hub_damp = 1.0 / (1.0 + 0.12 * out_deg.saturating_sub(3) as f64);
                    let hop_decay = if hop == 1 { 0.86 } else { 0.62 };
                    let mut best_page_score = 0.0_f64;

                    for unit in target_units {
                        if seed_uid_set.contains(&unit.unit_id) {
                            continue;
                        }
                        if let Some(scope) = scope_filter {
                            if unit.scope != scope && unit.scope != "default" {
                                continue;
                            }
                        }
                        if use_governance
                            && exclude_retired
                            && unit.status.eq_ignore_ascii_case("retired")
                        {
                            filtered_retired_count += 1;
                            continue;
                        }
                        let haystack =
                            format!("{} {} {}", unit.page_title, unit.section_heading, unit.body)
                                .to_lowercase();
                        let lex_overlap = query_term_coverage(&query_terms, &haystack);
                        let dense_sim = vector_sims
                            .get(&unit.unit_id)
                            .copied()
                            .unwrap_or(0.0)
                            .max(0.0);
                        let anchor_bonus = if target_anchor.as_deref().is_some_and(|anchor| {
                            heading_matches_anchor(&unit.section_heading, anchor)
                        }) {
                            0.055
                        } else {
                            0.0
                        };
                        let h_df = heading_df
                            .get(&unit.section_heading.to_lowercase())
                            .copied()
                            .unwrap_or(1) as f64;
                        let total_p = total_pages.max(1) as f64;
                        let heading_specificity =
                            ((total_p + 1.0) / (h_df + 0.5)).ln() / (total_p + 1.5).ln();
                        let mut nbr_score = parent_score
                            * edge_weight
                            * hop_decay
                            * hub_damp
                            * (0.55 + 0.45 * heading_specificity.clamp(0.0, 1.0))
                            + 0.14 * dense_sim
                            + 0.08 * lex_overlap
                            + anchor_bonus;
                        if use_governance {
                            let (mult, _) = okf_governance_multiplier(
                                &unit.status,
                                &unit.trust_tier,
                                unit.verified,
                                unit.stale_after.as_deref(),
                                &reference_date,
                                exclude_retired,
                            );
                            if mult <= 0.0 {
                                continue;
                            }
                            nbr_score *= mult;
                        }
                        if nbr_score > best_page_score {
                            best_page_score = nbr_score;
                        }
                        let channel = if hop == 1 {
                            "graph_hop_1".to_string()
                        } else {
                            "graph_hop_2".to_string()
                        };
                        let via = format!("{src_page} -> {target_page} ({direction})");
                        let update = graph_candidates
                            .get(&unit.unit_id)
                            .is_none_or(|prev| nbr_score > prev.0);
                        if update {
                            graph_candidates.insert(
                                unit.unit_id,
                                (nbr_score, channel, via, redir_origin.clone()),
                            );
                        }
                    }

                    if hop < max_hops
                        && best_page_score > 0.0
                        && expanded_pages.insert(target_page.clone())
                    {
                        next_frontier.push((target_page, best_page_score));
                    }
                }
            }
            next_frontier.sort_by(|left, right| right.1.total_cmp(&left.1));
            for (page_path, parent_score) in next_frontier.into_iter().take(4) {
                frontier.push(FrontierEntry {
                    page_path,
                    source_unit_id: None,
                    parent_score,
                    hop: hop + 1,
                });
            }
        }
    }

    // 5. Budgeted diversity-aware context packing.
    struct PoolCandidate {
        score: f64,
        unit_id: String,
        channel: String,
        via: Option<String>,
        redirected_from: Option<String>,
    }

    let mut unified_pool: Vec<PoolCandidate> = Vec::new();
    for (score, unit_id, channel, redirected_from) in seed_list {
        unified_pool.push(PoolCandidate {
            score,
            unit_id,
            channel,
            via: None,
            redirected_from,
        });
    }
    for (unit_id, (score, channel, via, redirected_from)) in graph_candidates {
        unified_pool.push(PoolCandidate {
            score,
            unit_id,
            channel,
            via: Some(via),
            redirected_from,
        });
    }
    unified_pool.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.unit_id.cmp(&right.unit_id))
    });
    unified_pool.truncate(48);

    let candidate_uids: Vec<String> = unified_pool
        .iter()
        .map(|item| item.unit_id.clone())
        .collect();
    let units_by_id = load_units_by_ids(connection, space_id, &candidate_uids)?;

    let top_ref_score = unified_pool.first().map(|item| item.score).unwrap_or(0.0);
    let min_marginal_cutoff = top_ref_score * 0.20;
    let mut remaining = unified_pool;
    let mut packed: Vec<HybridEvidenceItem> = Vec::new();
    let mut seen_hashes: HashSet<String> = HashSet::new();
    let mut page_counts: HashMap<String, usize> = HashMap::new();
    let mut tokens_used = 0usize;

    while !remaining.is_empty() && tokens_used < token_budget {
        let mut best_idx: Option<usize> = None;
        let mut best_marginal = f64::NEG_INFINITY;
        for (idx, candidate) in remaining.iter().enumerate() {
            let Some(unit) = units_by_id.get(&candidate.unit_id) else {
                continue;
            };
            if let Some(scope) = scope_filter {
                if unit.scope != scope && unit.scope != "default" {
                    continue;
                }
            }
            if use_governance && exclude_retired && unit.status.eq_ignore_ascii_case("retired") {
                continue;
            }
            if seen_hashes.contains(&unit.content_sha256) {
                continue;
            }
            if tokens_used + unit.token_count > token_budget {
                continue;
            }
            let count = page_counts.get(&unit.page_path).copied().unwrap_or(0);
            let redundancy_penalty = 0.72_f64.powi(count as i32);
            let bridge_bonus = if candidate.via.is_some() && count == 0 && !packed.is_empty() {
                1.15
            } else {
                1.0
            };
            let marginal = candidate.score * redundancy_penalty * bridge_bonus;
            if packed.len() >= 2 && marginal < min_marginal_cutoff {
                continue;
            }
            if marginal > best_marginal {
                best_marginal = marginal;
                best_idx = Some(idx);
            }
        }
        let Some(chosen_idx) = best_idx else {
            break;
        };
        let chosen = remaining.remove(chosen_idx);
        let Some(unit) = units_by_id.get(&chosen.unit_id) else {
            continue;
        };
        seen_hashes.insert(unit.content_sha256.clone());
        *page_counts.entry(unit.page_path.clone()).or_insert(0) += 1;
        tokens_used += unit.token_count;
        let is_stale = unit
            .stale_after
            .as_deref()
            .is_some_and(|date| okf::is_iso_date_expired(date, Some(&reference_date)));
        let sources: Vec<String> = serde_json::from_str(&unit.sources_json).unwrap_or_default();
        packed.push(HybridEvidenceItem {
            unit_id: unit.unit_id.clone(),
            slug: slug_for_path(&unit.page_path),
            citation: format!("{}#L{}-L{}", unit.page_path, unit.line_start, unit.line_end),
            page_path: unit.page_path.clone(),
            page_title: unit.page_title.clone(),
            section_heading: unit.section_heading.clone(),
            heading_path: unit.heading_path.clone(),
            text: unit.body.clone(),
            token_count: unit.token_count,
            score: (chosen.score * 100_000.0).round() / 100_000.0,
            retrieval_channel: chosen.channel,
            via_link_from: chosen.via,
            doc_type: unit.doc_type.clone(),
            status: unit.status.clone(),
            trust_tier: unit.trust_tier.clone(),
            verified: unit.verified,
            stale_after: unit.stale_after.clone(),
            is_stale,
            redirect_to: unit.redirect_to.clone(),
            redirected_from: chosen.redirected_from,
            sources,
        });
    }

    Ok(HybridSearchResult {
        query: query.to_string(),
        token_budget,
        tokens_used,
        status: if packed.is_empty() {
            "empty".to_string()
        } else {
            "ok".to_string()
        },
        latency_ms: round_ms(started.elapsed().as_secs_f64() * 1000.0),
        embedding_model: model_name,
        filtered_retired_count,
        followed_redirects,
        explored_edges,
        evidence: packed,
    })
}

fn round_ms(ms: f64) -> f64 {
    (ms * 1000.0).round() / 1000.0
}

fn heading_matches_anchor(heading: &str, anchor: &str) -> bool {
    let normalize = |value: &str| -> String {
        value
            .chars()
            .filter_map(|c| {
                if c.is_alphanumeric() {
                    Some(c.to_ascii_lowercase())
                } else {
                    None
                }
            })
            .collect()
    };
    let h = normalize(heading);
    let a = normalize(anchor);
    !a.is_empty() && (h == a || h.contains(&a) || a.contains(&h))
}

#[derive(Debug, Clone)]
struct StoredUnit {
    unit_id: String,
    page_path: String,
    page_title: String,
    section_heading: String,
    heading_path: String,
    line_start: usize,
    line_end: usize,
    body: String,
    token_count: usize,
    content_sha256: String,
    sources_json: String,
    doc_type: String,
    status: String,
    trust_tier: String,
    verified: bool,
    stale_after: Option<String>,
    redirect_to: Option<String>,
    scope: String,
}

fn load_units_for_page(
    connection: &Connection,
    space_id: &str,
    page_path: &str,
) -> Result<Vec<StoredUnit>, String> {
    let mut stmt = connection
        .prepare(
            "SELECT unit_id, page_path, page_title, section_heading, heading_path,
                    line_start, line_end, body, token_count, content_sha256,
                    sources_json, doc_type, status, trust_tier, verified,
                    stale_after, redirect_to, scope
             FROM knowledge_units
             WHERE space_id = ?1 AND page_path = ?2
             ORDER BY line_start ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![space_id, page_path], map_stored_unit)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn load_units_by_ids(
    connection: &Connection,
    space_id: &str,
    unit_ids: &[String],
) -> Result<HashMap<String, StoredUnit>, String> {
    let mut out = HashMap::new();
    if unit_ids.is_empty() {
        return Ok(out);
    }
    let mut stmt = connection
        .prepare(
            "SELECT unit_id, page_path, page_title, section_heading, heading_path,
                    line_start, line_end, body, token_count, content_sha256,
                    sources_json, doc_type, status, trust_tier, verified,
                    stale_after, redirect_to, scope
             FROM knowledge_units
             WHERE space_id = ?1 AND unit_id = ?2",
        )
        .map_err(|e| e.to_string())?;
    for uid in unit_ids {
        if let Ok(unit) = stmt.query_row(params![space_id, uid], map_stored_unit) {
            out.insert(unit.unit_id.clone(), unit);
        }
    }
    Ok(out)
}

fn map_stored_unit(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredUnit> {
    Ok(StoredUnit {
        unit_id: row.get(0)?,
        page_path: row.get(1)?,
        page_title: row.get(2)?,
        section_heading: row.get(3)?,
        heading_path: row.get(4)?,
        line_start: row.get::<_, i64>(5)?.max(1) as usize,
        line_end: row.get::<_, i64>(6)?.max(1) as usize,
        body: row.get(7)?,
        token_count: row.get::<_, i64>(8)?.max(1) as usize,
        content_sha256: row.get(9)?,
        sources_json: row.get(10)?,
        doc_type: row.get(11)?,
        status: row.get(12)?,
        trust_tier: row.get(13)?,
        verified: row.get::<_, i64>(14)? != 0,
        stale_after: row.get(15)?,
        redirect_to: row.get(16)?,
        scope: row.get(17)?,
    })
}

fn load_unit_pages(
    connection: &Connection,
    space_id: &str,
    unit_ids: &HashSet<String>,
) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    let mut stmt = connection
        .prepare("SELECT page_path FROM knowledge_units WHERE space_id = ?1 AND unit_id = ?2")
        .map_err(|e| e.to_string())?;
    for uid in unit_ids {
        if let Ok(page_path) = stmt.query_row(params![space_id, uid], |row| row.get::<_, String>(0))
        {
            out.insert(uid.clone(), page_path);
        }
    }
    Ok(out)
}

fn load_heading_stats(
    connection: &Connection,
    space_id: &str,
) -> Result<(HashMap<String, usize>, usize), String> {
    let mut heading_df: HashMap<String, usize> = HashMap::new();
    let mut pages: HashSet<String> = HashSet::new();
    let mut stmt = connection
        .prepare("SELECT page_path, section_heading FROM knowledge_units WHERE space_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([space_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (page, heading) = row.map_err(|e| e.to_string())?;
        pages.insert(page);
        *heading_df.entry(heading.to_lowercase()).or_insert(0) += 1;
    }
    Ok((heading_df, pages.len().max(1)))
}

struct ResolvedLinkGraph {
    outgoing: HashMap<String, Vec<String>>,
    backlinks: HashMap<String, Vec<String>>,
    unit_outgoing: HashMap<String, Vec<(String, Option<String>)>>,
    page_anchors: HashMap<(String, String), String>,
}

fn build_resolved_link_graph(
    connection: &Connection,
    space_id: &str,
    root: &Path,
) -> Result<ResolvedLinkGraph, String> {
    let files = FileIndex::scan(root)?;
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    let mut backlinks: HashMap<String, Vec<String>> = HashMap::new();
    let mut unit_outgoing: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
    let mut page_anchors: HashMap<(String, String), String> = HashMap::new();

    let mut stmt = connection
        .prepare("SELECT source_path, target, kind FROM page_links WHERE space_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([space_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    for row in rows {
        let (source, target, kind) = row.map_err(|e| e.to_string())?;
        let resolved = if kind == LinkKind::Wiki as i64 {
            files.resolve_wikilink(&target, &source)
        } else if files.contains(&target) {
            Some(target)
        } else {
            None
        };
        if let Some(resolved_target) = resolved {
            if resolved_target != source {
                let out_list = outgoing.entry(source.clone()).or_default();
                if !out_list.contains(&resolved_target) {
                    out_list.push(resolved_target.clone());
                }
                let back_list = backlinks.entry(resolved_target).or_default();
                if !back_list.contains(&source) {
                    back_list.push(source);
                }
            }
        }
    }

    let mut unit_stmt = connection
        .prepare(
            "SELECT source_unit_id, source_path, target, target_anchor, kind
             FROM unit_links WHERE space_id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let unit_rows = unit_stmt
        .query_map([space_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    for row in unit_rows {
        let (unit_id, source, target, anchor, kind) = row.map_err(|e| e.to_string())?;
        let resolved = if kind == LinkKind::Wiki as i64 {
            files.resolve_wikilink(&target, &source)
        } else if files.contains(&target) {
            Some(target)
        } else {
            None
        };
        if let Some(resolved_target) = resolved {
            if resolved_target != source {
                if let Some(ref anchor_text) = anchor {
                    page_anchors
                        .entry((source.clone(), resolved_target.clone()))
                        .or_insert_with(|| anchor_text.clone());
                }
                let entry = unit_outgoing.entry(unit_id).or_default();
                if !entry
                    .iter()
                    .any(|(existing, _)| existing == &resolved_target)
                {
                    entry.push((resolved_target, anchor));
                }
            }
        }
    }

    Ok(ResolvedLinkGraph {
        outgoing,
        backlinks,
        unit_outgoing,
        page_anchors,
    })
}

struct ChunkedSection {
    unit_id: String,
    section_heading: String,
    heading_path: String,
    line_start: usize,
    line_end: usize,
    body: String,
    token_count: usize,
    content_sha256: String,
}

fn chunk_markdown_page(
    relative: &str,
    page_title: &str,
    description: Option<&str>,
    content: &str,
) -> Vec<ChunkedSection> {
    let (body_start_line, body) = match okf::split_frontmatter(content) {
        Ok((Some(_), body)) => {
            let consumed = content.len().saturating_sub(body.len());
            let prefix_lines = content[..consumed].bytes().filter(|&b| b == b'\n').count();
            (prefix_lines + 1, body)
        }
        _ => (1usize, content),
    };

    let lines: Vec<&str> = body.lines().collect();
    if lines.is_empty() || lines.iter().all(|line| line.trim().is_empty()) {
        let fallback = description
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(page_title)
            .to_string();
        if fallback.trim().is_empty() {
            return Vec::new();
        }
        let end_line = content.lines().count().max(1);
        let tokens = estimate_tokens(&fallback);
        let digest = format!("{:x}", Sha256::digest(fallback.as_bytes()));
        return vec![ChunkedSection {
            unit_id: format!("{relative}::s0::L1-L{end_line}"),
            section_heading: page_title.to_string(),
            heading_path: page_title.to_string(),
            line_start: 1,
            line_end: end_line,
            body: fallback,
            token_count: tokens,
            content_sha256: digest,
        }];
    }

    let mut raw_sections: Vec<(String, String, usize, usize, Vec<&str>)> = Vec::new();
    let mut heading_stack: Vec<(usize, String)> = Vec::new();
    let mut current_heading = page_title.to_string();
    let mut current_path = page_title.to_string();
    let mut current_start = body_start_line;
    let mut current_lines: Vec<&str> = Vec::new();
    let mut in_fence = false;
    let mut fence_marker: char = '`';

    for (idx, line) in lines.iter().enumerate() {
        let line_no = body_start_line + idx;
        let trimmed = line.trim_start();
        if let Some(ch) = fence_start_char(trimmed) {
            if !in_fence {
                in_fence = true;
                fence_marker = ch;
            } else if ch == fence_marker {
                in_fence = false;
            }
            current_lines.push(line);
            continue;
        }
        if !in_fence {
            if let Some((level, heading_text)) = parse_atx_heading(trimmed) {
                if !current_lines.is_empty() && current_lines.iter().any(|l| !l.trim().is_empty()) {
                    raw_sections.push((
                        current_heading.clone(),
                        current_path.clone(),
                        current_start,
                        line_no.saturating_sub(1).max(current_start),
                        std::mem::take(&mut current_lines),
                    ));
                } else {
                    current_lines.clear();
                }
                while heading_stack.last().is_some_and(|(lvl, _)| *lvl >= level) {
                    heading_stack.pop();
                }
                heading_stack.push((level, heading_text.clone()));
                current_heading = heading_text;
                current_path = heading_stack
                    .iter()
                    .map(|(_, h)| h.as_str())
                    .collect::<Vec<_>>()
                    .join(" > ");
                current_start = line_no;
                current_lines.push(line);
                continue;
            }
        }
        current_lines.push(line);
    }

    if !current_lines.is_empty() && current_lines.iter().any(|l| !l.trim().is_empty()) {
        let end_line = body_start_line + lines.len().saturating_sub(1);
        raw_sections.push((
            current_heading,
            current_path,
            current_start,
            end_line.max(current_start),
            current_lines,
        ));
    }

    // Sub-split overly long sections (> 360 tokens) by paragraphs so strict
    // token budgets always pack cleanly without dropping entire long pages.
    let mut chunks = Vec::new();
    let mut seq = 0usize;
    for (heading, heading_path, sec_start, sec_end, sec_lines) in raw_sections {
        let full_text = sec_lines.join("\n").trim().to_string();
        if full_text.is_empty() {
            continue;
        }
        let full_tokens = estimate_tokens(&full_text);
        if full_tokens <= 360 {
            let digest = format!("{:x}", Sha256::digest(full_text.as_bytes()));
            chunks.push(ChunkedSection {
                unit_id: format!("{relative}::s{seq}::L{sec_start}-L{sec_end}"),
                section_heading: heading,
                heading_path,
                line_start: sec_start,
                line_end: sec_end,
                body: full_text,
                token_count: full_tokens,
                content_sha256: digest,
            });
            seq += 1;
            continue;
        }

        let mut buf: Vec<&str> = Vec::new();
        let mut buf_start = sec_start;
        let mut buf_tokens = 0usize;
        for (offset, line) in sec_lines.iter().enumerate() {
            let line_no = sec_start + offset;
            let line_tokens = estimate_tokens(line);
            if !buf.is_empty() && buf_tokens + line_tokens > 260 && line.trim().is_empty() {
                let text = buf.join("\n").trim().to_string();
                if !text.is_empty() {
                    let end = line_no.saturating_sub(1).max(buf_start);
                    let tokens = estimate_tokens(&text);
                    let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
                    chunks.push(ChunkedSection {
                        unit_id: format!("{relative}::s{seq}::L{buf_start}-L{end}"),
                        section_heading: heading.clone(),
                        heading_path: heading_path.clone(),
                        line_start: buf_start,
                        line_end: end,
                        body: text,
                        token_count: tokens,
                        content_sha256: digest,
                    });
                    seq += 1;
                }
                buf.clear();
                buf_tokens = 0;
                buf_start = line_no + 1;
                continue;
            }
            if buf.is_empty() {
                buf_start = line_no;
            }
            buf.push(line);
            buf_tokens += line_tokens;
        }
        if !buf.is_empty() {
            let text = buf.join("\n").trim().to_string();
            if !text.is_empty() {
                let tokens = estimate_tokens(&text);
                let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
                chunks.push(ChunkedSection {
                    unit_id: format!("{relative}::s{seq}::L{buf_start}-L{sec_end}"),
                    section_heading: heading,
                    heading_path,
                    line_start: buf_start,
                    line_end: sec_end.max(buf_start),
                    body: text,
                    token_count: tokens,
                    content_sha256: digest,
                });
                seq += 1;
            }
        }
    }
    chunks
}

fn fence_start_char(trimmed_line: &str) -> Option<char> {
    if trimmed_line.starts_with("```") {
        Some('`')
    } else if trimmed_line.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

fn parse_atx_heading(trimmed_line: &str) -> Option<(usize, String)> {
    let hashes = trimmed_line.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = trimmed_line[hashes..].strip_prefix(' ')?;
    let heading = rest.trim().trim_end_matches('#').trim();
    if heading.is_empty() {
        None
    } else {
        Some((hashes, heading.to_string()))
    }
}

fn estimate_tokens(text: &str) -> usize {
    let mut ascii_words = 0usize;
    let mut cjk_chars = 0usize;
    let mut in_word = false;
    for ch in text.chars() {
        if ('\u{4E00}'..='\u{9FFF}').contains(&ch)
            || ('\u{3400}'..='\u{4DBF}').contains(&ch)
            || ('\u{3040}'..='\u{30FF}').contains(&ch)
            || ('\u{AC00}'..='\u{D7AF}').contains(&ch)
        {
            cjk_chars += 1;
            in_word = false;
        } else if ch.is_alphanumeric() {
            if !in_word {
                ascii_words += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    ((ascii_words as f64 * 1.25).ceil() as usize + cjk_chars).max(1)
}

fn is_reserved_nav_doc(path: &str) -> bool {
    path == "index.md"
        || path == "log.md"
        || path.ends_with("/index.md")
        || path.ends_with("/log.md")
}

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "by", "can", "did", "do", "does", "for",
    "from", "had", "has", "have", "how", "in", "into", "is", "it", "its", "of", "on", "or", "that",
    "the", "their", "them", "there", "these", "they", "this", "to", "was", "were", "what", "when",
    "where", "which", "who", "why", "with", "would",
];

fn extract_query_terms(query: &str) -> Vec<String> {
    let mut all_tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            all_tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        all_tokens.push(current);
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut filtered: Vec<String> = Vec::new();
    for token in &all_tokens {
        let trimmed = token.trim_matches('-');
        if trimmed.chars().count() < 2 || STOPWORDS.contains(&trimmed) {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            filtered.push(trimmed.to_string());
        }
    }
    if filtered.is_empty() {
        for token in all_tokens {
            let trimmed = token.trim_matches('-').to_string();
            if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
                filtered.push(trimmed);
            }
        }
    }
    filtered.truncate(20);
    filtered
}

fn fts_or_query(query: &str) -> String {
    extract_query_terms(query)
        .into_iter()
        .map(|term| {
            let escaped = term.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn query_term_coverage(terms: &[String], haystack_lower: &str) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }
    let matched = terms
        .iter()
        .filter(|term| haystack_lower.contains(term.as_str()))
        .count();
    matched as f64 / terms.len() as f64
}

pub fn backlinks(
    connection: &Connection,
    space_id: &str,
    root: &Path,
    target_path: &str,
) -> Result<Vec<SearchHit>, String> {
    let Some(target) = resolve_wikilink_target(target_path) else {
        return Ok(Vec::new());
    };
    let name = target.rsplit('/').next().unwrap_or(&target).to_lowercase();
    // Markdown links must match exactly; wikilinks are narrowed by file name
    // here and resolved below the way the reader resolves them.
    let mut statement = connection
        .prepare(
            "SELECT page.path, page.title, page.body, link.target, link.kind
             FROM page_links AS link
             JOIN indexed_pages AS page
               ON page.space_id = link.space_id AND page.path = link.source_path
             WHERE link.space_id = ?1
               AND ((link.kind = 0 AND link.target = ?2)
                 OR (link.kind = 1 AND (lower(link.target) = ?3
                                        OR lower(link.target) LIKE '%/' || ?3
                                        OR link.target = ?2)))
             ORDER BY page.title COLLATE NOCASE, page.path",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params![space_id, target, name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let files = if rows.iter().any(|row| row.4 == LinkKind::Wiki as i64) {
        Some(FileIndex::scan(root)?)
    } else {
        None
    };
    let mut results: Vec<SearchHit> = Vec::new();
    for (path, title, body, link_target, kind) in rows {
        let points_here = kind == LinkKind::Markdown as i64
            || files
                .as_ref()
                .and_then(|files| files.resolve_wikilink(&link_target, &path))
                .is_some_and(|resolved| resolved == target);
        if points_here && !results.iter().any(|hit| hit.path == path) {
            results.push(SearchHit {
                slug: slug_for_path(&path),
                snippet: first_content_line(&body),
                path,
                title,
                title_match: false,
            });
        }
    }
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
    let files = FileIndex::scan(root)?;
    let mut statement = connection
        .prepare(
            "SELECT link.source_path, page.title, link.target, link.kind
             FROM page_links AS link
             JOIN indexed_pages AS page
               ON page.space_id = link.space_id AND page.path = link.source_path
             WHERE link.space_id = ?1
             ORDER BY link.source_path, link.target",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([space_id], |row| {
            Ok((
                BrokenLink {
                    source_path: row.get(0)?,
                    source_title: row.get(1)?,
                    target: row.get(2)?,
                },
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut results: Vec<BrokenLink> = Vec::new();
    for (link, kind) in rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
    {
        let broken = is_diagnostic_source(&link.source_path)
            && is_diagnostic_target(&link.target)
            && !live_targets.contains(&link.target)
            && (kind != LinkKind::Wiki as i64
                || files
                    .resolve_wikilink(&link.target, &link.source_path)
                    .is_none());
        if broken && !results.contains(&link) {
            results.push(link);
        }
    }
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

/// How a stored link target is resolved: Markdown links name an exact,
/// case-sensitive path; wikilinks resolve like Obsidian (by name anywhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Markdown = 0,
    Wiki = 1,
}

pub fn extract_links(body: &str, source_path: &str) -> Vec<(String, LinkKind)> {
    extract_links_with_anchors(body, source_path)
        .into_iter()
        .map(|(target, _, kind)| (target, kind))
        .fold(Vec::new(), |mut acc, item| {
            if !acc.contains(&item) {
                acc.push(item);
            }
            acc
        })
}

pub fn extract_links_with_anchors(
    body: &str,
    source_path: &str,
) -> Vec<(String, Option<String>, LinkKind)> {
    let mut links: Vec<(String, Option<String>, LinkKind)> = Vec::new();
    let excluded = code_ranges(body);
    let wikilinks = extract_wikilinks_with_anchors(body, &excluded);
    for (_, target, anchor) in &wikilinks {
        if !links
            .iter()
            .any(|(existing, _, kind)| existing == target && *kind == LinkKind::Wiki)
        {
            links.push((target.clone(), anchor.clone(), LinkKind::Wiki));
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
                        .any(|(wikilink, _, _)| ranges_overlap(wikilink, &range)) =>
            {
                if let Some(target) = resolve_markdown_target(source_path, dest_url.as_ref()) {
                    let anchor = extract_link_anchor(dest_url.as_ref());
                    if !links.iter().any(|(existing, _, kind)| {
                        existing == &target && *kind == LinkKind::Markdown
                    }) {
                        links.push((target, anchor, LinkKind::Markdown));
                    }
                }
            }
            _ => {}
        }
    }
    links
}

pub fn code_ranges(body: &str) -> Vec<Range<usize>> {
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

fn extract_link_anchor(raw_target: &str) -> Option<String> {
    let (_, after_hash) = raw_target.split_once('#')?;
    let clean = after_hash.split('?').next().unwrap_or(after_hash).trim();
    if clean.is_empty() {
        return None;
    }
    let decoded = percent_decode_str(clean)
        .decode_utf8()
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| clean.to_string());
    let trimmed = decoded.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_wikilinks_with_anchors(
    text: &str,
    excluded: &[Range<usize>],
) -> Vec<(Range<usize>, String, Option<String>)> {
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
            // Inside tables Obsidian escapes the alias pipe: [[Note\\|alias]].
            let raw_target = raw
                .split('|')
                .next()
                .unwrap_or_default()
                .trim_end_matches('\\');
            if let Some(target) = resolve_wikilink_target(raw_target) {
                let anchor = extract_link_anchor(raw_target);
                links.push((span.clone(), target, anchor));
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

pub fn resolve_markdown_target(source_path: &str, target: &str) -> Option<String> {
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

pub fn resolve_wikilink_target(target: &str) -> Option<String> {
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

/// Visible files of a Space, for resolving wikilinks the way Obsidian and the
/// CoWiki reader do: a path resolves exactly, relative to the linking page,
/// or by path suffix; a bare name resolves to a file with that name anywhere,
/// preferring the linking page's folder, then the shortest path.
pub struct FileIndex {
    files: Vec<String>,
    by_key: HashMap<String, usize>,
    by_name: HashMap<String, Vec<usize>>,
}

fn folder_of(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(folder, _)| folder)
        .unwrap_or("")
}

impl FileIndex {
    pub fn new(files: Vec<String>) -> Self {
        let mut by_key = HashMap::new();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, path) in files.iter().enumerate() {
            let key = path.to_lowercase();
            let name = key.rsplit('/').next().unwrap_or(&key).to_string();
            by_key.entry(key).or_insert(index);
            by_name.entry(name).or_default().push(index);
        }
        Self {
            files,
            by_key,
            by_name,
        }
    }

    pub fn scan(root: &Path) -> Result<Self, String> {
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| should_walk(entry.path(), root))
        {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.file_type().is_file() {
                files.push(relative_path(root, entry.path())?);
            }
        }
        files.sort();
        Ok(Self::new(files))
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }

    pub fn contains(&self, path: &str) -> bool {
        self.by_key
            .get(&path.to_lowercase())
            .is_some_and(|index| self.files[*index] == path)
    }

    fn closest(&self, candidates: impl Iterator<Item = usize>, source: &str) -> Option<String> {
        let folder = folder_of(source).to_lowercase();
        candidates
            .map(|index| &self.files[index])
            .min_by(|left, right| {
                let same = |path: &str| folder_of(path).to_lowercase() != folder;
                same(left)
                    .cmp(&same(right))
                    .then(left.matches('/').count().cmp(&right.matches('/').count()))
                    .then(left.cmp(right))
            })
            .cloned()
    }

    /// Resolve a stored wikilink target (normalized by `resolve_wikilink_target`,
    /// so `.md` is appended to names without it) from `source`.
    pub fn resolve_wikilink(&self, stored: &str, source: &str) -> Option<String> {
        let mut forms = vec![stored.to_string()];
        // `![[photo.png]]` is stored as `photo.png.md`.
        if let Some(stem) = stored.strip_suffix(".md") {
            if stem
                .rsplit('/')
                .next()
                .is_some_and(|name| name.contains('.'))
            {
                forms.push(stem.to_string());
            }
        }
        for form in forms {
            let key = form.to_lowercase();
            if key.contains('/') {
                if let Some(index) = self.by_key.get(&key) {
                    return Some(self.files[*index].clone());
                }
                let folder = folder_of(source);
                if !folder.is_empty() {
                    let relative = format!("{}/{key}", folder.to_lowercase());
                    if let Some(index) = self.by_key.get(&relative) {
                        return Some(self.files[*index].clone());
                    }
                }
                let suffix = format!("/{key}");
                let found = self.closest(
                    self.files
                        .iter()
                        .enumerate()
                        .filter(|(_, path)| path.to_lowercase().ends_with(&suffix))
                        .map(|(index, _)| index),
                    source,
                );
                if found.is_some() {
                    return found;
                }
            } else if let Some(indices) = self.by_name.get(&key) {
                return self.closest(indices.iter().copied(), source);
            }
        }
        None
    }
}

fn first_content_line(body: &str) -> String {
    let content = okf::split_frontmatter(body)
        .map(|(_, stripped)| stripped)
        .unwrap_or(body);
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "---" && !line.starts_with("title:"))
        .unwrap_or_default()
        .trim_start_matches('#')
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_memory_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize(&mut conn).unwrap();
        conn
    }

    #[test]
    fn search_excludes_retired_follows_redirects_and_downranks_deprecated_or_stale() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(
            root.join("retired.md"),
            "---\ntype: Note\ntitle: Legacy Quorum\nstatus: retired\n---\n\n# Legacy Quorum\n\nQuorum consensus retired spec.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("deprecated.md"),
            "---\ntype: Note\ntitle: Old Quorum\nstatus: deprecated\nstale_after: \"2020-01-01\"\n---\n\n# Old Quorum\n\nQuorum consensus old design.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("active.md"),
            "---\ntype: Note\ntitle: Active Quorum\nstatus: active\ntrust_tier: human_verified\nverified: true\n---\n\n# Active Quorum\n\nQuorum consensus verified design.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("alias-old.md"),
            "---\ntype: Note\ntitle: Zephyr Protocol\nredirect_to: canonical-zephyr.md\n---\n\n# Zephyr Protocol\n\nMoved to canonical page.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("canonical-zephyr.md"),
            "---\ntype: Note\ntitle: Current Transport\nstatus: active\n---\n\n# Current Transport\n\nCanonical transport architecture.\n",
        )
        .unwrap();

        let mut db = open_memory_db();
        rebuild_space(&mut db, "space-1", root).unwrap();

        let hits = search(&db, "space-1", "quorum", 10).unwrap();
        let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        assert!(
            !paths.contains(&"retired.md"),
            "retired page must be excluded"
        );
        assert_eq!(
            paths[0], "active.md",
            "verified active page should rank before deprecated/stale"
        );
        assert_eq!(paths[1], "deprecated.md");

        let redir_hits = search(&db, "space-1", "zephyr", 10).unwrap();
        assert_eq!(redir_hits.len(), 1);
        assert_eq!(redir_hits[0].path, "canonical-zephyr.md");
        assert_eq!(redir_hits[0].title, "Current Transport");
    }

    #[test]
    fn hybrid_search_expands_two_hop_graph_under_token_budget_with_citations() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(
            root.join("notes/gateway.md"),
            "---\ntype: Architecture\ntitle: Auth Gateway\nstatus: active\nverified: true\n---\n\n# Auth Gateway\n\n## Overview\n\nThe Auth Gateway issues short-lived session tokens and delegates signing to [[Token Vault]].\n",
        )
        .unwrap();
        std::fs::write(
            root.join("notes/Token Vault.md"),
            "---\ntype: Architecture\ntitle: Token Vault\nstatus: active\n---\n\n# Token Vault\n\n## Key Custody\n\nToken Vault stores signing keys inside [Hardware KMS](kms-policy.md).\n",
        )
        .unwrap();
        std::fs::write(
            root.join("notes/kms-policy.md"),
            "---\ntype: Policy\ntitle: Hardware KMS\nstatus: active\nverified: true\n---\n\n# Hardware KMS\n\n## Rotation Cadence\n\nMaster HSM keys rotate every 90 days using dual-custody ceremonies.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("notes/retired-kms.md"),
            "---\ntype: Policy\ntitle: Retired KMS\nstatus: retired\n---\n\n# Retired KMS\n\nOld Auth Gateway policy that must never surface.\n",
        )
        .unwrap();

        let mut db = open_memory_db();
        rebuild_space(&mut db, "space-1", root).unwrap();

        let result = hybrid_search(
            &db,
            "space-1",
            root,
            "How does Auth Gateway rotate signing keys?",
            &HybridSearchOptions {
                token_budget: Some(400),
                max_hops: Some(2),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.status, "ok");
        assert!(result.tokens_used <= 400);
        assert!(result.filtered_retired_count >= 1);
        let paths: Vec<&str> = result
            .evidence
            .iter()
            .map(|item| item.page_path.as_str())
            .collect();
        assert!(
            paths.contains(&"notes/gateway.md"),
            "missing seed page: {paths:?}"
        );
        assert!(
            paths.contains(&"notes/Token Vault.md"),
            "missing 1-hop wikilink page: {paths:?}"
        );
        assert!(
            paths.contains(&"notes/kms-policy.md"),
            "missing 2-hop markdown link page: {paths:?}"
        );
        assert!(!paths.contains(&"notes/retired-kms.md"));
        assert!(result
            .evidence
            .iter()
            .all(|item| item.citation.contains("#L")));
    }

    #[test]
    fn schema_upgrade_from_v2_resets_cleanly_to_v3() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE knowledge_index_metadata (singleton INTEGER PRIMARY KEY, schema_version INTEGER NOT NULL);
             INSERT INTO knowledge_index_metadata (singleton, schema_version) VALUES (1, 2);
             CREATE TABLE indexed_pages (id INTEGER PRIMARY KEY, space_id TEXT, path TEXT, title TEXT, body TEXT, modified_ns INTEGER);",
        )
        .unwrap();
        initialize(&mut conn).unwrap();
        let ver: i64 = conn
            .query_row(
                "SELECT schema_version FROM knowledge_index_metadata WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ver, SCHEMA_VERSION);
        assert!(schema_is_structurally_healthy(&conn));
    }

    #[test]
    fn hybrid_search_follows_frontmatter_only_redirect_stub_and_respects_exclude_retired_flag() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(
            root.join("stub.md"),
            "---\ntype: Note\ntitle: Chronos Scheduler\nstatus: deprecated\nredirect_to: active-cron.md\n---\n",
        )
        .unwrap();
        std::fs::write(
            root.join("active-cron.md"),
            "---\ntype: Note\ntitle: Periodic Runner\nstatus: active\nverified: true\n---\n\n# Periodic Runner\n\nRuns scheduled maintenance tasks.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("old-retired.md"),
            "---\ntype: Note\ntitle: Retired Chronos\nstatus: retired\n---\n\n# Retired Chronos\n\nChronos historical note.\n",
        )
        .unwrap();

        let mut db = open_memory_db();
        rebuild_space(&mut db, "space-1", root).unwrap();

        let default_res = hybrid_search(
            &db,
            "space-1",
            root,
            "Chronos Scheduler",
            &HybridSearchOptions::default(),
        )
        .unwrap();
        assert_eq!(default_res.followed_redirects.len(), 1);
        assert_eq!(default_res.followed_redirects[0].from, "stub.md");
        assert_eq!(default_res.followed_redirects[0].to, "active-cron.md");
        assert!(default_res
            .evidence
            .iter()
            .any(|item| item.page_path == "active-cron.md"
                && item.redirected_from.as_deref() == Some("stub.md")));
        assert!(!default_res
            .evidence
            .iter()
            .any(|item| item.page_path == "old-retired.md"));

        let include_retired = hybrid_search(
            &db,
            "space-1",
            root,
            "Retired Chronos",
            &HybridSearchOptions {
                exclude_retired: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(include_retired
            .evidence
            .iter()
            .any(|item| item.page_path == "old-retired.md"));
    }

    #[cfg(unix)]
    #[test]
    fn index_paths_refuses_symlinks_escaping_space() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("space");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(
            outside.join("secret.md"),
            "# Secret\n\nClassified sentinel.",
        )
        .unwrap();
        symlink(outside.join("secret.md"), root.join("linked.md")).unwrap();

        let mut db = open_memory_db();
        index_paths(&mut db, "space-1", &root, &[PathBuf::from("linked.md")]).unwrap();
        assert!(search(&db, "space-1", "Classified", 10).unwrap().is_empty());
    }

    #[test]
    fn dense_vector_embedding_cache_and_anchor_unit_links_work_end_to_end() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(
            root.join("alpha.md"),
            "---\ntype: Guide\ntitle: Gateway Guide\nscope: prod\n---\n\n# Gateway Guide\n\n## Intro\n\nSee [Rotation Policy](beta.md#rotation-cadence) for key updates.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("beta.md"),
            "---\ntype: Policy\ntitle: Security Policy\nscope: prod\n---\n\n# Security Policy\n\n## General Notes\n\nMiscellaneous background text.\n\n## Rotation Cadence\n\nHardware signing keys rotate every 90 days.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("staging.md"),
            "---\ntype: Guide\ntitle: Staging Gateway\nscope: staging\n---\n\n# Staging Gateway\n\nStaging-only gateway testing notes.\n",
        )
        .unwrap();

        let mut db = open_memory_db();
        rebuild_space(&mut db, "space-1", root).unwrap();

        let stats = index_stats(&db, "space-1").unwrap();
        assert_eq!(stats.schema_version, SCHEMA_VERSION);
        assert_eq!(stats.indexed_pages, 3);
        assert!(stats.knowledge_units >= 4);
        assert!(stats.unit_links >= 1);
        assert!(stats.cached_embeddings >= 4);
        assert_eq!(stats.embedding_model, embeddings::LOCAL_EMBEDDING_MODEL);

        // Vector-only mode should still retrieve morphologically related content.
        let vec_only = hybrid_search(
            &db,
            "space-1",
            root,
            "rotating hardware keys cadence",
            &HybridSearchOptions {
                use_lexical: Some(false),
                use_vector: Some(true),
                use_graph_expansion: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(vec_only.status, "ok");
        assert!(vec_only
            .evidence
            .iter()
            .any(|item| item.page_path == "beta.md" && item.retrieval_channel == "seed_vector"));

        // Scope filter excludes staging-only documents while retaining prod docs.
        let prod_only = hybrid_search(
            &db,
            "space-1",
            root,
            "Gateway",
            &HybridSearchOptions {
                scope_filter: Some("prod".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(prod_only
            .evidence
            .iter()
            .all(|item| item.page_path != "staging.md"));
        // Anchor-targeted section on beta.md should surface via unit_links.
        assert!(prod_only
            .evidence
            .iter()
            .any(|item| item.page_path == "beta.md" && item.section_heading == "Rotation Cadence"));
    }
}
