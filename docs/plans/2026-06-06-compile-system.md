# Compile System Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a two-phase compile pipeline with decoupled agent harnesses, SSE observability, per-space agent pools, and knowledge graph storage.

**Architecture:** ShallowCompile (sync) dispatches sources to remote agent processes, which autonomously organize wiki pages via HTTP tools. DeepIntegrate (async, per-space mutex) triggers on space-specific events (Personal: auto; Team: post-review-approve), extracting entities and fact triples into PostgreSQL-backed knowledge graph. A shared `ApiClient` in `crates/core/src/client/` provides HTTP endpoints; `crates/agents/` handles protocol, registry, dispatch, and SSE events.

**Tech Stack:** Rust + axum + sqlx + pgvector + tokio, PostgreSQL, Git (libgit2), HTTP/gRPC agent communication

**Spec:** `docs/compile-system-design.md`

---

### File Structure Overview

| File | Action | Purpose |
|------|--------|---------|
| `crates/db/src/migrations/009_graph.sql` | Create | DDL for entities, facts, page_entities |
| `crates/db/src/graph.rs` | Create | PgGraphStore: GraphStore trait impl |
| `crates/core/src/models.rs` | Modify | Entity, Fact, PageEntity structs |
| `crates/core/src/client/mod.rs` | Create | ApiClient struct |
| `crates/core/src/client/pages.rs` | Create | Wiki CRUD HTTP client |
| `crates/core/src/client/graph.rs` | Create | Graph HTTP client |
| `crates/core/src/client/compile.rs` | Create | Compile HTTP client |
| `crates/core/src/compiler/mod.rs` | Modify | ShallowCompile + DeepIntegrate orchestration |
| `crates/core/src/compiler/shallow.rs` | Create | SourceCompiler |
| `crates/core/src/compiler/deep.rs` | Create | GraphBuilder |
| `crates/core/src/compiler/pool.rs` | Create | AgentPool manager |
| `crates/core/src/compiler/dispatch.rs` | Create | Source dispatch to harness |
| `crates/agents/Cargo.toml` | Create | New crate |
| `crates/agents/src/lib.rs` | Create | Module exports |
| `crates/agents/src/protocol.rs` | Create | AgentRequest/Response types |
| `crates/agents/src/client.rs` | Create | AgentClient (HTTP/gRPC) |
| `crates/agents/src/registry.rs` | Create | HarnessRegistry |
| `crates/agents/src/dispatch.rs` | Create | DispatchPolicy |
| `crates/agents/src/events.rs` | Create | SSE event types |
| `crates/agents/src/error.rs` | Create | AgentError |
| `crates/server/src/routes/compile.rs` | Modify | Wire ShallowCompile + SSE endpoint |
| `crates/server/src/routes/review.rs` | Modify | Wire DeepIntegrate trigger on approve |
| `crates/server/src/routes/graph.rs` | Create | Graph query endpoints |

---

### Task 1: Database — Graph Tables & Migration

**Files:**
- Create: `crates/db/src/migrations/009_graph.sql`
- Create: `crates/db/src/graph.rs`
- Modify: `crates/db/src/lib.rs`

- [ ] **Step 1: Write migration SQL**

```sql
-- crates/db/src/migrations/009_graph.sql
CREATE TABLE IF NOT EXISTS entities (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    type        TEXT NOT NULL,
    aliases     TEXT[] NOT NULL DEFAULT '{}',
    description TEXT,
    embedding   vector(__EMBEDDING_DIM__),
    space       TEXT NOT NULL,
    is_orphaned BOOLEAN NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_entities_space ON entities(space);
CREATE INDEX IF NOT EXISTS idx_entities_embedding
    ON entities USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);
CREATE INDEX IF NOT EXISTS idx_entities_name_space ON entities(name, space);

CREATE TABLE IF NOT EXISTS facts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id  UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    predicate   TEXT NOT NULL,
    object_id   UUID REFERENCES entities(id) ON DELETE CASCADE,
    object_lit  TEXT,
    page_path   TEXT NOT NULL,
    source_name TEXT,
    confidence  REAL NOT NULL DEFAULT 1.0,
    space       TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_facts_page_path ON facts(page_path, space);
CREATE INDEX IF NOT EXISTS idx_facts_subject ON facts(subject_id);
CREATE INDEX IF NOT EXISTS idx_facts_space ON facts(space);

CREATE TABLE IF NOT EXISTS page_entities (
    page_path   TEXT NOT NULL,
    entity_id   UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relevance   REAL NOT NULL DEFAULT 1.0,
    space       TEXT NOT NULL,
    PRIMARY KEY (page_path, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_page_entities_entity ON page_entities(entity_id);
```

- [ ] **Step 2: Register migration in db crate**

```rust
// crates/db/src/lib.rs — add after existing migrations:
let sql9 = include_str!("migrations/009_graph.sql")
    .replace("__EMBEDDING_DIM__", &embedding_dim.to_string());
sqlx::raw_sql(&sql9).execute(pool).await.map_err(|e| { tracing::error!("DB error: {e}"); e })?;
```

- [ ] **Step 3: Write GraphStore trait and PgGraphStore**

