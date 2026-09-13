use super::LocalEngine;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_COMMENT_CHARS: usize = 10_000;
const MAX_SOURCE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PageComment {
    pub id: String,
    pub workspace_slug: String,
    pub page_slug: String,
    pub user_id: String,
    pub content_hash: Option<String>,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub body: String,
    pub parent_id: Option<String>,
    pub resolved: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommentSnapshot {
    pub content_hash: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PageCommentsResponse {
    pub comments: Vec<PageComment>,
    pub snapshots: Vec<CommentSnapshot>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommentMember {
    pub id: String,
    pub name: String,
}

pub(super) fn initialize(connection: &rusqlite::Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS local_comment_snapshots (
                space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                page_slug TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                source TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                PRIMARY KEY (space_id, page_slug, content_hash)
            );
            CREATE TABLE IF NOT EXISTS local_page_comments (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                page_slug TEXT NOT NULL,
                user_id TEXT NOT NULL,
                user_name TEXT NOT NULL,
                content_hash TEXT,
                start_line INTEGER,
                end_line INTEGER,
                body TEXT NOT NULL,
                parent_id TEXT REFERENCES local_page_comments(id) ON DELETE CASCADE,
                resolved INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                CHECK (length(body) BETWEEN 1 AND 10000),
                CHECK (
                    (parent_id IS NULL AND content_hash IS NOT NULL AND start_line IS NOT NULL AND end_line IS NOT NULL)
                    OR
                    (parent_id IS NOT NULL AND content_hash IS NULL AND start_line IS NULL AND end_line IS NULL)
                )
            );
            CREATE INDEX IF NOT EXISTS local_page_comments_page_idx
                ON local_page_comments(space_id, page_slug, created_at, id);",
        )
        .map_err(|error| error.to_string())
}

impl LocalEngine {
    pub fn list_page_comments(
        &self,
        space_slug: &str,
        page_slug: &str,
    ) -> Result<PageCommentsResponse, String> {
        validate_page_slug(page_slug)?;
        let db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        let space_id = space_id(&db, space_slug)?;
        let mut comments_query = db
            .prepare(
                "SELECT comments.id, spaces.slug, comments.page_slug, comments.user_id,
                        comments.content_hash, comments.start_line, comments.end_line,
                        comments.body, comments.parent_id, comments.resolved,
                        comments.created_at, comments.updated_at
                 FROM local_page_comments AS comments
                 JOIN spaces ON spaces.id = comments.space_id
                 WHERE comments.space_id = ?1 AND comments.page_slug = ?2
                 ORDER BY comments.created_at, comments.id",
            )
            .map_err(|error| error.to_string())?;
        let comments = comments_query
            .query_map(params![space_id, page_slug], |row| {
                Ok(PageComment {
                    id: row.get(0)?,
                    workspace_slug: row.get(1)?,
                    page_slug: row.get(2)?,
                    user_id: row.get(3)?,
                    content_hash: row.get(4)?,
                    start_line: row.get(5)?,
                    end_line: row.get(6)?,
                    body: row.get(7)?,
                    parent_id: row.get(8)?,
                    resolved: row.get::<_, i64>(9)? != 0,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let mut snapshots_query = db
            .prepare(
                "SELECT content_hash, source FROM local_comment_snapshots
                 WHERE space_id = ?1 AND page_slug = ?2 ORDER BY created_at",
            )
            .map_err(|error| error.to_string())?;
        let snapshots = snapshots_query
            .query_map(params![space_id, page_slug], |row| {
                Ok(CommentSnapshot {
                    content_hash: row.get(0)?,
                    source: row.get(1)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(PageCommentsResponse {
            comments,
            snapshots,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_page_comment(
        &self,
        space_slug: &str,
        page_slug: &str,
        user_id: &str,
        user_name: &str,
        body: &str,
        source: Option<&str>,
        start_line: Option<i32>,
        end_line: Option<i32>,
        parent_id: Option<&str>,
    ) -> Result<PageComment, String> {
        validate_page_slug(page_slug)?;
        let body = validate_body(body)?;
        let user_name = if user_name.trim().is_empty() {
            "You"
        } else {
            user_name.trim()
        };
        let mut db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        let space_id = space_id(&db, space_slug)?;
        let transaction = db.transaction().map_err(|error| error.to_string())?;
        let (content_hash, start_line, end_line) = if let Some(parent_id) = parent_id {
            if source.is_some() || start_line.is_some() || end_line.is_some() {
                return Err("replies cannot include a source anchor".into());
            }
            let exists = transaction
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM local_page_comments
                        WHERE id = ?1 AND space_id = ?2 AND page_slug = ?3
                    )",
                    params![parent_id, space_id, page_slug],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|error| error.to_string())?;
            if !exists {
                return Err("reply parent does not belong to this page".into());
            }
            (None, None, None)
        } else {
            let source = source.ok_or_else(|| "a source snapshot is required".to_string())?;
            let (start_line, end_line) = validate_anchor(source, start_line, end_line)?;
            let hash = format!("{:x}", Sha256::digest(source.as_bytes()));
            transaction
                .execute(
                    "INSERT OR IGNORE INTO local_comment_snapshots
                        (space_id, page_slug, content_hash, source) VALUES (?1, ?2, ?3, ?4)",
                    params![space_id, page_slug, hash, source],
                )
                .map_err(|error| error.to_string())?;
            (Some(hash), Some(start_line), Some(end_line))
        };
        let id = Uuid::new_v4().to_string();
        transaction
            .execute(
                "INSERT INTO local_page_comments
                    (id, space_id, page_slug, user_id, user_name, content_hash,
                     start_line, end_line, body, parent_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    space_id,
                    page_slug,
                    user_id,
                    user_name,
                    content_hash,
                    start_line,
                    end_line,
                    body,
                    parent_id
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        drop(db);
        self.get_page_comment(space_slug, &id)
    }

    pub fn list_comment_members(&self, space_slug: &str) -> Result<Vec<CommentMember>, String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        let space_id = space_id(&db, space_slug)?;
        let mut query = db
            .prepare(
                "SELECT user_id, max(user_name) FROM local_page_comments
                 WHERE space_id = ?1 GROUP BY user_id ORDER BY lower(max(user_name)), user_id",
            )
            .map_err(|error| error.to_string())?;
        let members = query
            .query_map([space_id], |row| {
                Ok(CommentMember {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(members)
    }

    pub fn set_page_comment_resolved(
        &self,
        space_slug: &str,
        comment_id: &str,
        resolved: bool,
    ) -> Result<PageComment, String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        let space_id = space_id(&db, space_slug)?;
        let changed = db
            .execute(
                "UPDATE local_page_comments
                 SET resolved = ?3, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND space_id = ?2",
                params![comment_id, space_id, resolved],
            )
            .map_err(|error| error.to_string())?;
        drop(db);
        if changed == 0 {
            return Err("comment not found".into());
        }
        self.get_page_comment(space_slug, comment_id)
    }

    pub fn delete_page_comment(
        &self,
        space_slug: &str,
        comment_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        let space_id = space_id(&db, space_slug)?;
        let owner = db
            .query_row(
                "SELECT user_id FROM local_page_comments WHERE id = ?1 AND space_id = ?2",
                params![comment_id, space_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "comment not found".to_string())?;
        if owner != user_id {
            return Err("only the comment author can delete it".into());
        }
        db.execute(
            "DELETE FROM local_page_comments WHERE id = ?1 AND space_id = ?2",
            params![comment_id, space_id],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn get_page_comment(&self, space_slug: &str, comment_id: &str) -> Result<PageComment, String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "local database lock poisoned".to_string())?;
        let space_id = space_id(&db, space_slug)?;
        db.query_row(
            "SELECT comments.id, spaces.slug, comments.page_slug, comments.user_id,
                    comments.content_hash, comments.start_line, comments.end_line,
                    comments.body, comments.parent_id, comments.resolved,
                    comments.created_at, comments.updated_at
             FROM local_page_comments AS comments JOIN spaces ON spaces.id = comments.space_id
             WHERE comments.id = ?1 AND comments.space_id = ?2",
            params![comment_id, space_id],
            |row| {
                Ok(PageComment {
                    id: row.get(0)?,
                    workspace_slug: row.get(1)?,
                    page_slug: row.get(2)?,
                    user_id: row.get(3)?,
                    content_hash: row.get(4)?,
                    start_line: row.get(5)?,
                    end_line: row.get(6)?,
                    body: row.get(7)?,
                    parent_id: row.get(8)?,
                    resolved: row.get::<_, i64>(9)? != 0,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "comment not found".to_string())
    }
}

fn space_id(connection: &rusqlite::Connection, space_slug: &str) -> Result<String, String> {
    connection
        .query_row(
            "SELECT id FROM spaces WHERE slug = ?1",
            [space_slug],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Space not found".to_string())
}

fn validate_page_slug(page_slug: &str) -> Result<(), String> {
    let valid = !page_slug.is_empty()
        && page_slug.len() <= 1024
        && page_slug.ends_with(".md")
        && !page_slug.starts_with('/')
        && !page_slug.contains('\\')
        && page_slug
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".." && !part.starts_with('.'));
    if valid {
        Ok(())
    } else {
        Err("comment path must be a visible Markdown page".into())
    }
}

fn validate_body(body: &str) -> Result<&str, String> {
    let body = body.trim();
    if (1..=MAX_COMMENT_CHARS).contains(&body.chars().count()) {
        Ok(body)
    } else {
        Err("comment body must be between 1 and 10000 characters".into())
    }
}

fn validate_anchor(
    source: &str,
    start: Option<i32>,
    end: Option<i32>,
) -> Result<(i32, i32), String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("source snapshot is too large".into());
    }
    let start = start.ok_or_else(|| "startLine is required".to_string())?;
    let end = end.ok_or_else(|| "endLine is required".to_string())?;
    let line_count = source.lines().count().max(1) as i32;
    if start < 1 || end < start || end > line_count {
        return Err("comment anchor is outside the source snapshot".into());
    }
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::super::LocalEngine;

    #[test]
    fn local_comments_persist_and_support_threads() {
        let temp = tempfile::tempdir().unwrap();
        let metadata = temp.path().join("metadata");
        let folder = temp.path().join("space");
        std::fs::create_dir_all(&folder).unwrap();
        let engine = LocalEngine::open(&metadata).unwrap();
        engine.add_space("Local", "local", &folder).unwrap();

        let root = engine
            .create_page_comment(
                "local",
                "wiki/page.md",
                "local-user",
                "You",
                "Review this",
                Some("# Page\n\nText\n"),
                Some(3),
                Some(3),
                None,
            )
            .unwrap();
        let reply = engine
            .create_page_comment(
                "local",
                "wiki/page.md",
                "local-user",
                "You",
                "Done",
                None,
                None,
                None,
                Some(&root.id),
            )
            .unwrap();
        assert_eq!(reply.parent_id.as_deref(), Some(root.id.as_str()));
        engine
            .set_page_comment_resolved("local", &root.id, true)
            .unwrap();
        drop(engine);

        let reopened = LocalEngine::open(&metadata).unwrap();
        let page = reopened
            .list_page_comments("local", "wiki/page.md")
            .unwrap();
        assert_eq!(page.comments.len(), 2);
        assert_eq!(page.snapshots.len(), 1);
        assert!(
            page.comments
                .iter()
                .find(|item| item.id == root.id)
                .unwrap()
                .resolved
        );
        reopened
            .delete_page_comment("local", &root.id, "local-user")
            .unwrap();
        assert!(reopened
            .list_page_comments("local", "wiki/page.md")
            .unwrap()
            .comments
            .is_empty());
    }
}
