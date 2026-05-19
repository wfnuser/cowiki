# cowiki MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a collaborative wiki where humans and AI agents co-maintain knowledge with Git-based version control, personal/shared spaces, and review workflows.

**Architecture:** Rust (axum) backend manages a Git repo and PostgreSQL. React frontend provides wiki browsing, editing, and review UI. MCP server exposes tools for AI agents. OpenAI API powers compilation and search.

**Tech Stack:** Rust + axum, PostgreSQL + pgvector, Git (git2 crate), React + TypeScript + Vite + Tailwind, Milkdown editor, OpenAI API, Docker Compose

---

## File Structure

```
cowiki/
├── Cargo.toml                    # Rust workspace root
├── docker-compose.yml            # PostgreSQL + pgvector
├── .env.example                  # Environment variables template
├── crates/
│   ├── server/                   # axum HTTP server + routes
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs           # Entry point, router setup
│   │       ├── config.rs         # Env/config loading
│   │       ├── routes/
│   │       │   ├── mod.rs
│   │       │   ├── pages.rs      # Wiki page CRUD
│   │       │   ├── ingest.rs     # Source ingestion
│   │       │   ├── compile.rs    # Compilation trigger
│   │       │   ├── submit.rs     # Submission creation
│   │       │   ├── review.rs     # Review approve/reject
│   │       │   ├── search.rs     # Semantic search
│   │       │   └── auth.rs       # User auth
│   │       └── error.rs          # Error types
│   ├── core/                     # Business logic (no HTTP)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── git.rs            # Git repo operations
│   │       ├── compiler.rs       # LLM compilation pipeline
│   │       ├── dedup.rs          # Deduplication detection
│   │       ├── openai.rs         # OpenAI API client
│   │       └── models.rs         # Domain types
│   └── db/                       # Database layer
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── migrations/       # SQL migrations
│           │   └── 001_init.sql
│           ├── users.rs          # User queries
│           ├── pages.rs          # Page metadata queries
│           └── submissions.rs    # Submission queries
├── web/                          # React frontend
│   ├── package.json
│   ├── vite.config.ts
│   ├── index.html
│   ├── tailwind.config.ts
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── api.ts                # HTTP client
│       ├── components/
│       │   ├── Layout.tsx        # Shell with sidebar
│       │   ├── PageList.tsx      # Wiki page listing
│       │   ├── PageView.tsx      # Single page reader
│       │   ├── PageEditor.tsx    # Milkdown editor
│       │   ├── SubmitDialog.tsx  # Submit flow UI
│       │   ├── ReviewList.tsx    # Pending reviews
│       │   ├── ReviewDetail.tsx  # Diff view + approve/reject
│       │   ├── SearchBar.tsx     # Search input
│       │   └── IngestForm.tsx    # Ingest URL/file/text
│       └── pages/
│           ├── WikiPage.tsx      # Browse wiki
│           ├── PersonalPage.tsx  # Personal space
│           ├── ReviewPage.tsx    # Review submissions
│           └── SearchPage.tsx    # Search results
├── docs/                         # Already exists
│   ├── spec.md
│   ├── adr/
│   └── plans/
└── CONTEXT.md                    # Already exists
```

---

## Phase 1: Foundation (running skeleton)

### Task 1: Project scaffolding + Docker

**Files:**
- Create: `Cargo.toml`, `crates/server/Cargo.toml`, `crates/core/Cargo.toml`, `crates/db/Cargo.toml`
- Create: `docker-compose.yml`, `.env.example`, `.env`, `.gitignore`
- Create: `crates/server/src/main.rs`, `crates/core/src/lib.rs`, `crates/db/src/lib.rs`

- [ ] **Step 1: Create Cargo workspace**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = ["crates/server", "crates/core", "crates/db"]
```

```toml
# crates/server/Cargo.toml
[package]
name = "cowiki-server"
version = "0.1.0"
edition = "2021"