```rust
// crates/db/src/graph.rs
use async_trait::async_trait;
use pgvector::Vector;
use sqlx::PgPool;
use uuid::Uuid;

use cowiki_core::models::{Entity, Fact, EntityType};

#[async_trait]
pub trait GraphStore {
    async fn query_entity(&self, name: &str, space: &str) -> sqlx::Result<Option<Entity>>;
    async fn find_similar(&self, embedding: &[f32], threshold: f32, space: &str) -> sqlx::Result<Vec<Entity>>;
    async fn upsert_entity(&self, entity: &Entity) -> sqlx::Result<Entity>;
    async fn upsert_fact(&self, fact: &Fact) -> sqlx::Result<Fact>;
    async fn link_page_entity(&self, page_path: &str, entity_id: Uuid, relevance: f32, space: &str) -> sqlx::Result<()>;
    async fn list_entities(&self, page_path: Option<&str>, space: &str) -> sqlx::Result<Vec<Entity>>;
    async fn delete_by_page(&self, page_path: &str, space: &str) -> sqlx::Result<()>;
    async fn find_orphan_entities(&self, space: &str) -> sqlx::Result<Vec<Entity>>;
    async fn delete_orphan_entities(&self, space: &str, older_than_days: i32) -> sqlx::Result<u64>;
}

pub struct PgGraphStore {
    pool: PgPool,
}

impl PgGraphStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GraphStore for PgGraphStore {
    async fn query_entity(&self, name: &str, space: &str) -> sqlx::Result<Option<Entity>> {
        sqlx::query_as::<_, Entity>(
            "SELECT id, name, type, aliases, description, embedding, space, is_orphaned, created_at, updated_at
             FROM entities WHERE name = $1 AND space = $2"
        )
        .bind(name)
        .bind(space)
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_similar(&self, embedding: &[f32], threshold: f32, space: &str) -> sqlx::Result<Vec<Entity>> {
        let emb = Vector::from(embedding.to_vec());
        sqlx::query_as::<_, Entity>(
            r#"SELECT id, name, type, aliases, description, embedding, space, is_orphaned, created_at, updated_at,
               1 - (embedding <=> $1::vector) as similarity
            FROM entities
            WHERE space = $2 AND embedding IS NOT NULL
              AND 1 - (embedding <=> $1::vector) > $3
            ORDER BY similarity DESC
            LIMIT 10"#
        )
        .bind(&emb)
        .bind(space)
        .bind(threshold)
        .fetch_all(&self.pool)
        .await
    }

    async fn upsert_entity(&self, entity: &Entity) -> sqlx::Result<Entity> {
        let emb = entity.embedding.as_ref().map(|e| Vector::from(e.clone()));
        sqlx::query_as::<_, Entity>(
            r#"INSERT INTO entities (name, type, aliases, description, embedding, space)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (name, space) DO UPDATE SET
                aliases = EXCLUDED.aliases,
                description = COALESCE(EXCLUDED.description, entities.description),
                embedding = COALESCE(EXCLUDED.embedding, entities.embedding),
                is_orphaned = false,
                updated_at = now()
            RETURNING id, name, type, aliases, description, embedding, space, is_orphaned, created_at, updated_at"#
        )
        .bind(&entity.name)
        .bind(&entity.type_)
        .bind(&entity.aliases)
        .bind(&entity.description)
        .bind(&emb)
        .bind(&entity.space)
        .fetch_one(&self.pool)
        .await
    }

    async fn upsert_fact(&self, fact: &Fact) -> sqlx::Result<Fact> {
        sqlx::query_as::<_, Fact>(
            r#"INSERT INTO facts (subject_id, predicate, object_id, object_lit, page_path, source_name, confidence, space)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT DO NOTHING
            RETURNING id, subject_id, predicate, object_id, object_lit, page_path, source_name, confidence, space, created_at"#
        )
        .bind(fact.subject_id)
        .bind(&fact.predicate)
        .bind(fact.object_id)
        .bind(&fact.object_lit)
        .bind(&fact.page_path)
        .bind(&fact.source_name)
        .bind(fact.confidence)
        .bind(&fact.space)
        .fetch_one(&self.pool)
        .await
    }

    async fn link_page_entity(&self, page_path: &str, entity_id: Uuid, relevance: f32, space: &str) -> sqlx::Result<()> {
        sqlx::query(
            r#"INSERT INTO page_entities (page_path, entity_id, relevance, space)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (page_path, entity_id) DO UPDATE SET relevance = EXCLUDED.relevance"#
        )
        .bind(page_path)
        .bind(entity_id)
        .bind(relevance)
        .bind(space)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_entities(&self, page_path: Option<&str>, space: &str) -> sqlx::Result<Vec<Entity>> {
        if let Some(path) = page_path {
            sqlx::query_as::<_, Entity>(
                r#"SELECT e.id, e.name, e.type, e.aliases, e.description, e.embedding, e.space,
                   e.is_orphaned, e.created_at, e.updated_at
                FROM entities e
                JOIN page_entities pe ON e.id = pe.entity_id
                WHERE pe.page_path = $1 AND pe.space = $2"#
            )
            .bind(path)
            .bind(space)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, Entity>(
                "SELECT id, name, type, aliases, description, embedding, space, is_orphaned, created_at, updated_at
                 FROM entities WHERE space = $1 ORDER BY updated_at DESC"
            )
            .bind(space)
            .fetch_all(&self.pool)
            .await
        }
    }

    async fn delete_by_page(&self, page_path: &str, space: &str) -> sqlx::Result<()> {
        // Mark entities as orphaned if no remaining page_entities
        sqlx::query(
            r#"UPDATE entities SET is_orphaned = true
            WHERE id IN (
                SELECT entity_id FROM page_entities WHERE page_path = $1 AND space = $2
            )
            AND id NOT IN (
                SELECT entity_id FROM page_entities WHERE (page_path != $1 OR space != $2)
            )"#
        )
        .bind(page_path)
        .bind(space)
        .execute(&self.pool)
        .await?;

        sqlx::query("DELETE FROM page_entities WHERE page_path = $1 AND space = $2")
            .bind(page_path)
            .bind(space)
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM facts WHERE page_path = $1 AND space = $2")
            .bind(page_path)
            .bind(space)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn find_orphan_entities(&self, space: &str) -> sqlx::Result<Vec<Entity>> {
        sqlx::query_as::<_, Entity>(
            "SELECT id, name, type, aliases, description, embedding, space, is_orphaned, created_at, updated_at
             FROM entities WHERE space = $1 AND is_orphaned = true"
        )
        .bind(space)
        .fetch_all(&self.pool)
        .await
    }

    async fn delete_orphan_entities(&self, space: &str, older_than_days: i32) -> sqlx::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM entities WHERE space = $1 AND is_orphaned = true AND updated_at < now() - $2::interval"
        )
        .bind(space)
        .bind(format!("{} days", older_than_days))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
```

- [ ] **Step 4: Add Entity, Fact, PageEntity models to core**

```rust
// Add to crates/core/src/models.rs:

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Entity {
    pub id: Uuid,
    pub name: String,
    #[sqlx(rename = "type")]
    pub type_: String,
    pub aliases: Vec<String>,
    pub description: Option<String>,
    #[sqlx(try_from = "Option<Vector>")]
    pub embedding: Option<Vec<f32>>,
    pub space: String,
    pub is_orphaned: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Fact {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub predicate: String,
    pub object_id: Option<Uuid>,
    pub object_lit: Option<String>,
    pub page_path: String,
    pub source_name: Option<String>,
    pub confidence: f32,
    pub space: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityType {
    pub name: String,
}

impl EntityType {
    pub const PERSON: &str = "Person";
    pub const TECHNOLOGY: &str = "Technology";
    pub const CONCEPT: &str = "Concept";
    pub const EVENT: &str = "Event";
}
```

- [ ] **Step 5: Add graph module to db crate**

```rust
// crates/db/src/lib.rs — add:
pub mod graph;
```

- [ ] **Step 6: Commit**

```bash
git add crates/db/src/migrations/009_graph.sql crates/db/src/graph.rs crates/db/src/lib.rs crates/core/src/models.rs
git commit -m "feat: add graph data model — entities, facts, page_entities tables with GraphStore trait

- 009_graph.sql: DDL for entities (with pgvector embedding), facts (triples), page_entities (junction)
- graph.rs: GraphStore trait + PgGraphStore impl with all CRUD operations
- models.rs: Entity, Fact, EntityType structs

Refs: #15"
```

---

### Task 2: API Client — `crates/core/src/client/`

**Files:**
- Create: `crates/core/src/client/mod.rs`
- Create: `crates/core/src/client/pages.rs`
- Create: `crates/core/src/client/graph.rs`
- Create: `crates/core/src/client/compile.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Create ApiClient struct**

```rust
// crates/core/src/client/mod.rs
pub mod pages;
pub mod graph;
pub mod compile;

use reqwest::Client as HttpClient;

#[derive(Clone)]
pub struct ApiClient {
    http: HttpClient,
    base_url: String,
    auth_token: String,
}

