//! WikiFsGateway — 5 wiki primitives, delegates to wiki_fs.
//!
//! Zero domain semantics. Cowiki knows nothing about wiki/entity/concept/source.
//! Domain knowledge lives in agent SKILL.md prompts.
//!
//! No constraints — agents have full tool access within their workspace.
//! Hard rules: sources/ is read-only (enforced at filesystem level).

use std::sync::Mutex;

use crate::git::WikiRepoManager;
use crate::wiki_fs;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub summary: Option<String>,
}

impl ToolResult {
    pub fn ok(output: serde_json::Value) -> Self {
        Self {
            success: true,
            output,
            summary: None,
        }
    }
    pub fn ok_with_summary(output: serde_json::Value, summary: &str) -> Self {
        Self {
            success: true,
            output,
            summary: Some(summary.to_string()),
        }
    }
    pub fn err(error: &str) -> Self {
        Self {
            success: false,
            output: serde_json::json!({"error": error}),
            summary: Some(error.to_string()),
        }
    }
}

// ── WikiFsGateway ──────────────────────────────────────────────

/// Generic file gateway — 5 primitives: list, read, write, remove, search.
///
/// Delegates all operations to `wiki_fs` for path validation and git access.
/// Zero domain semantics — agents compose primitives via SKILL.md prompts.
///
/// Constraint tracking: records written and removed page paths so callers
/// can discover agent output without scanning directories.
pub struct WikiFsGateway {
    repo_manager: WikiRepoManager,
    db: Option<sqlx::PgPool>,
    /// Pages written during the current agent request (reset per request).
    pub written_pages: Mutex<Vec<String>>,
    /// Pages removed during the current agent request (reset per request).
    pub removed_pages: Mutex<Vec<String>>,
}

impl WikiFsGateway {
    pub fn new(data_dir: &str) -> Self {
        Self {
            repo_manager: WikiRepoManager::new(data_dir),
            db: None,
            written_pages: Mutex::new(Vec::new()),
            removed_pages: Mutex::new(Vec::new()),
        }
    }

    pub fn with_db(mut self, db: sqlx::PgPool) -> Self {
        self.db = Some(db);
        self
    }

    /// Reset per-request tracking. Call before each agent dispatch.
    pub fn reset_tracking(&self) {
        if let Ok(mut w) = self.written_pages.lock() {
            w.clear();
        }
        if let Ok(mut r) = self.removed_pages.lock() {
            r.clear();
        }
    }

    /// Take the accumulated written/removed page paths.
    pub fn take_tracking(&self) -> (Vec<String>, Vec<String>) {
        let written = self
            .written_pages
            .lock()
            .map(|mut w| w.drain(..).collect())
            .unwrap_or_default();
        let removed = self
            .removed_pages
            .lock()
            .map(|mut r| r.drain(..).collect())
            .unwrap_or_default();
        (written, removed)
    }

    /// Persist current tracking state to a JSON file under the workspace repo.
    /// The cowiki server reads this file after agent dispatch to discover pages.
    pub fn persist_tracking(&self, workspace: &str) {
        let (written, removed) = {
            let w = self
                .written_pages
                .lock()
                .map(|w| w.clone())
                .unwrap_or_default();
            let r = self
                .removed_pages
                .lock()
                .map(|r| r.clone())
                .unwrap_or_default();
            (w, r)
        };
        if let Ok(repo) = self.repo_manager.get(workspace) {
            let dir = repo.path().join(".cowiki");
            let _ = std::fs::create_dir_all(&dir);
            let tracking = serde_json::json!({
                "written_pages": written,
                "removed_pages": removed,
            });
            let _ = std::fs::write(
                dir.join("tracking.json"),
                serde_json::to_string_pretty(&tracking).unwrap_or_default(),
            );
        }
    }

    /// Read persisted tracking from a workspace repo.
    /// Returns (written_pages, removed_pages).
    pub fn read_tracking_file(repo_root: &std::path::Path) -> (Vec<String>, Vec<String>) {
        let path = repo_root.join(".cowiki").join("tracking.json");
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(v) => {
                    let written = v
                        .get("written_pages")
                        .and_then(|w| w.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let removed = v
                        .get("removed_pages")
                        .and_then(|r| r.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    (written, removed)
                }
                Err(_) => (vec![], vec![]),
            },
            Err(_) => (vec![], vec![]),
        }
    }

    /// Execute a tool primitive.
    pub fn execute(
        &self,
        tool: &str,
        args: serde_json::Value,
        workspace: &str,
        branch: &str,
    ) -> ToolResult {
        // Validate workspace slug before any filesystem access
        if let Err(e) = validate_workspace_slug(workspace) {
            return ToolResult::err(&e);
        }

        let repo = match self
            .repo_manager
            .get(workspace)
            .map_err(|e| format!("repo error: {e}"))
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(&e),
        };