[dependencies]
cowiki-core = { path = "../core" }
cowiki-db = { path = "../db" }
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tower-http = { version = "0.6", features = ["cors", "fs"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
dotenvy = "0.15"
```

```toml
# crates/core/Cargo.toml
[package]
name = "cowiki-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
git2 = "0.20"
reqwest = { version = "0.12", features = ["json"] }
thiserror = "2"
sha2 = "0.10"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
```

```toml
# crates/db/Cargo.toml
[package]
name = "cowiki-db"
version = "0.1.0"
edition = "2021"

[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
pgvector = { version = "0.4", features = ["sqlx"] }
tracing = "0.1"
```

- [ ] **Step 2: Create Docker Compose**

```yaml
# docker-compose.yml
services:
  postgres:
    image: pgvector/pgvector:pg17
    environment:
      POSTGRES_DB: cowiki
      POSTGRES_USER: cowiki
      POSTGRES_PASSWORD: cowiki
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
```

- [ ] **Step 3: Create .env and .gitignore**

```bash
# .env.example
DATABASE_URL=postgres://cowiki:cowiki@localhost:5432/cowiki
OPENAI_API_KEY=sk-...
OPENAI_BASE_URL=https://api.openai.com/v1
COWIKI_DATA_DIR=./data
COWIKI_PORT=3000
```

```gitignore
# .gitignore
target/
node_modules/
.env
data/
web/dist/
```

- [ ] **Step 4: Create minimal server entry point**

```rust
// crates/server/src/main.rs
use axum::{routing::get, Router};
use tower_http::cors::CorsLayer;

mod config;
mod error;
mod routes;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .layer(CorsLayer::permissive());

    let port = std::env::var("COWIKI_PORT").unwrap_or_else(|_| "3000".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    tracing::info!("cowiki server listening on port {port}");
    axum::serve(listener, app).await.unwrap();
}
```

```rust
// crates/server/src/config.rs
pub struct Config {
    pub database_url: String,
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub data_dir: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL required"),
            openai_api_key: std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required"),
            openai_base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            data_dir: std::env::var("COWIKI_DATA_DIR").unwrap_or_else(|_| "./data".into()),
            port: std::env::var("COWIKI_PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .unwrap_or(3000),
        }
    }
}
```

```rust
// crates/server/src/error.rs
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug)]
pub enum AppError {
    Internal(String),
    NotFound(String),
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };
        (status, msg).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl From<git2::Error> for AppError {
    fn from(e: git2::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
```

```rust
// crates/server/src/routes/mod.rs
pub mod pages;
```

```rust
// crates/core/src/lib.rs
pub mod git;
pub mod models;
pub mod compiler;
pub mod dedup;
pub mod openai;
```

```rust
// crates/db/src/lib.rs
pub mod users;
pub mod pages;
pub mod submissions;
```

- [ ] **Step 5: Start Docker, verify build**

Run: `docker compose up -d && cargo build`
Expected: PostgreSQL running on 5432, Rust builds with no errors.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: project scaffolding — Rust workspace, Docker, PostgreSQL"
```

---

### Task 2: Database schema + migrations

**Files:**
- Create: `crates/db/src/migrations/001_init.sql`
- Modify: `crates/db/src/lib.rs`

- [ ] **Step 1: Write migration SQL**

```sql
-- crates/db/src/migrations/001_init.sql
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL UNIQUE,
    email TEXT,
    api_key TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE pages (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    branch TEXT NOT NULL DEFAULT 'main',
    embedding vector(1536),
    content_hash TEXT NOT NULL DEFAULT '',
    created_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(slug, branch)
);

CREATE INDEX pages_embedding_idx ON pages
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

CREATE TABLE submissions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected', 'changes_requested')),
    summary TEXT NOT NULL DEFAULT '',
    page_slugs TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_by UUID REFERENCES users(id),
    reviewed_at TIMESTAMPTZ
);
```

- [ ] **Step 2: Add migration runner to db crate**

```rust
// crates/db/src/lib.rs
use sqlx::PgPool;

pub mod users;
pub mod pages;
pub mod submissions;

pub async fn create_pool(database_url: &str) -> sqlx::Result<PgPool> {
    PgPool::connect(database_url).await
}

pub async fn run_migrations(pool: &PgPool) -> sqlx::Result<()> {
    let sql = include_str!("migrations/001_init.sql");
    sqlx::raw_sql(sql).execute(pool).await?;
    Ok(())
}
```

- [ ] **Step 3: Wire DB into server main.rs**

```rust
// crates/server/src/main.rs
use axum::{routing::get, Router};
use tower_http::cors::CorsLayer;
use std::sync::Arc;

mod config;
mod error;
mod routes;

pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: config::Config,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let config = config::Config::from_env();
    let db = cowiki_db::create_pool(&config.database_url)
        .await
        .expect("failed to connect to database");
    cowiki_db::run_migrations(&db)
        .await
        .expect("failed to run migrations");

    let state = Arc::new(AppState { db, config });

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    tracing::info!("cowiki server listening on :3000");
    axum::serve(listener, app).await.unwrap();
}
```

- [ ] **Step 4: Test migration runs**

Run: `cargo run`
Expected: Server starts, logs "cowiki server listening on :3000", migrations applied.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: database schema — users, pages, submissions with pgvector"
```

---

### Task 3: Git repo management

**Files:**
- Create: `crates/core/src/git.rs`
- Create: `crates/core/src/models.rs`

- [ ] **Step 1: Implement Git repo wrapper**

```rust
// crates/core/src/git.rs
use git2::{Repository, Signature, BranchType};
use std::path::{Path, PathBuf};
use std::fs;

pub struct WikiRepo {
    path: PathBuf,
}

impl WikiRepo {
    /// Initialize or open the wiki data repo.
    pub fn open_or_init(data_dir: &str) -> Result<Self, git2::Error> {
        let path = PathBuf::from(data_dir).join("repo");
        let repo = if path.exists() {
            Repository::open(&path)?
        } else {
            let repo = Repository::init(&path)?;
            // Create initial commit on main
            let sig = Signature::now("cowiki", "cowiki@local")?;
            let tree_id = repo.index()?.write_tree()?;
            let tree = repo.find_tree(tree_id)?;
            repo.commit(Some("HEAD"), &sig, &sig, "init: empty wiki", &tree, &[])?;

            // Create wiki/ and sources/ directories
            fs::create_dir_all(path.join("wiki")).ok();
            fs::create_dir_all(path.join("sources")).ok();
            repo
        };
        drop(repo);
        Ok(Self { path })
    }

    fn repo(&self) -> Result<Repository, git2::Error> {
        Repository::open(&self.path)
    }

    /// Ensure a user branch exists (branched from main).
    pub fn ensure_user_branch(&self, user_id: &str) -> Result<String, git2::Error> {
        let branch_name = format!("user/{user_id}");
        let repo = self.repo()?;

        if repo.find_branch(&branch_name, BranchType::Local).is_ok() {
            return Ok(branch_name);
        }

        let main = repo.find_branch("main", BranchType::Local)
            .or_else(|_| repo.find_branch("master", BranchType::Local))?;
        let commit = main.get().peel_to_commit()?;
        repo.branch(&branch_name, &commit, false)?;
        Ok(branch_name)
    }

    /// Write a file to a branch and commit.
    pub fn write_file(
        &self,
        branch: &str,
        file_path: &str,
        content: &[u8],
        message: &str,
        author: &str,
    ) -> Result<(), git2::Error> {
        let repo = self.repo()?;

        // Checkout branch
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let commit = branch_ref.get().peel_to_commit()?;

        // Write file to disk
        let full_path = self.path.join(file_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&full_path, content).map_err(|e| {
            git2::Error::from_str(&format!("failed to write file: {e}"))
        })?;

        // Stage and commit
        let mut index = repo.index()?;
        index.add_path(Path::new(file_path))?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = Signature::now(author, &format!("{author}@cowiki"))?;
        repo.commit(
            Some(&format!("refs/heads/{branch}")),
            &sig, &sig, message, &tree, &[&commit],
        )?;
        Ok(())
    }

    /// Read a file from a branch.
    pub fn read_file(&self, branch: &str, file_path: &str) -> Result<Option<Vec<u8>>, git2::Error> {
        let repo = self.repo()?;
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let commit = branch_ref.get().peel_to_commit()?;
        let tree = commit.tree()?;
        match tree.get_path(Path::new(file_path)) {
            Ok(entry) => {
                let blob = repo.find_blob(entry.id())?;
                Ok(Some(blob.content().to_vec()))
            }
            Err(_) => Ok(None),
        }
    }

    /// List files in a directory on a branch.
    pub fn list_files(&self, branch: &str, dir: &str) -> Result<Vec<String>, git2::Error> {
        let repo = self.repo()?;
        let branch_ref = repo.find_branch(branch, BranchType::Local)?;
        let commit = branch_ref.get().peel_to_commit()?;
        let tree = commit.tree()?;

        let subtree = if dir.is_empty() {
            tree
        } else {
            let entry = tree.get_path(Path::new(dir))?;
            repo.find_tree(entry.id())?
        };

        let mut files = Vec::new();
        for entry in subtree.iter() {
            if let Some(name) = entry.name() {
                if name.ends_with(".md") {
                    files.push(format!("{dir}/{name}").trim_start_matches('/').to_string());
                }
            }
        }
        Ok(files)
    }

    /// Get diff between a user branch and main for specific files.
    pub fn diff_files(&self, branch: &str, slugs: &[String]) -> Result<Vec<FileDiff>, git2::Error> {
        let repo = self.repo()?;
        let main_commit = repo.find_branch("main", BranchType::Local)?
            .get().peel_to_commit()?;
        let branch_commit = repo.find_branch(branch, BranchType::Local)?
            .get().peel_to_commit()?;

        let mut diffs = Vec::new();
        for slug in slugs {
            let path = format!("wiki/{slug}.md");
            let main_content = self.read_file("main", &path)?;
            let branch_content = self.read_file(branch, &path)?;

            diffs.push(FileDiff {
                path: path.clone(),
                old_content: main_content.map(|b| String::from_utf8_lossy(&b).into_owned()),
                new_content: branch_content.map(|b| String::from_utf8_lossy(&b).into_owned()),
            });
        }
        Ok(diffs)
    }

    /// Merge specific files from a branch into main.
    pub fn merge_to_main(
        &self,
        branch: &str,
        file_paths: &[String],
        author: &str,
        message: &str,
    ) -> Result<(), git2::Error> {
        for path in file_paths {
            if let Some(content) = self.read_file(branch, path)? {
                self.write_file("main", path, &content, message, author)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
}

impl FileDiff {
    pub fn is_new(&self) -> bool {
        self.old_content.is_none() && self.new_content.is_some()
    }

    pub fn is_modified(&self) -> bool {
        self.old_content.is_some() && self.new_content.is_some()
    }
}
```

- [ ] **Step 2: Create domain models**

```rust
// crates/core/src/models.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub sources: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub filename: String,
    pub content: String,
    pub content_hash: String,
    pub ingested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionSummary {
    pub description: String,
    pub new_pages: Vec<String>,
    pub updated_pages: Vec<String>,
    pub duplicates: Vec<DuplicateWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateWarning {
    pub new_slug: String,
    pub existing_slug: String,
    pub similarity: f64,
}
```

- [ ] **Step 3: Verify build**

Run: `cargo build`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: Git repo management — branch, read, write, diff, merge"
```

---

### Task 4: Page CRUD API routes

**Files:**
- Create: `crates/server/src/routes/pages.rs`
- Create: `crates/db/src/users.rs`, `crates/db/src/pages.rs`, `crates/db/src/submissions.rs`
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Implement DB query modules**

```rust
// crates/db/src/users.rs
use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub api_key: String,
}

pub async fn find_by_api_key(pool: &PgPool, api_key: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT id, name, email, api_key FROM users WHERE api_key = $1")
        .bind(api_key)
        .fetch_optional(pool)
        .await
}

pub async fn create(pool: &PgPool, name: &str, email: Option<&str>) -> sqlx::Result<User> {
    let api_key = format!("cw_{}", Uuid::new_v4().to_string().replace('-', ""));
    sqlx::query_as::<_, User>(
        "INSERT INTO users (name, email, api_key) VALUES ($1, $2, $3) RETURNING id, name, email, api_key"
    )
    .bind(name)
    .bind(email)
    .bind(&api_key)
    .fetch_one(pool)
    .await
}
```

```rust
// crates/db/src/pages.rs
use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use pgvector::Vector;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageMeta {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub branch: String,
    pub content_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn upsert(
    pool: &PgPool,
    slug: &str,
    title: &str,
    summary: &str,
    branch: &str,
    content_hash: &str,
    embedding: Option<&[f32]>,
    user_id: Uuid,
) -> sqlx::Result<PageMeta> {
    let emb = embedding.map(|e| Vector::from(e.to_vec()));
    sqlx::query_as::<_, PageMeta>(
        r#"INSERT INTO pages (slug, title, summary, branch, content_hash, embedding, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (slug, branch) DO UPDATE SET
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            content_hash = EXCLUDED.content_hash,
            embedding = EXCLUDED.embedding,
            updated_at = now()
        RETURNING id, slug, title, summary, branch, content_hash, created_at, updated_at"#
    )
    .bind(slug).bind(title).bind(summary).bind(branch)
    .bind(content_hash).bind(emb).bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn list_by_branch(pool: &PgPool, branch: &str) -> sqlx::Result<Vec<PageMeta>> {
    sqlx::query_as::<_, PageMeta>(
        "SELECT id, slug, title, summary, branch, content_hash, created_at, updated_at FROM pages WHERE branch = $1 ORDER BY updated_at DESC"
    )
    .bind(branch)
    .fetch_all(pool)
    .await
}

pub async fn find_similar(pool: &PgPool, embedding: &[f32], branch: &str, limit: i64, threshold: f64) -> sqlx::Result<Vec<(PageMeta, f64)>> {
    let emb = Vector::from(embedding.to_vec());
    let rows = sqlx::query_as::<_, (Uuid, String, String, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, f64)>(
        r#"SELECT id, slug, title, summary, branch, content_hash, created_at, updated_at,
            1 - (embedding <=> $1) as similarity
        FROM pages WHERE branch = $2 AND embedding IS NOT NULL
        HAVING 1 - (embedding <=> $1) > $3
        ORDER BY similarity DESC LIMIT $4"#
    )
    .bind(&emb).bind(branch).bind(threshold).bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| {
        (PageMeta { id: r.0, slug: r.1, title: r.2, summary: r.3, branch: r.4, content_hash: r.5, created_at: r.6, updated_at: r.7 }, r.8)
    }).collect())
}
```

```rust
// crates/db/src/submissions.rs
use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Submission {
    pub id: Uuid,
    pub user_id: Uuid,
    pub status: String,
    pub summary: String,
    pub page_slugs: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn create(pool: &PgPool, user_id: Uuid, summary: &str, page_slugs: &[String]) -> sqlx::Result<Submission> {
    sqlx::query_as::<_, Submission>(
        "INSERT INTO submissions (user_id, summary, page_slugs) VALUES ($1, $2, $3) RETURNING *"
    )
    .bind(user_id).bind(summary).bind(page_slugs)
    .fetch_one(pool)
    .await
}

pub async fn list_pending(pool: &PgPool) -> sqlx::Result<Vec<Submission>> {
    sqlx::query_as::<_, Submission>("SELECT * FROM submissions WHERE status = 'pending' ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
}

pub async fn update_status(pool: &PgPool, id: Uuid, status: &str, reviewer_id: Uuid) -> sqlx::Result<Submission> {
    sqlx::query_as::<_, Submission>(
        "UPDATE submissions SET status = $2, reviewed_by = $3, reviewed_at = now() WHERE id = $1 RETURNING *"
    )
    .bind(id).bind(status).bind(reviewer_id)
    .fetch_one(pool)
    .await
}
```

- [ ] **Step 2: Implement pages routes**

```rust
// crates/server/src/routes/pages.rs
use axum::{extract::{Path, Query, State}, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::{AppState, error::{AppError, Result}};

#[derive(Deserialize)]
pub struct ListParams {
    pub branch: Option<String>,
}

#[derive(Serialize)]
pub struct PageResponse {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub branch: String,
}

pub async fn list_pages(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<cowiki_db::pages::PageMeta>>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());
    let pages = cowiki_db::pages::list_by_branch(&state.db, &branch).await?;
    Ok(Json(pages))
}

pub async fn get_page(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Query(params): Query<ListParams>,
) -> Result<Json<PageResponse>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());
    let path = format!("wiki/{slug}.md");
    let content = state.wiki_repo.read_file(&branch, &path)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("page {slug} not found")))?;
    let body = String::from_utf8_lossy(&content).into_owned();

    // Parse frontmatter
    let (title, summary) = parse_frontmatter(&body);

    Ok(Json(PageResponse { slug, title, summary, body, branch }))
}

fn parse_frontmatter(content: &str) -> (String, String) {
    if !content.starts_with("---") {
        return ("Untitled".into(), "".into());
    }
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return ("Untitled".into(), "".into());
    }
    let fm = parts[1];
    let title = fm.lines()
        .find(|l| l.starts_with("title:"))
        .map(|l| l.trim_start_matches("title:").trim().trim_matches('"').to_string())
        .unwrap_or_else(|| "Untitled".into());
    let summary = fm.lines()
        .find(|l| l.starts_with("summary:"))
        .map(|l| l.trim_start_matches("summary:").trim().trim_matches('"').to_string())
        .unwrap_or_default();
    (title, summary)
}

#[derive(Deserialize)]
pub struct WritePage {
    pub slug: String,
    pub body: String,
    pub branch: String,
}

pub async fn write_page(
    State(state): State<Arc<AppState>>,
    Json(input): Json<WritePage>,
) -> Result<Json<serde_json::Value>> {
    let path = format!("wiki/{}.md", input.slug);
    state.wiki_repo.write_file(
        &input.branch, &path, input.body.as_bytes(),
        &format!("edit: {}", input.slug), &input.branch,
    ).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({"ok": true, "slug": input.slug})))
}
```

- [ ] **Step 3: Wire routes and wiki_repo into AppState**

Update `crates/server/src/main.rs` to add `wiki_repo: cowiki_core::git::WikiRepo` to `AppState` and mount routes:

```rust
// In main.rs — add to AppState:
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: config::Config,
    pub wiki_repo: cowiki_core::git::WikiRepo,
}

// In main() — init repo and add routes:
let wiki_repo = cowiki_core::git::WikiRepo::open_or_init(&config.data_dir)
    .expect("failed to init wiki repo");

let app = Router::new()
    .route("/api/health", get(|| async { "ok" }))
    .route("/api/pages", get(routes::pages::list_pages))
    .route("/api/pages/{slug}", get(routes::pages::get_page))
    .route("/api/pages", post(routes::pages::write_page))
    .layer(CorsLayer::permissive())
    .with_state(state);
```

- [ ] **Step 4: Test endpoints**

Run: `cargo run &` then:
```bash
curl http://localhost:3000/api/health
curl http://localhost:3000/api/pages?branch=main
```
Expected: `ok` and `[]`

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: page CRUD API — list, get, write with Git backend"
```

---

### Task 5: React frontend skeleton

**Files:**
- Create: `web/package.json`, `web/vite.config.ts`, `web/index.html`, `web/tsconfig.json`
- Create: `web/src/main.tsx`, `web/src/App.tsx`, `web/src/api.ts`
- Create: `web/src/components/Layout.tsx`, `web/src/components/PageList.tsx`, `web/src/components/PageView.tsx`

- [ ] **Step 1: Initialize React project**

```bash
cd web
npm create vite@latest . -- --template react-ts
npm install
npm install @tailwindcss/vite tailwindcss react-router-dom
npm install -D @types/react-router-dom
```

- [ ] **Step 2: Configure Vite proxy**

```typescript
// web/vite.config.ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      '/api': 'http://localhost:3000'
    }
  }
})
```

- [ ] **Step 3: Create API client**

```typescript
// web/src/api.ts
const BASE = '/api';

export interface PageMeta {
  slug: string;
  title: string;
  summary: string;
  branch: string;
  updated_at: string;
}

export interface PageFull extends PageMeta {
  body: string;
}

export async function listPages(branch = 'main'): Promise<PageMeta[]> {
  const res = await fetch(`${BASE}/pages?branch=${branch}`);
  return res.json();
}

export async function getPage(slug: string, branch = 'main'): Promise<PageFull> {
  const res = await fetch(`${BASE}/pages/${slug}?branch=${branch}`);
  return res.json();
}

export async function writePage(slug: string, body: string, branch: string): Promise<void> {
  await fetch(`${BASE}/pages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ slug, body, branch }),
  });
}
```

- [ ] **Step 4: Create Layout component**

```tsx
// web/src/components/Layout.tsx
import { Link, Outlet } from 'react-router-dom';

export function Layout() {
  return (
    <div className="min-h-screen bg-stone-50 text-stone-900">
      <header className="border-b border-stone-200 bg-white">
        <div className="mx-auto max-w-7xl px-6 py-4 flex items-center justify-between">
          <Link to="/" className="text-xl font-bold tracking-tight">cowiki</Link>
          <nav className="flex gap-6 text-sm">
            <Link to="/" className="hover:text-stone-600">Shared Wiki</Link>
            <Link to="/personal" className="hover:text-stone-600">My Space</Link>
            <Link to="/reviews" className="hover:text-stone-600">Reviews</Link>
          </nav>
        </div>
      </header>
      <main className="mx-auto max-w-7xl px-6 py-8">
        <Outlet />
      </main>
    </div>
  );
}
```

- [ ] **Step 5: Create PageList and App with routing**

```tsx
// web/src/components/PageList.tsx
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { listPages, type PageMeta } from '../api';

export function PageList({ branch = 'main' }: { branch?: string }) {
  const [pages, setPages] = useState<PageMeta[]>([]);

  useEffect(() => {
    listPages(branch).then(setPages);
  }, [branch]);

  if (pages.length === 0) {
    return (
      <div className="text-center py-20 text-stone-400">
        <p className="text-lg">No pages yet</p>
        <p className="text-sm mt-2">Ingest a source or write a page to get started.</p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {pages.map((p) => (
        <Link
          key={p.slug}
          to={`/page/${p.slug}?branch=${branch}`}
          className="block rounded-lg border border-stone-200 bg-white p-4 hover:border-stone-300 transition"
        >
          <h3 className="font-semibold">{p.title}</h3>
          {p.summary && <p className="text-sm text-stone-500 mt-1">{p.summary}</p>}
        </Link>
      ))}
    </div>
  );
}
```

```tsx
// web/src/App.tsx
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { Layout } from './components/Layout';
import { PageList } from './components/PageList';

function SharedWiki() {
  return (
    <div>
      <h1 className="text-2xl font-bold mb-6">Shared Wiki</h1>
      <PageList branch="main" />
    </div>
  );
}

function PersonalSpace() {
  return (
    <div>
      <h1 className="text-2xl font-bold mb-6">My Space</h1>
      <PageList branch="user/default" />
    </div>
  );
}

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<SharedWiki />} />
          <Route path="/personal" element={<PersonalSpace />} />
          <Route path="/reviews" element={<div>Reviews (coming soon)</div>} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
```

```tsx
// web/src/main.tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

```css
/* web/src/index.css */
@import "tailwindcss";
```

- [ ] **Step 6: Run frontend**

Run: `cd web && npm run dev`
Expected: Browser opens at localhost:5173, shows "cowiki" header with "Shared Wiki" / "My Space" / "Reviews" nav, empty page list.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: React frontend skeleton — layout, routing, page list"
```

---

## Phase 2: Core Flows

### Task 6: Ingest API

**Files:**
- Create: `crates/server/src/routes/ingest.rs`
- Modify: `crates/server/src/routes/mod.rs`, `crates/server/src/main.rs`

- [ ] **Step 1: Implement ingest route**

```rust
// crates/server/src/routes/ingest.rs
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::sync::Arc;
use crate::{AppState, error::Result};

#[derive(Deserialize)]
pub struct IngestRequest {
    /// "url", "text", or "file"
    pub source_type: String,
    pub content: String,
    pub filename: Option<String>,
    pub branch: String,
}

#[derive(Serialize)]
pub struct IngestResponse {
    pub filename: String,
    pub content_hash: String,
}

pub async fn ingest(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IngestRequest>,
) -> Result<Json<IngestResponse>> {
    let content = match input.source_type.as_str() {
        "url" => fetch_url(&input.content).await?,
        "text" | "file" => input.content.clone(),
        _ => return Err(crate::error::AppError::BadRequest("invalid source_type".into())),
    };

    let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    let filename = input.filename.unwrap_or_else(|| {
        let short_hash = &hash[..8];
        format!("source-{short_hash}.md")
    });

    let path = format!("sources/{filename}");
    state.wiki_repo.write_file(
        &input.branch, &path, content.as_bytes(),
        &format!("ingest: {filename}"), &input.branch,
    ).map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    Ok(Json(IngestResponse { filename, content_hash: hash }))
}

async fn fetch_url(url: &str) -> Result<String> {
    let resp = reqwest::get(url).await
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let text = resp.text().await
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(text)
}
```

- [ ] **Step 2: Mount route**

Add to `routes/mod.rs`: `pub mod ingest;`
Add to `main.rs` router: `.route("/api/ingest", post(routes::ingest::ingest))`

- [ ] **Step 3: Test**

```bash
curl -X POST http://localhost:3000/api/ingest \
  -H 'Content-Type: application/json' \
  -d '{"source_type":"text","content":"# Docker Tips\n\nAlways use bridge network...","branch":"user/default"}'
```
Expected: `{"filename":"source-xxxxxxxx.md","content_hash":"..."}`

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: ingest API — URL, text, file sources to personal branch"
```

---

### Task 7: Compile API (OpenAI integration)

**Files:**
- Create: `crates/core/src/openai.rs`
- Create: `crates/core/src/compiler.rs`
- Create: `crates/server/src/routes/compile.rs`

- [ ] **Step 1: Implement OpenAI client**

```rust
// crates/core/src/openai.rs
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAIClient {
    client: Client,
    api_key: String,
    base_url: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f64,
}

#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAIClient {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn chat(&self, system: &str, user: &str) -> Result<String, reqwest::Error> {
        let resp: ChatResponse = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&ChatRequest {
                model: "gpt-4o-mini".into(),
                messages: vec![
                    Message { role: "system".into(), content: system.into() },
                    Message { role: "user".into(), content: user.into() },
                ],
                temperature: 0.3,
            })
            .send().await?
            .json().await?;
        Ok(resp.choices.first().map(|c| c.message.content.clone()).unwrap_or_default())
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, reqwest::Error> {
        let resp: EmbeddingResponse = self.client
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({
                "model": "text-embedding-3-small",
                "input": text,
            }))
            .send().await?
            .json().await?;
        Ok(resp.data.first().map(|d| d.embedding.clone()).unwrap_or_default())
    }
}
```

- [ ] **Step 2: Implement compiler**

```rust
// crates/core/src/compiler.rs
use crate::openai::OpenAIClient;
use crate::models::Page;

pub struct Compiler {
    openai: OpenAIClient,
}

impl Compiler {
    pub fn new(openai: OpenAIClient) -> Self {
        Self { openai }
    }

    /// Compile a set of sources into wiki pages.
    pub async fn compile(&self, sources: &[(String, String)]) -> Result<Vec<Page>, String> {
        let combined = sources.iter()
            .map(|(name, content)| format!("## Source: {name}\n\n{content}"))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let system = r#"You are a knowledge compiler. Given source documents, extract distinct concepts and produce wiki pages.

For each concept, output a markdown document with YAML frontmatter:

```
---
title: "Concept Title"
summary: "One-line summary"
sources:
  - source-filename.md
---

Content here with clear explanations.
```

Separate multiple pages with `===PAGE_BREAK===`.

Be concise. One concept per page. Use clear headings. Attribute claims to sources with `^[filename.md]`."#;

        let user = format!("Compile the following sources into wiki pages:\n\n{combined}");
        let result = self.openai.chat(system, &user).await
            .map_err(|e| e.to_string())?;

        let pages = result.split("===PAGE_BREAK===")
            .filter(|s| !s.trim().is_empty())
            .map(|raw| parse_compiled_page(raw.trim()))
            .collect();

        Ok(pages)
    }

    pub async fn generate_summary(&self, content: &str) -> Result<String, String> {
        self.openai.chat(
            "Generate a one-line summary (max 100 chars) of this wiki page. Return only the summary, nothing else.",
            content,
        ).await.map_err(|e| e.to_string())
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        self.openai.embed(text).await.map_err(|e| e.to_string())
    }
}

fn parse_compiled_page(raw: &str) -> Page {
    let mut title = "Untitled".to_string();
    let mut summary = String::new();
    let mut sources = Vec::new();
    let mut body = raw.to_string();

    if raw.starts_with("---") {
        let parts: Vec<&str> = raw.splitn(3, "---").collect();
        if parts.len() >= 3 {
            let fm = parts[1];
            body = parts[2].trim().to_string();
            for line in fm.lines() {
                let line = line.trim();
                if line.starts_with("title:") {
                    title = line.trim_start_matches("title:").trim().trim_matches('"').to_string();
                } else if line.starts_with("summary:") {
                    summary = line.trim_start_matches("summary:").trim().trim_matches('"').to_string();
                } else if line.starts_with("- ") && !line.contains(':') {
                    sources.push(line.trim_start_matches("- ").trim().to_string());
                }
            }
        }
    }

    let slug = title.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .replace(' ', "-");

    Page {
        slug,
        title,
        summary,
        body,
        sources,
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}
```

- [ ] **Step 3: Implement compile route**

```rust
// crates/server/src/routes/compile.rs
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::{AppState, error::Result};

#[derive(Deserialize)]
pub struct CompileRequest {
    pub branch: String,
}

#[derive(Serialize)]
pub struct CompileResponse {
    pub pages: Vec<CompiledPage>,
}

#[derive(Serialize)]
pub struct CompiledPage {
    pub slug: String,
    pub title: String,
    pub summary: String,
}

pub async fn compile(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CompileRequest>,
) -> Result<Json<CompileResponse>> {
    // 1. List sources on the branch
    let source_files = state.wiki_repo.list_files(&input.branch, "sources")
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    if source_files.is_empty() {
        return Ok(Json(CompileResponse { pages: vec![] }));
    }

    // 2. Read all sources
    let mut sources = Vec::new();
    for file in &source_files {
        if let Some(content) = state.wiki_repo.read_file(&input.branch, file)
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))? {
            let text = String::from_utf8_lossy(&content).into_owned();
            let name = file.rsplit('/').next().unwrap_or(file);
            sources.push((name.to_string(), text));
        }
    }

    // 3. Compile via LLM
    let compiled = state.compiler.compile(&sources).await
        .map_err(|e| crate::error::AppError::Internal(e))?;

    // 4. Write pages to branch and index in DB
    let mut result_pages = Vec::new();
    for page in &compiled {
        let full_content = format!(
            "---\ntitle: \"{}\"\nsummary: \"{}\"\nsources:\n{}\n---\n\n{}",
            page.title, page.summary,
            page.sources.iter().map(|s| format!("  - {s}")).collect::<Vec<_>>().join("\n"),
            page.body,
        );

        let path = format!("wiki/{}.md", page.slug);
        state.wiki_repo.write_file(
            &input.branch, &path, full_content.as_bytes(),
            &format!("compile: {}", page.title), &input.branch,
        ).map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

        // Generate embedding and save to DB
        if let Ok(emb) = state.compiler.embed(&format!("{}\n{}", page.title, page.summary)).await {
            let hash = format!("{:x}", sha2::Sha256::digest(full_content.as_bytes()));
            cowiki_db::pages::upsert(
                &state.db, &page.slug, &page.title, &page.summary,
                &input.branch, &hash, Some(&emb),
                uuid::Uuid::nil(), // TODO: get from auth
            ).await.ok();
        }

        result_pages.push(CompiledPage {
            slug: page.slug.clone(),
            title: page.title.clone(),
            summary: page.summary.clone(),
        });
    }

    Ok(Json(CompileResponse { pages: result_pages }))
}
```

- [ ] **Step 4: Add compiler to AppState and mount route**

In `main.rs`:
```rust
use cowiki_core::openai::OpenAIClient;
use cowiki_core::compiler::Compiler;