impl ApiClient {
    pub fn new(base_url: String, auth_token: String) -> Self {
        Self {
            http: HttpClient::new(),
            base_url,
            auth_token,
        }
    }

    pub fn pages(&self) -> pages::PagesClient<'_> {
        pages::PagesClient { parent: self }
    }

    pub fn graph(&self) -> graph::GraphClient<'_> {
        graph::GraphClient { parent: self }
    }

    pub fn compile(&self) -> compile::CompileClient<'_> {
        compile::CompileClient { parent: self }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.auth_token)
    }
}
```

- [ ] **Step 2: Create pages client**

```rust
// crates/core/src/client/pages.rs
use serde::{Deserialize, Serialize};

use super::ApiClient;

pub struct PagesClient<'a> {
    pub(crate) parent: &'a ApiClient,
}

#[derive(Serialize)]
struct CreateWikiRequest {
    path: String,
    content: String,
    branch: String,
}

#[derive(Serialize)]
struct EditWikiRequest {
    path: String,
    content: String,
    branch: String,
}

#[derive(Deserialize)]
struct WikiListResponse {
    entries: Vec<WikiEntry>,
}

#[derive(Deserialize)]
struct WikiEntry {
    name: String,
    is_dir: bool,
    path: String,
}

impl<'a> PagesClient<'a> {
    pub async fn create_wiki(&self, path: &str, content: &str, branch: &str) -> Result<String, String> {
        let resp = self.parent.http
            .post(self.parent.url("/api/pages/wiki"))
            .header("Authorization", self.parent.auth_header())
            .json(&CreateWikiRequest {
                path: path.to_string(),
                content: content.to_string(),
                branch: branch.to_string(),
            })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.text().await.map_err(|e| e.to_string())
    }

    pub async fn read_wiki(&self, path: &str, branch: &str) -> Result<String, String> {
        let resp = self.parent.http
            .get(self.parent.url(&format!("/api/pages/wiki?path={}&branch={}", path, branch)))
            .header("Authorization", self.parent.auth_header())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.text().await.map_err(|e| e.to_string())
    }

    pub async fn edit_wiki(&self, path: &str, content: &str, branch: &str) -> Result<String, String> {
        let resp = self.parent.http
            .put(self.parent.url("/api/pages/wiki"))
            .header("Authorization", self.parent.auth_header())
            .json(&EditWikiRequest {
                path: path.to_string(),
                content: content.to_string(),
                branch: branch.to_string(),
            })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.text().await.map_err(|e| e.to_string())
    }

    pub async fn ls_wiki(&self, path: Option<&str>, branch: &str) -> Result<Vec<WikiEntry>, String> {
        let url = match path {
            Some(p) => format!("/api/pages/wiki/ls?path={}&branch={}", p, branch),
            None => format!("/api/pages/wiki/ls?branch={}", branch),
        };
        let resp = self.parent.http
            .get(self.parent.url(&url))
            .header("Authorization", self.parent.auth_header())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body: WikiListResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(body.entries)
    }