        let result = match tool {
            "list" => {
                let dir = args.get("dir").and_then(|v| v.as_str()).unwrap_or("");
                if dir.is_empty() {
                    return ToolResult::err("dir required");
                }
                let valid_dir = match wiki_fs::validate_dir(dir) {
                    Ok(d) => d,
                    Err(e) => return ToolResult::err(&e),
                };
                match wiki_fs::list_pages_recursive(&repo, branch, valid_dir) {
                    Ok(entries) => ToolResult::ok(serde_json::json!({"entries": entries})),
                    Err(e) => ToolResult::err(&e),
                }
            }

            "read" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                if path.is_empty() {
                    return ToolResult::err("path required");
                }
                let (dir, slug) = match parse_file_path(path) {
                    Ok(v) => v,
                    Err(e) => return ToolResult::err(&e),
                };
                match wiki_fs::read_page(&repo, branch, dir, slug) {
                    Ok(Some(content)) => ToolResult::ok(serde_json::json!({
                        "content": String::from_utf8_lossy(&content).to_string(),
                        "path": path,
                    })),
                    Ok(None) => ToolResult::err("file not found"),
                    Err(e) => ToolResult::err(&e),
                }
            }

            "write" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
                if path.is_empty() || body.is_empty() {
                    return ToolResult::err("path and body required");
                }
                let (dir, slug) = match parse_file_path(path) {
                    Ok(v) => v,
                    Err(e) => return ToolResult::err(&e),
                };
                // sources/ is read-only
                if dir == "sources" {
                    return ToolResult::err("sources/ is read-only, cannot write");
                }
                if let Err(e) = repo.ensure_user_branch(branch) {
                    return ToolResult::err(&e.to_string());
                }
                match wiki_fs::write_page(&repo, branch, dir, slug, body.as_bytes(), "agent") {
                    Ok(()) => {
                        // Track written page
                        if let Ok(mut w) = self.written_pages.lock() {
                            w.push(path.to_string());
                        }
                        self.persist_tracking(workspace);
                        ToolResult::ok(serde_json::json!({"path": path, "written": true}))
                    }
                    Err(e) => ToolResult::err(&e),
                }
            }

            "remove" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                if path.is_empty() {
                    return ToolResult::err("path required");
                }
                let (dir, slug) = match parse_file_path(path) {
                    Ok(v) => v,
                    Err(e) => return ToolResult::err(&e),
                };
                // sources/ is read-only — reject writes AND removes
                if dir == "sources" {
                    return ToolResult::err("sources/ is read-only, cannot remove");
                }
                if let Err(e) = repo.ensure_user_branch(branch) {
                    return ToolResult::err(&e.to_string());
                }
                match wiki_fs::remove_page(&repo, branch, dir, slug, "agent") {
                    Ok(()) => {
                        // Track removed page
                        if let Ok(mut r) = self.removed_pages.lock() {
                            r.push(path.to_string());
                        }
                        self.persist_tracking(workspace);
                        ToolResult::ok(serde_json::json!({"deleted": path}))
                    }
                    Err(e) => ToolResult::err(&e),
                }
            }

            "search" => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                if query.is_empty() {
                    return ToolResult::err("query required");
                }
                let db = match &self.db {
                    Some(db) => db.clone(),
                    None => return ToolResult::err("search not configured — DB pool missing"),
                };
                // Postgres FTS — search across pages table with workspace scope
                let sql = r#"
                    SELECT slug, ts_rank(tsv, query) AS rank
                    FROM pages, plainto_tsquery('english', $1) AS query
                    WHERE tsv @@ query
                      AND workspace_slug = $2
                    ORDER BY rank DESC
                    LIMIT 20
                "#;
                // Run DB query. Use block_in_place to safely block in tokio context
                // without the "Cannot start a runtime from within a runtime" panic.
                let query_owned = query.to_string();
                let ws_owned = workspace.to_string();
                let rows_result: Result<Vec<(String, f64)>, sqlx::Error> =
                    tokio::task::block_in_place(|| {
                        let handle = tokio::runtime::Handle::current();
                        handle.block_on(async {
                            sqlx::query_as::<_, (String, f64)>(sql)
                                .bind(&query_owned)
                                .bind(&ws_owned)
                                .fetch_all(&db)
                                .await
                        })
                    });
                let rows = match rows_result {
                    Ok(r) => r,
                    Err(e) => return ToolResult::err(&format!("search error: {e}")),
                };
                let results: Vec<serde_json::Value> = rows
                    .into_iter()
                    .map(|(slug, rank)| serde_json::json!({"slug": slug, "rank": rank}))
                    .collect();
                ToolResult::ok(serde_json::json!({"results": results}))
            }

            _ => ToolResult::err(&format!(
                "unknown primitive '{}'. valid: list, read, write, remove, search",
                tool
            )),
        };

        result
    }
}

/// Parse "wiki/foo/bar.md" → (dir="wiki", slug="foo/bar")
fn parse_file_path(path: &str) -> Result<(&str, &str), String> {
    let (dir, rest) = path
        .split_once('/')
        .ok_or_else(|| format!("path must be '<dir>/<slug>.md', got: {path}"))?;
    let slug = rest.strip_suffix(".md").unwrap_or(rest);
    if slug.is_empty() {
        return Err(format!("empty slug in path: {path}"));
    }
    Ok((dir, slug))
}

/// Validate workspace slug — reject empty, path traversal, or separator chars.
fn validate_workspace_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() || slug.contains("..") || slug.contains('/') || slug.contains('\\') {
        return Err(format!("invalid workspace slug: {slug}"));
    }
    Ok(())
}