// In AppState:
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: config::Config,
    pub wiki_repo: cowiki_core::git::WikiRepo,
    pub compiler: Compiler,
}

// In main():
let openai = OpenAIClient::new(&config.openai_api_key, &config.openai_base_url);
let compiler = Compiler::new(openai);
// Add to state, mount .route("/api/compile", post(routes::compile::compile))
```

- [ ] **Step 5: Test compile flow**

```bash
# First ingest
curl -X POST http://localhost:3000/api/ingest \
  -H 'Content-Type: application/json' \
  -d '{"source_type":"text","content":"# Docker Networking\n\nDocker uses bridge networks by default. To connect containers, create a custom network:\n\n```\ndocker network create mynet\n```\n\nContainers on the same network can communicate by name.","branch":"user/default"}'

# Then compile
curl -X POST http://localhost:3000/api/compile \
  -H 'Content-Type: application/json' \
  -d '{"branch":"user/default"}'
```
Expected: Returns list of compiled pages with slugs and titles.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: compile pipeline — OpenAI integration, source → wiki page compilation"
```

---

### Task 8: Submit + Review API

**Files:**
- Create: `crates/server/src/routes/submit.rs`
- Create: `crates/server/src/routes/review.rs`
- Create: `crates/core/src/dedup.rs`