    pub async fn mkdir_wiki(&self, path: &str, branch: &str) -> Result<String, String> {
        let resp = self.parent.http
            .post(self.parent.url("/api/pages/wiki/mkdir"))
            .header("Authorization", self.parent.auth_header())
            .json(&serde_json::json!({ "path": path, "branch": branch }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.text().await.map_err(|e| e.to_string())
    }

    pub async fn rm_wiki(&self, path: &str, branch: &str) -> Result<String, String> {
        let resp = self.parent.http
            .delete(self.parent.url(&format!("/api/pages/wiki?path={}&branch={}", path, branch)))
            .header("Authorization", self.parent.auth_header())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.text().await.map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 3: Create graph client**

```rust
// crates/core/src/client/graph.rs
use serde_json::Value;

use super::ApiClient;

pub struct GraphClient<'a> {
    pub(crate) parent: &'a ApiClient,
}

impl<'a> GraphClient<'a> {
    pub async fn query_entity(&self, name: &str, space: &str) -> Result<Option<Value>, String> {
        let resp = self.parent.http
            .get(self.parent.url(&format!("/api/entities?name={}&space={}", name, space)))
            .header("Authorization", self.parent.auth_header())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status() == 404 {
            return Ok(None);
        }
        resp.json().await.map(|v| Some(v)).map_err(|e| e.to_string())
    }

    pub async fn find_similar(&self, embedding: &[f32], threshold: f32, space: &str) -> Result<Vec<Value>, String> {
        let resp = self.parent.http
            .post(self.parent.url("/api/entities/similar"))
            .header("Authorization", self.parent.auth_header())
            .json(&serde_json::json!({
                "embedding": embedding,
                "threshold": threshold,
                "space": space,
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn upsert_entity(&self, entity: Value) -> Result<Value, String> {
        let resp = self.parent.http
            .post(self.parent.url("/api/entities"))
            .header("Authorization", self.parent.auth_header())
            .json(&entity)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn upsert_fact(&self, fact: Value) -> Result<Value, String> {
        let resp = self.parent.http
            .post(self.parent.url("/api/facts"))
            .header("Authorization", self.parent.auth_header())
            .json(&fact)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn link_page_entity(&self, page_path: &str, entity_name: &str, relevance: f32, space: &str) -> Result<Value, String> {
        let resp = self.parent.http
            .post(self.parent.url("/api/entities/link"))
            .header("Authorization", self.parent.auth_header())
            .json(&serde_json::json!({
                "page_path": page_path,
                "entity_name": entity_name,
                "relevance": relevance,
                "space": space,
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn list_entities(&self, page_path: Option<&str>, space: &str) -> Result<Vec<Value>, String> {
        let url = match page_path {
            Some(p) => format!("/api/entities?space={}&page_path={}", space, p),
            None => format!("/api/entities?space={}", space),
        };
        let resp = self.parent.http
            .get(self.parent.url(&url))
            .header("Authorization", self.parent.auth_header())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json().await.map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 4: Create compile client**

```rust
// crates/core/src/client/compile.rs
use serde::Deserialize;
use serde_json::Value;

use super::ApiClient;

pub struct CompileClient<'a> {
    pub(crate) parent: &'a ApiClient,
}

#[derive(Deserialize)]
pub struct CompileResponse {
    pub pages: Vec<CompiledPage>,
    pub skipped: usize,
}

#[derive(Deserialize)]
pub struct CompiledPage {
    pub slug: String,
    pub title: String,
    pub summary: String,
}

impl<'a> CompileClient<'a> {
    pub async fn run(&self, branch: &str) -> Result<CompileResponse, String> {
        let resp = self.parent.http
            .post(self.parent.url("/api/compile"))
            .header("Authorization", self.parent.auth_header())
            .json(&serde_json::json!({ "branch": branch }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn rebuild_graph(&self, space: &str) -> Result<Value, String> {
        let resp = self.parent.http
            .post(self.parent.url("/api/compile/graph/rebuild"))
            .header("Authorization", self.parent.auth_header())
            .json(&serde_json::json!({ "space": space }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json().await.map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 5: Register client module in lib.rs**

```rust
// crates/core/src/lib.rs — add:
pub mod client;
```

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/client/ crates/core/src/lib.rs
git commit -m "feat: add server-internal API client (crates/core/src/client/)

- mod.rs: ApiClient struct with HTTP client, auth, URL builder
- pages.rs: create_wiki, read_wiki, edit_wiki, ls_wiki, mkdir_wiki, rm_wiki
- graph.rs: query_entity, find_similar, upsert_entity, upsert_fact, link_page_entity, list_entities
- compile.rs: run compile, rebuild_graph

Refs: #15"
```

---

### Task 3: Agent Protocol — `crates/agents/`

**Files:**
- Create: `crates/agents/Cargo.toml`
- Create: `crates/agents/src/lib.rs`
- Create: `crates/agents/src/protocol.rs`
- Create: `crates/agents/src/client.rs`
- Create: `crates/agents/src/registry.rs`
- Create: `crates/agents/src/dispatch.rs`
- Create: `crates/agents/src/events.rs`
- Create: `crates/agents/src/error.rs`

- [ ] **Step 1: Create agents crate Cargo.toml**

```toml
# crates/agents/Cargo.toml
[package]
name = "cowiki-agents"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
thiserror = "2"
cowiki-core = { path = "../core" }
```

- [ ] **Step 2: Write protocol types**

```rust
// crates/agents/src/protocol.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub task_type: String,       // "compile_page" | "build_graph" | "review_submission"
    pub system_prompt: String,
    pub user_input: String,
    pub workspace_path: String,
    pub tools: Vec<ToolDef>,
    pub output_schema: Option<serde_json::Value>,
    pub config: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema for tool parameters
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,
    #[serde(default = "default_token_budget")]
    pub token_budget: u32,
    pub model: Option<String>,
}

fn default_max_rounds() -> u32 { 20 }
fn default_token_budget() -> u32 { 100_000 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub usage: Option<UsageInfo>,
    pub rounds: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessRegistration {
    pub name: String,
    pub task_type: String,
    pub endpoint: String,       // "http://host:port/agent/run"
    pub transport: TransportType,
    pub max_concurrency: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportType {
    Http,
    Grpc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub compile_page: PoolEntry,
    pub build_graph: PoolEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolEntry {
    pub size: u32,
    pub harness: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierLimit {
    pub tier: String,
    pub max_compile_agents: u32,
    pub max_graph_agents: u32,
}

impl TierLimit {
    pub fn for_tier(tier: &str) -> Self {
        match tier {
            "free" => Self { tier: "free".into(), max_compile_agents: 1, max_graph_agents: 1 },
            "pro" => Self { tier: "pro".into(), max_compile_agents: 4, max_graph_agents: 1 },
            "enterprise" => Self { tier: "enterprise".into(), max_compile_agents: 16, max_graph_agents: 1 },
            _ => Self { tier: "free".into(), max_compile_agents: 1, max_graph_agents: 1 },
        }
    }
}
```

- [ ] **Step 3: Write AgentClient (HTTP only)**

```rust
// crates/agents/src/client.rs
use reqwest::Client as HttpClient;

use crate::error::AgentError;
use crate::protocol::{AgentRequest, AgentResponse};

#[derive(Clone)]
pub struct AgentClient {
    http: HttpClient,
}

impl AgentClient {
    pub fn new() -> Self {
        Self { http: HttpClient::new() }
    }

    pub async fn run(&self, endpoint: &str, request: AgentRequest) -> Result<AgentResponse, AgentError> {
        let resp = self.http
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AgentError::HttpStatus(resp.status().as_u16()));
        }

        let body: AgentResponse = resp.json().await
            .map_err(|e| AgentError::Protocol(e.to_string()))?;

        Ok(body)
    }
}
```

- [ ] **Step 4: Write HarnessRegistry**

```rust
// crates/agents/src/registry.rs
use std::collections::HashMap;

use crate::error::AgentError;
use crate::protocol::HarnessRegistration;

#[derive(Default)]
pub struct HarnessRegistry {
    harnesses: HashMap<String, HarnessRegistration>,
}

impl HarnessRegistry {
    pub fn new() -> Self {
        Self { harnesses: HashMap::new() }
    }

    pub fn register(&mut self, h: HarnessRegistration) {
        self.harnesses.insert(h.name.clone(), h);
    }

    pub fn get(&self, name: &str) -> Option<&HarnessRegistration> {
        self.harnesses.get(name)
    }

    pub fn get_for_task(&self, task_type: &str) -> Vec<&HarnessRegistration> {
        self.harnesses
            .values()
            .filter(|h| h.task_type == task_type)
            .collect()
    }

    pub fn list_all(&self) -> Vec<&HarnessRegistration> {
        self.harnesses.values().collect()
    }
}

impl HarnessRegistry {
    /// Register default harnesses for MVP
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(HarnessRegistration {
            name: "compile-simple".into(),
            task_type: "compile_page".into(),
            endpoint: "http://localhost:9100/agent/run".into(),
            transport: crate::protocol::TransportType::Http,
            max_concurrency: 4,
        });
        registry.register(HarnessRegistration {
            name: "entity-extract".into(),
            task_type: "build_graph".into(),
            endpoint: "http://localhost:9101/agent/run".into(),
            transport: crate::protocol::TransportType::Http,
            max_concurrency: 1,
        });
        registry
    }
}
```

- [ ] **Step 5: Write DispatchPolicy**

```rust
// crates/agents/src/dispatch.rs
use crate::protocol::HarnessRegistration;

pub struct DispatchPolicy;

impl DispatchPolicy {
    /// Select harness for a given task type and context.
    /// MVP: always returns the first matching harness.
    /// Future: complexity-based routing (source type, token count, file count).
    pub fn dispatch<'a>(
        task_type: &str,
        _source_types: &[String],
        harnesses: &'a [&HarnessRegistration],
    ) -> Option<&'a HarnessRegistration> {
        harnesses.first().copied()
    }
}
```

- [ ] **Step 6: Write SSE event types**

```rust
// crates/agents/src/events.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CompileEvent {
    #[serde(rename = "phase")]
    Phase {
        phase: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pages_count: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "agent-start")]
    AgentStart {
        agent_id: String,
        harness_type: String,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "llm-round")]
    LlmRound {
        round: u32,
        token_count: u64,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "tool-call")]
    ToolCall {
        tool: String,
        args: serde_json::Value,
        round: u32,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "tool-result")]
    ToolResult {
        tool: String,
        success: bool,
        summary: Option<String>,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "entity")]
    Entity {
        entity: String,
        #[serde(rename = "type")]
        entity_type: String,
        confidence: f32,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "fact")]
    Fact {
        subject: String,
        predicate: String,
        object: String,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
        recoverable: bool,
        timestamp: DateTime<Utc>,
    },
}

impl CompileEvent {
    pub fn now() -> DateTime<Utc> { Utc::now() }

    pub fn phase_started(phase: &str) -> Self {
        Self::Phase {
            phase: phase.to_string(),
            status: "started".to_string(),
            pages_count: None,
            message: None,
            timestamp: Self::now(),
        }
    }

    pub fn phase_completed(phase: &str, pages_count: usize) -> Self {
        Self::Phase {
            phase: phase.to_string(),
            status: "completed".to_string(),
            pages_count: Some(pages_count),
            message: None,
            timestamp: Self::now(),
        }
    }