- [ ] **Step 1: Implement dedup detection**

```rust
// crates/core/src/dedup.rs
use crate::models::DuplicateWarning;

pub async fn check_duplicates(
    db: &sqlx::PgPool,
    embeddings: &[(String, Vec<f32>)],
    threshold: f64,
) -> Vec<DuplicateWarning> {
    let mut warnings = Vec::new();
    for (slug, emb) in embeddings {
        if let Ok(similar) = cowiki_db::pages::find_similar(db, emb, "main", 3, threshold).await {
            for (page, score) in similar {
                if page.slug != *slug {
                    warnings.push(DuplicateWarning {
                        new_slug: slug.clone(),
                        existing_slug: page.slug,
                        similarity: score,
                    });
                }
            }
        }
    }
    warnings
}
```

- [ ] **Step 2: Implement submit route**

```rust
// crates/server/src/routes/submit.rs
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::{AppState, error::Result};
use cowiki_core::models::DuplicateWarning;

#[derive(Deserialize)]
pub struct SubmitRequest {
    pub branch: String,
    pub page_slugs: Vec<String>,
}

#[derive(Serialize)]
pub struct SubmitResponse {
    pub submission_id: uuid::Uuid,
    pub summary: String,
    pub duplicates: Vec<DuplicateWarning>,
}

pub async fn submit(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SubmitRequest>,
) -> Result<Json<SubmitResponse>> {
    // 1. Get diffs
    let diffs = state.wiki_repo.diff_files(&input.branch, &input.page_slugs)
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    // 2. Generate embeddings for dedup
    let mut embeddings = Vec::new();
    for slug in &input.page_slugs {
        let path = format!("wiki/{slug}.md");
        if let Some(content) = state.wiki_repo.read_file(&input.branch, &path)
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))? {
            let text = String::from_utf8_lossy(&content);
            if let Ok(emb) = state.compiler.embed(&text).await {
                embeddings.push((slug.clone(), emb));
            }
        }
    }

    // 3. Check duplicates
    let duplicates = cowiki_core::dedup::check_duplicates(&state.db, &embeddings, 0.85).await;

    // 4. Generate summary via LLM
    let diff_desc = diffs.iter().map(|d| {
        if d.is_new() { format!("+ new: {}", d.path) }
        else { format!("~ modified: {}", d.path) }
    }).collect::<Vec<_>>().join("\n");

    let summary = state.compiler.generate_summary(&format!(
        "Submission with these changes:\n{diff_desc}"
    )).await.unwrap_or_else(|_| diff_desc);

    // 5. Create submission record
    let submission = cowiki_db::submissions::create(
        &state.db, uuid::Uuid::nil(), &summary, &input.page_slugs,
    ).await.map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    Ok(Json(SubmitResponse {
        submission_id: submission.id,
        summary,
        duplicates,
    }))
}
```

- [ ] **Step 3: Implement review routes**

```rust
// crates/server/src/routes/review.rs
use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::{AppState, error::{AppError, Result}};

pub async fn list_reviews(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<cowiki_db::submissions::Submission>>> {
    let subs = cowiki_db::submissions::list_pending(&state.db).await?;
    Ok(Json(subs))
}

#[derive(Serialize)]
pub struct ReviewDetail {
    pub submission: cowiki_db::submissions::Submission,
    pub diffs: Vec<cowiki_core::git::FileDiff>,
}

pub async fn get_review(
    State(state): State<Arc<AppState>>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<ReviewDetail>> {
    let subs = cowiki_db::submissions::list_pending(&state.db).await?;
    let submission = subs.into_iter().find(|s| s.id == id)
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;

    // Find user branch from the submission's user
    // For MVP, assume user/default
    let branch = "user/default";
    let diffs = state.wiki_repo.diff_files(branch, &submission.page_slugs)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ReviewDetail { submission, diffs }))
}

#[derive(Deserialize)]
pub struct ReviewAction {
    pub action: String, // "approve" or "reject"
}

pub async fn review_action(
    State(state): State<Arc<AppState>>,
    Path(id): Path<uuid::Uuid>,
    Json(input): Json<ReviewAction>,
) -> Result<Json<serde_json::Value>> {
    let subs = cowiki_db::submissions::list_pending(&state.db).await?;
    let submission = subs.into_iter().find(|s| s.id == id)
        .ok_or_else(|| AppError::NotFound("submission not found".into()))?;

    match input.action.as_str() {
        "approve" => {
            // Merge files to main
            let branch = "user/default";
            let file_paths: Vec<String> = submission.page_slugs.iter()
                .map(|s| format!("wiki/{s}.md"))
                .collect();
            state.wiki_repo.merge_to_main(
                branch, &file_paths, "reviewer",
                &format!("approve: {}", submission.summary),
            ).map_err(|e| AppError::Internal(e.to_string()))?;

            // Update page records to main branch
            for slug in &submission.page_slugs {
                let path = format!("wiki/{slug}.md");
                if let Some(content) = state.wiki_repo.read_file("main", &path)
                    .map_err(|e| AppError::Internal(e.to_string()))? {
                    let text = String::from_utf8_lossy(&content);
                    let hash = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
                    if let Ok(emb) = state.compiler.embed(&text).await {
                        cowiki_db::pages::upsert(
                            &state.db, slug, slug, "", "main", &hash, Some(&emb), uuid::Uuid::nil(),
                        ).await.ok();
                    }
                }
            }

            cowiki_db::submissions::update_status(&state.db, id, "approved", uuid::Uuid::nil()).await?;
        }
        "reject" => {
            cowiki_db::submissions::update_status(&state.db, id, "rejected", uuid::Uuid::nil()).await?;
        }
        _ => return Err(AppError::BadRequest("invalid action".into())),
    }

    Ok(Json(serde_json::json!({"ok": true})))
}
```