    pub fn agent_start(agent_id: &str, harness: &str) -> Self {
        Self::AgentStart {
            agent_id: agent_id.to_string(),
            harness_type: harness.to_string(),
            timestamp: Self::now(),
        }
    }

    pub fn tool_call(tool: &str, args: serde_json::Value, round: u32) -> Self {
        Self::ToolCall {
            tool: tool.to_string(),
            args,
            round,
            timestamp: Self::now(),
        }
    }

    pub fn tool_result(tool: &str, success: bool, summary: Option<&str>) -> Self {
        Self::ToolResult {
            tool: tool.to_string(),
            success,
            summary: summary.map(|s| s.to_string()),
            timestamp: Self::now(),
        }
    }

    pub fn entity_found(name: &str, entity_type: &str, confidence: f32) -> Self {
        Self::Entity {
            entity: name.to_string(),
            entity_type: entity_type.to_string(),
            confidence,
            timestamp: Self::now(),
        }
    }

    pub fn fact_extracted(subject: &str, predicate: &str, object: &str) -> Self {
        Self::Fact {
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            timestamp: Self::now(),
        }
    }
}
```

- [ ] **Step 7: Write AgentError**

```rust
// crates/agents/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("HTTP status {0}")]
    HttpStatus(u16),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("timeout")]
    Timeout,
    #[error("pool exhausted")]
    PoolExhausted,
    #[error("harness not found: {0}")]
    HarnessNotFound(String),
}
```

- [ ] **Step 8: Write lib.rs**

```rust
// crates/agents/src/lib.rs
pub mod protocol;
pub mod client;
pub mod registry;
pub mod dispatch;
pub mod events;
pub mod error;
```

- [ ] **Step 9: Add agents crate to workspace**

```toml
# In Cargo.toml (root), add to workspace members:
"crates/agents"
```

- [ ] **Step 10: Commit**

---

### Task 4: ShallowCompile — SourceCompiler

**Files:**
- Create: `crates/core/src/compiler/mod.rs`
- Create: `crates/core/src/compiler/shallow.rs`
- Move: `crates/core/src/compiler.rs` → `crates/core/src/compiler/shallow.rs` (merge)
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Create compiler module structure**

```rust
// crates/core/src/compiler/mod.rs
pub mod shallow;
pub mod deep;
pub mod pool;
pub mod dispatch;

pub use shallow::SourceCompiler;
pub use deep::GraphBuilder;
pub use pool::AgentPool;
```

- [ ] **Step 2: Write SourceCompiler**

```rust
// crates/core/src/compiler/shallow.rs
use std::collections::HashMap;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use cowiki_agents::client::AgentClient;
use cowiki_agents::protocol::{AgentRequest, AgentConfig, ToolDef};
use cowiki_agents::events::CompileEvent;

use crate::compiler::pool::AgentPool;
use crate::compiler::dispatch;

pub struct SourceCompiler {
    agent_pool: AgentPool,
    event_tx: broadcast::Sender<CompileEvent>,
}

impl SourceCompiler {
    pub fn new(
        agent_pool: AgentPool,
        event_tx: broadcast::Sender<CompileEvent>,
    ) -> Self {
        Self { agent_pool, event_tx }
    }

    pub async fn compile(
        &self,
        sources: &[(String, String)],  // (name, content)
        branch: &str,
        space: &str,
    ) -> Result<Vec<CompiledPage>, CompileError> {
        // 1. Emit phase start
        let _ = self.event_tx.send(CompileEvent::phase_started("shallow"));

        // 2. Acquire agent from pool
        let agent = self.agent_pool
            .acquire(space, "compile_page")
            .await
            .map_err(|_| CompileError::PoolExhausted)?;

        let _ = self.event_tx.send(CompileEvent::agent_start(&agent.id, &agent.harness));

        // 3. Build agent request with wiki tools
        let wiki_tools = vec![
            ToolDef {
                name: "ls_wiki".into(),
                description: "List wiki directory contents".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }),
            },
            ToolDef {
                name: "mkdir_wiki".into(),
                description: "Create a wiki subdirectory".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            },
            ToolDef {
                name: "create_wiki".into(),
                description: "Create a new wiki page".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDef {
                name: "read_wiki".into(),
                description: "Read a wiki page's content".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            },
            ToolDef {
                name: "edit_wiki".into(),
                description: "Edit an existing wiki page".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDef {
                name: "rm_wiki".into(),
                description: "Delete a wiki page or empty directory".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            },
        ];

        let combined = sources
            .iter()
            .map(|(name, content)| format!("## Source: {name}\n\n{content}"))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let request = AgentRequest {
            task_type: "compile_page".into(),
            system_prompt: COMPILE_SYSTEM_PROMPT.into(),
            user_input: combined,
            workspace_path: format!("{}/wiki/", branch),
            tools: wiki_tools,
            output_schema: None,  // Agent freely organizes pages; no fixed schema
            config: AgentConfig {
                max_rounds: 20,
                token_budget: 100_000,
                model: None,
            },
        };

        // 4. Execute agent (remote HTTP call)
        let client = AgentClient::new();
        let response = client.run(&agent.endpoint, request).await?;

        let _ = self.event_tx.send(CompileEvent::phase_completed("shallow", 0));

        Ok(vec![])  // Actual pages parsed from agent response
    }
}

const COMPILE_SYSTEM_PROMPT: &str = r#"You are a knowledge compiler. Given source documents, organize them into well-structured wiki pages.

Use the wiki tools to create pages with clear organization:
- Create directories to group related topics (mkdir_wiki)
- Create one page per distinct topic (create_wiki)
- Use descriptive file names (infra/docker-networking.md, not page1.md)
- Read existing pages before editing them (read_wiki, edit_wiki)

Be concise. One topic per page. Use clear headings. Attribute claims to sources."#;

#[derive(Debug)]
pub struct CompiledPage {
    pub title: String,
    pub summary: String,
    pub path: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("agent pool exhausted")]
    PoolExhausted,
    #[error("agent error: {0}")]
    Agent(String),
}
```

- [ ] **Step 3: Commit**

---

### Task 5: DeepIntegrate — GraphBuilder

**Files:**
- Create: `crates/core/src/compiler/deep.rs`

- [ ] **Step 1: Write GraphBuilder**

```rust
// crates/core/src/compiler/deep.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

use cowiki_agents::client::AgentClient;
use cowiki_agents::protocol::{AgentRequest, AgentConfig, ToolDef};
use cowiki_agents::events::CompileEvent;

use crate::compiler::pool::AgentPool;

/// Per-space mutex map. Only one graph build per space at a time.
pub type SpaceMutexes = Arc<Mutex<HashMap<String, ()>>>;

pub struct GraphBuilder {
    agent_pool: AgentPool,
    space_mutexes: SpaceMutexes,
    event_tx: broadcast::Sender<CompileEvent>,
}

impl GraphBuilder {
    pub fn new(
        agent_pool: AgentPool,
        event_tx: broadcast::Sender<CompileEvent>,
    ) -> Self {
        Self {
            agent_pool,
            space_mutexes: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    /// Try to acquire the per-space mutex. Returns Ok if acquired, Err(409) if busy.
    pub async fn try_acquire_space(&self, space: &str) -> Result<SpaceGuard, SpaceBusy> {
        let mut map = self.space_mutexes.lock().await;
        if map.contains_key(space) {
            return Err(SpaceBusy);
        }
        map.insert(space.to_string(), ());
        Ok(SpaceGuard {
            space: space.to_string(),
            mutexes: self.space_mutexes.clone(),
        })
    }

    pub async fn build_graph(
        &self,
        pages: &[(String, String)],  // (path, content)
        space: &str,
        space_guard: SpaceGuard,
    ) -> Result<(), GraphError> {
        let _ = self.event_tx.send(CompileEvent::phase_started("deep"));

        let agent = self.agent_pool
            .acquire(space, "build_graph")
            .await
            .map_err(|_| GraphError::PoolExhausted)?;

        let graph_tools = vec![
            ToolDef {
                name: "query_entity".into(),
                description: "Look up an entity by name for disambiguation".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    },
                    "required": ["name"]
                }),
            },
            ToolDef {
                name: "find_similar".into(),
                description: "Find similar entities by embedding".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "embedding": { "type": "array", "items": { "type": "number" } },
                        "threshold": { "type": "number" }
                    },
                    "required": ["embedding"]
                }),
            },
            ToolDef {
                name: "upsert_entity".into(),
                description: "Create or update an entity node".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "type": { "type": "string" },
                        "aliases": { "type": "array", "items": { "type": "string" } },
                        "description": { "type": "string" }
                    },
                    "required": ["name", "type"]
                }),
            },
            ToolDef {
                name: "upsert_fact".into(),
                description: "Create or update a fact triple".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "subject": { "type": "string" },
                        "predicate": { "type": "string" },
                        "object": { "type": "string" },
                        "confidence": { "type": "number" }
                    },
                    "required": ["subject", "predicate", "object"]
                }),
            },
            ToolDef {
                name: "link_page_entity".into(),
                description: "Associate a wiki page with an entity".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "page_path": { "type": "string" },
                        "entity_name": { "type": "string" },
                        "relevance": { "type": "number" }
                    },
                    "required": ["page_path", "entity_name"]
                }),
            },
            ToolDef {
                name: "list_entities".into(),
                description: "List entities, optionally filtered by page".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "page_path": { "type": "string" }
                    }
                }),
            },
        ];

        let pages_text = pages
            .iter()
            .map(|(path, content)| format!("## Page: {path}\n\n{content}"))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let request = AgentRequest {
            task_type: "build_graph".into(),
            system_prompt: GRAPH_SYSTEM_PROMPT.into(),
            user_input: pages_text,
            workspace_path: format!("{}/wiki/", space),
            tools: graph_tools,
            output_schema: None,
            config: AgentConfig {
                max_rounds: 30,
                token_budget: 200_000,
                model: None,
            },
        };

        let client = AgentClient::new();
        let response = client.run(&agent.endpoint, request).await
            .map_err(|e| GraphError::Agent(e.to_string()))?;

        if response.success {
            let _ = self.event_tx.send(CompileEvent::phase_completed("deep", 0));
            Ok(())
        } else {
            Err(GraphError::Agent(response.error.unwrap_or_default()))
        }
    }
}

pub struct SpaceGuard {
    space: String,
    mutexes: SpaceMutexes,
}

impl Drop for SpaceGuard {
    fn drop(&mut self) {
        // We can't do async Drop, so this is best-effort via spawn
        let space = self.space.clone();
        let mutexes = self.mutexes.clone();
        tokio::spawn(async move {
            mutexes.lock().await.remove(&space);
        });
    }
}

#[derive(Debug)]
pub struct SpaceBusy;

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("agent pool exhausted")]
    PoolExhausted,
    #[error("space busy: graph build already running")]
    SpaceBusy,
    #[error("agent error: {0}")]
    Agent(String),
}

const GRAPH_SYSTEM_PROMPT: &str = r#"You are a knowledge graph builder. Read wiki pages and extract structured knowledge:

1. Identify distinct entities (technologies, concepts, people, events)
2. Extract factual triples: (entity, predicate, entity-or-value)
3. Link each entity to the pages that mention it

Use the tools provided to:
- query_entity: check if an entity already exists
- upsert_entity: create new entities or update existing ones
- upsert_fact: record a fact triple
- link_page_entity: connect pages to their entities
- list_entities: see what's already in the graph

Focus on accuracy over quantity. Each fact should be clearly supported by the page content."#;
```

- [ ] **Step 2: Commit**

---

### Task 6: Agent Pool

**Files:**
- Create: `crates/core/src/compiler/pool.rs`

- [ ] **Step 1: Write AgentPool**

```rust
// crates/core/src/compiler/pool.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Semaphore, SemaphorePermit};

use cowiki_agents::protocol::{PoolConfig, TierLimit};
use cowiki_agents::registry::HarnessRegistry;

pub struct AgentPool {
    /// Per-space, per-task-type semaphore controlling concurrency
    semaphores: Arc<tokio::sync::Mutex<HashMap<String, Arc<Semaphore>>>>,
    registry: HarnessRegistry,
    configs: Arc<tokio::sync::Mutex<HashMap<String, PoolConfig>>>,
}