- [ ] **Step 4: Mount routes**

Add to `routes/mod.rs`: `pub mod submit; pub mod review;`
Add to router:
```rust
.route("/api/submit", post(routes::submit::submit))
.route("/api/reviews", get(routes::review::list_reviews))
.route("/api/reviews/{id}", get(routes::review::get_review))
.route("/api/reviews/{id}", post(routes::review::review_action))
```

- [ ] **Step 5: Test full flow**

```bash
# Submit
curl -X POST http://localhost:3000/api/submit \
  -H 'Content-Type: application/json' \
  -d '{"branch":"user/default","page_slugs":["docker-networking"]}'

# List reviews
curl http://localhost:3000/api/reviews

# Approve (use the submission_id from above)
curl -X POST http://localhost:3000/api/reviews/<id> \
  -H 'Content-Type: application/json' \
  -d '{"action":"approve"}'

# Verify page is now on main
curl http://localhost:3000/api/pages?branch=main
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: submit + review — dedup check, diff, approve/reject workflow"
```

---

### Task 9: Search API

**Files:**
- Create: `crates/server/src/routes/search.rs`

- [ ] **Step 1: Implement search route**

```rust
// crates/server/src/routes/search.rs
use axum::{extract::{Query, State}, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::{AppState, error::Result};

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub branch: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub similarity: f64,
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<SearchResult>>> {
    let branch = params.branch.unwrap_or_else(|| "main".into());
    let limit = params.limit.unwrap_or(10);

    let embedding = state.compiler.embed(&params.q).await
        .map_err(|e| crate::error::AppError::Internal(e))?;

    let results = cowiki_db::pages::find_similar(&state.db, &embedding, &branch, limit, 0.3).await
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    Ok(Json(results.into_iter().map(|(page, score)| SearchResult {
        slug: page.slug,
        title: page.title,
        summary: page.summary,
        similarity: score,
    }).collect()))
}
```