impl AgentPool {
    pub fn new(registry: HarnessRegistry) -> Self {
        Self {
            semaphores: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            registry,
            configs: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Configure pool for a space. Enforces tier limits.
    pub async fn configure(&self, space: &str, config: PoolConfig, tier: &str) -> Result<(), String> {
        let limit = TierLimit::for_tier(tier);
        if config.compile_page.size > limit.max_compile_agents {
            return Err(format!(
                "compile_page pool size {} exceeds tier max {}",
                config.compile_page.size, limit.max_compile_agents
            ));
        }
        if config.build_graph.size > limit.max_graph_agents {
            return Err(format!(
                "build_graph pool size {} exceeds tier max {}",
                config.build_graph.size, limit.max_graph_agents
            ));
        }

        // Create/update semaphores
        let mut sem_map = self.semaphores.lock().await;
        sem_map.insert(
            sem_key(space, "compile_page"),
            Arc::new(Semaphore::new(config.compile_page.size as usize)),
        );
        sem_map.insert(
            sem_key(space, "build_graph"),
            Arc::new(Semaphore::new(config.build_graph.size as usize)),
        );

        self.configs.lock().await.insert(space.to_string(), config);
        Ok(())
    }

    /// Acquire an agent for a task. Blocks until one is available.
    pub async fn acquire(&self, space: &str, task_type: &str) -> Result<PooledAgent, PoolError> {
        let key = sem_key(space, task_type);
        let sem = {
            let map = self.semaphores.lock().await;
            map.get(&key).cloned().unwrap_or_else(|| {
                Arc::new(Semaphore::new(1)) // Default: 1 agent
            })
        };

        let permit = sem.acquire().await.map_err(|_| PoolError::Closed)?;

        let harness_name = {
            let configs = self.configs.lock().await;
            configs
                .get(space)
                .and_then(|c| {
                    if task_type == "compile_page" {
                        Some(c.compile_page.harness.clone())
                    } else {
                        Some(c.build_graph.harness.clone())
                    }
                })
                .unwrap_or_else(|| {
                    if task_type == "compile_page" {
                        "compile-simple".into()
                    } else {
                        "entity-extract".into()
                    }
                })
        };

        let harness = self.registry
            .get(&harness_name)
            .ok_or(PoolError::HarnessNotFound(harness_name))?;

        Ok(PooledAgent {
            id: uuid::Uuid::new_v4().to_string(),
            harness: harness.name.clone(),
            endpoint: harness.endpoint.clone(),
            _permit: permit,
        })
    }
}

fn sem_key(space: &str, task_type: &str) -> String {
    format!("{}:{}", space, task_type)
}

pub struct PooledAgent {
    pub id: String,
    pub harness: String,
    pub endpoint: String,
    _permit: SemaphorePermit<'static>,  // Dropping releases the slot
}

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("pool closed")]
    Closed,
    #[error("harness not found: {0}")]
    HarnessNotFound(String),
}
```

- [ ] **Step 2: Commit**

---

### Task 7: Wire Routes — Compile & SSE

**Files:**
- Modify: `crates/server/src/routes/compile.rs`
- Modify: `crates/server/src/routes/review.rs`
- Create: `crates/server/src/routes/graph.rs`
- Modify: `crates/server/src/routes/mod.rs`

- [ ] **Step 1: Update AppState to hold pool and event channel**

```rust
// In crates/server/src/main.rs (or wherever AppState is defined):
use cowiki_core::compiler::AgentPool;
use cowiki_core::compiler::GraphBuilder;
use cowiki_core::compiler::SourceCompiler;
use cowiki_agents::events::CompileEvent;
use cowiki_agents::registry::HarnessRegistry;
use tokio::sync::broadcast;

// Add to AppState:
pub struct AppState {
    // ... existing fields ...
    pub agent_pool: AgentPool,
    pub source_compiler: SourceCompiler,
    pub graph_builder: GraphBuilder,
    pub event_tx: broadcast::Sender<CompileEvent>,
}
```

- [ ] **Step 2: Rewrite compile route to use SourceCompiler**

```rust
// crates/server/src/routes/compile.rs — replace do_compile:
async fn do_compile(
    state: &AppState,
    repo: &cowiki_core::git::WikiRepo,
    branch: &str,
    space: &str,
) -> Result<Json<CompileResponse>> {
    // 1. Load state
    let mut compile_state = load_state(repo, branch);

    // 2. List and hash sources
    let source_files = repo.list_files(branch, "sources")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut new_sources = Vec::new();
    let mut skipped = 0usize;
    for file in &source_files {
        if let Some(content) = repo.read_file(branch, file)? {
            let text = String::from_utf8_lossy(&content).into_owned();
            let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
            let name = file.rsplit('/').next().unwrap_or(file).to_string();
            if compile_state.sources.get(&name) == Some(&hash) {
                skipped += 1;
                continue;
            }
            compile_state.sources.insert(name.clone(), hash);
            new_sources.push((name, text));
        }
    }

    if new_sources.is_empty() {
        return Ok(Json(CompileResponse { pages: vec![], skipped }));
    }

    // 3. Dispatch sources by type
    let dispatched = dispatch_sources(&new_sources);

    // 4. Run ShallowCompile via SourceCompiler
    let pages = state.source_compiler.compile(&dispatched, branch, space).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 5. Save state
    save_state(repo, branch, &compile_state);

    // 6. Fire DeepIntegrate based on space type
    let is_personal = space.starts_with("personal:");
    if is_personal {
        let graph = state.graph_builder.clone();
        let pages_for_graph: Vec<_> = pages.iter()
            .map(|p| (p.path.clone(), String::new()))
            .collect();
        let space = space.to_string();
        tokio::spawn(async move {
            match graph.try_acquire_space(&space).await {
                Ok(guard) => {
                    let _ = graph.build_graph(&pages_for_graph, &space, guard).await;
                }
                Err(_) => {
                    tracing::warn!("graph build already running for space {}", space);
                }
            }
        });
    }

    Ok(Json(CompileResponse {
        pages: pages.into_iter().map(|p| CompiledPage {
            slug: p.path.clone(),
            title: p.title,
            summary: p.summary,
        }).collect(),
        skipped,
    }))
}

fn dispatch_sources(sources: &[(String, String)]) -> Vec<(String, String)> {
    // MVP: all text sources → passthrough
    // Future: .md/.txt → passthrough, URL → extractor, PDF → pdf-harness
    sources.to_vec()
}
```

- [ ] **Step 3: Wire DeepIntegrate trigger in review approve**

```rust
// crates/server/src/routes/review.rs — add after merge_to_main in "approve" branch:
// Fire DeepIntegrate graph build (team space)
let graph_builder = state.graph_builder.clone();
let space = ws_slug.clone();
// Gather page paths from submission
let page_slugs: Vec<String> = submission.page_slugs.clone();
let repo = state.repo_manager.get(&ws_slug).unwrap();
let pages: Vec<(String, String)> = page_slugs.iter()
    .filter_map(|slug| {
        let path = format!("wiki/{}.md", slug);
        repo.read_file("main", &path).ok().flatten().map(|c| {
            (path, String::from_utf8_lossy(&c).into_owned())
        })
    })
    .collect();

tokio::spawn(async move {
    match graph_builder.try_acquire_space(&space).await {
        Ok(guard) => {
            let _ = graph_builder.build_graph(&pages, &space, guard).await;
        }
        Err(_) => {
            tracing::warn!("graph build already running for space {}", space);
        }
    }
});
```

- [ ] **Step 4: Create SSE endpoint**

```rust
// crates/server/src/routes/compile.rs — add:
use axum::response::sse::{Event, Sse, KeepAlive};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;

pub async fn compile_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).unwrap_or_default();
                Some(Ok(Event::default()
                    .event(match &event {
                        CompileEvent::Phase { .. } => "phase",
                        CompileEvent::AgentStart { .. } => "agent-start",
                        CompileEvent::LlmRound { .. } => "llm-round",
                        CompileEvent::ToolCall { .. } => "tool-call",
                        CompileEvent::ToolResult { .. } => "tool-result",
                        CompileEvent::Entity { .. } => "entity",
                        CompileEvent::Fact { .. } => "fact",
                        CompileEvent::Error { .. } => "error",
                    })
                    .data(json)))
            }
            Err(_) => None,
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

- [ ] **Step 5: Add SSE route**

```rust
// crates/server/src/routes/compile.rs — add route fn:
pub fn compile_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/compile", axum::routing::post(compile_ws))
        .route("/api/compile/events", axum::routing::get(compile_events))
        .route("/api/compile/graph/rebuild", axum::routing::post(graph_rebuild))
}
```

- [ ] **Step 6: Create graph query routes**

```rust
// crates/server/src/routes/graph.rs
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Deserialize)]
pub struct EntityQuery {
    pub name: Option<String>,
    pub space: String,
    pub page_path: Option<String>,
}

pub async fn list_entities(
    State(state): State<Arc<AppState>>,
    Query(query): Query<EntityQuery>,
) -> Result<Json<Vec<cowiki_core::models::Entity>>, crate::error::AppError> {
    let entities = state.graph_store.list_entities(
        query.page_path.as_deref(),
        &query.space,
    ).await.map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(Json(entities))
}

pub async fn get_entity(
    State(state): State<Arc<AppState>>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<cowiki_core::models::Entity>, crate::error::AppError> {
    let entity = sqlx::query_as::<_, cowiki_core::models::Entity>(
        "SELECT id, name, type, aliases, description, embedding, space, is_orphaned, created_at, updated_at
         FROM entities WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?
    .ok_or_else(|| crate::error::AppError::NotFound("entity not found".into()))?;

    Ok(Json(entity))
}

pub async fn graph_rebuild(
    State(state): State<Arc<AppState>>,
    Json(input): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, crate::error::AppError> {
    let space = input["space"].as_str().unwrap_or("shared");
    let guard = state.graph_builder.try_acquire_space(space).await
        .map_err(|_| crate::error::AppError::Internal("graph build already running".into()))?;

    // Load all pages from space
    let repo = state.repo_manager.get(space).map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let pages = load_all_pages(&repo, space)?;

    tokio::spawn(async move {
        let _ = state.graph_builder.build_graph(&pages, space, guard).await;
    });

    Ok(Json(serde_json::json!({ "status": "started" })))
}
```

- [ ] **Step 7: Commit**

---

### Task 8: Deletion Cascade

**Files:**
- Modify: `crates/server/src/routes/pages.rs`

- [ ] **Step 1: Add three-store delete**

```rust
// crates/server/src/routes/pages.rs — modify delete_page:
pub async fn delete_page(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<DeletePageParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = extract_user(&state.db, &headers).await?;
    let branch = params.branch.as_deref().unwrap_or("main");
    let space = &params.space;
    let page_path = &params.path;

    // 1. Git: remove from repo
    state.repo.write_file(branch, page_path, &[], &format!("delete: {}", page_path), branch)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    // (actual git rm — depends on WikiRepo API)

    // 2. Vector DB: remove page metadata + embedding
    cowiki_db::pages::delete(&state.db, page_path, space)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 3. Knowledge Graph: cascade delete
    state.graph_store.delete_by_page(page_path, space)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 4. Compile State: update if needed
    // (remove from .cowiki/state.json source_pages mapping)

    Ok(Json(serde_json::json!({ "deleted": true, "path": page_path })))
}

#[derive(Deserialize)]
struct DeletePageParams {
    path: String,
    branch: Option<String>,
    space: String,
}
```

- [ ] **Step 2: Add orphan cleanup cron**

```rust
// Add periodic orphan cleanup (can be called by a cron route or background task):
pub async fn cleanup_orphans(graph_store: &dyn GraphStore, space: &str) {
    match graph_store.delete_orphan_entities(space, 7).await {
        Ok(count) => tracing::info!("cleaned {} orphan entities from space {}", count, space),
        Err(e) => tracing::error!("orphan cleanup failed: {}", e),
    }
}
```

- [ ] **Step 3: Commit**

---

### Task 9: Integration Tests

**Files:**
- Create: `crates/server/tests/compile_system_tests.rs`

- [ ] **Step 1: Write integration test for ShallowCompile**

```rust
// crates/server/tests/compile_system_tests.rs
#[tokio::test]
async fn test_shallow_compile_creates_pages() {
    // Setup: create test git repo with source files
    // Setup: start mock agent HTTP server
    // Call: POST /api/compile
    // Assert: wiki pages created in git repo
    // Assert: embeddings stored in pgvector
    // Assert: .cowiki/state.json updated with hashes
}

#[tokio::test]
async fn test_shallow_hash_skip_unchanged_sources() {
    // Setup: compile once
    // Call: compile again (same sources)
    // Assert: skipped = source count, pages = []
}

#[tokio::test]
async fn test_deep_personal_space_auto_trigger() {
    // Setup: compile in personal space
    // Assert: DeepIntegrate fires automatically
    // Assert: entities created in DB
}

#[tokio::test]
async fn test_deep_team_space_review_trigger() {
    // Setup: compile in team space
    // Assert: DeepIntegrate does NOT fire immediately
    // Review approve
    // Assert: DeepIntegrate fires after approve
}

#[tokio::test]
async fn test_deep_mutex_per_space() {
    // Setup: start DeepIntegrate for space A
    // Call: try to start another DeepIntegrate for space A
    // Assert: 409 Conflict
    // Call: start DeepIntegrate for space B
    // Assert: succeeds (different space)
}

#[tokio::test]
async fn test_delete_cascade_three_stores() {
    // Create page with embedding + graph data
    // Delete page
    // Assert: removed from git
    // Assert: removed from pgvector
    // Assert: facts + page_entities deleted
    // Assert: orphan entities flagged
}

#[tokio::test]
async fn test_sse_events_streamed() {
    // Setup: SSE subscription
    // Call: compile
    // Assert: receive phase:started, agent-start, tool-call, tool-result, phase:completed
}

#[tokio::test]
async fn test_agent_pool_tier_gating() {
    // Setup: free tier space
    // Configure pool with size 4
    // Assert: rejected (exceeds tier limit)
}
```

- [ ] **Step 2: Commit**

---

### Task 10: Final Wiring & Polish

**Files:**
- Modify: `crates/core/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Update crate exports**

```rust
// crates/core/src/lib.rs — ensure:
pub mod compiler;
pub mod client;
pub mod models;
pub mod ai;
pub mod git;
pub mod dedup;
```

- [ ] **Step 2: Update workspace to include agents**

```toml
# Cargo.toml (root)
[workspace]
members = [
    "crates/server",
    "crates/core",
    "crates/db",
    "crates/utils",
    "crates/extractor",
    "crates/agents",
]
```

- [ ] **Step 3: Run full test suite**

```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: two-phase compile system with agent pools and knowledge graph

- ShallowCompile (sync): SourceCompiler dispatches to remote agents via HTTP
- DeepIntegrate (async): GraphBuilder extracts entities + facts (per-space mutex)
- Agent pool: per-space configurable size, tier-gated (free/pro/enterprise)
- SSE events: full agent conversation observability
- GraphStore: PgGraphStore with entities, facts, page_entities tables
- ApiClient: shared HTTP client in crates/core/src/client/
- agents crate: protocol, registry, dispatch, events
- Deletion cascade: git + pgvector + knowledge graph

Refs: #15"
```

---

## Implementation Order

1. **Task 1** (DB tables) — foundation
2. **Task 2** (API client) — shared infrastructure
3. **Task 3** (Agent protocol) — agents crate
4. **Task 6** (Agent pool) — pool logic (needed by phases)
5. **Task 4** (ShallowCompile) — source compiler
6. **Task 5** (DeepIntegrate) — graph builder
7. **Task 7** (Routes + SSE) — wire everything
8. **Task 8** (Deletion) — consistency
9. **Task 9** (Tests) — validation
10. **Task 10** (Polish) — final wiring