- [ ] **Step 2: Mount route**

Add `pub mod search;` and `.route("/api/search", get(routes::search::search))`

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: semantic search — pgvector cosine similarity"
```

---

## Phase 3: Frontend Completion

### Task 10: Frontend — ingest, compile, page editor, review UI

**Files:**
- Create: `web/src/components/IngestForm.tsx`
- Create: `web/src/components/PageEditor.tsx`
- Create: `web/src/components/PageView.tsx`
- Create: `web/src/components/ReviewList.tsx`
- Create: `web/src/components/ReviewDetail.tsx`
- Create: `web/src/components/SearchBar.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/api.ts`

This task is larger — implement all frontend components for the complete UI. Each component follows the same pattern: fetch from API, render with Tailwind, handle user actions.

- [ ] **Step 1: Extend API client**

Add to `web/src/api.ts`:

```typescript
export async function ingest(sourceType: string, content: string, branch: string, filename?: string) {
  const res = await fetch(`${BASE}/ingest`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source_type: sourceType, content, branch, filename }),
  });
  return res.json();
}

export async function compile(branch: string) {
  const res = await fetch(`${BASE}/compile`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ branch }),
  });
  return res.json();
}

export async function submit(branch: string, pageSlugs: string[]) {
  const res = await fetch(`${BASE}/submit`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ branch, page_slugs: pageSlugs }),
  });
  return res.json();
}

export async function listReviews() {
  const res = await fetch(`${BASE}/reviews`);
  return res.json();
}

export async function getReview(id: string) {
  const res = await fetch(`${BASE}/reviews/${id}`);
  return res.json();
}

export async function reviewAction(id: string, action: string) {
  const res = await fetch(`${BASE}/reviews/${id}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action }),
  });
  return res.json();
}

export async function search(q: string, branch = 'main') {
  const res = await fetch(`${BASE}/search?q=${encodeURIComponent(q)}&branch=${branch}`);
  return res.json();
}
```

- [ ] **Step 2: Build IngestForm, PageView, ReviewList, ReviewDetail, SearchBar components**

Each component is a standard React component using the API client and Tailwind styling. The design follows a clean, warm aesthetic similar to the screenshot (cream/stone background, clean typography, minimal chrome).

Build all components, wire up to App.tsx routes. Key pages:
- `/` — Shared Wiki (PageList + SearchBar)
- `/personal` — Personal Space (PageList + IngestForm + Compile button + Submit button)
- `/page/:slug` — Page viewer (markdown rendered)
- `/reviews` — Pending reviews list
- `/reviews/:id` — Review detail (diff view + approve/reject)

- [ ] **Step 3: Install Markdown renderer**

```bash
cd web && npm install react-markdown remark-gfm
```

- [ ] **Step 4: Test full flow in browser**

1. Open localhost:5173
2. Go to "My Space" → Ingest a URL or text
3. Click "Compile" → See generated pages
4. Select pages → "Submit to Shared"
5. Go to "Reviews" → See submission with diff
6. Approve → Page appears in "Shared Wiki"
7. Search for a concept → Results appear

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: complete frontend — ingest, compile, submit, review, search UI"
```

---

### Task 11: Serve frontend from Rust

**Files:**
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Build frontend and serve static files**

```bash
cd web && npm run build
```

In `main.rs`, serve `web/dist/` as static files with SPA fallback:

```rust
use tower_http::services::{ServeDir, ServeFile};

let spa = ServeDir::new("web/dist")
    .not_found_service(ServeFile::new("web/dist/index.html"));

let app = Router::new()
    .route("/api/health", get(|| async { "ok" }))
    // ... all API routes ...
    .fallback_service(spa)
    .layer(CorsLayer::permissive())
    .with_state(state);
```

- [ ] **Step 2: Test**

Run: `cargo run`
Open: `http://localhost:3000`
Expected: Full cowiki UI served from the Rust server.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: serve frontend from Rust — single binary deployment"
```

---

## Summary

| Phase | Tasks | What you get |
|-------|-------|-------------|
| **1: Foundation** | 1-5 | Running server + empty wiki UI |
| **2: Core Flows** | 6-9 | Ingest → Compile → Submit → Review → Search |
| **3: Frontend** | 10-11 | Complete web UI, single binary |

**Post-MVP (not in this plan):**
- MCP server for agent integration
- User authentication (API keys)
- Multiple users with real branches
- Wikilink resolution
- Knowledge graph visualization
