pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod git_http;
pub mod git_repo;
pub mod model;
pub mod pull_requests;
pub mod spaces;

use axum::Router;
use axum::routing::get;
use config::Config;
use git_repo::GitRepoStore;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub repos: GitRepoStore,
}

pub fn build_router(config: Config, pool: PgPool) -> Result<Router, git_repo::GitRepoError> {
    let repos = GitRepoStore::new(&config.repo_root)?;
    let state = AppState {
        config: Arc::new(config),
        pool,
        repos,
    };
    Ok(Router::new()
        .route("/healthz", get(healthz))
        .merge(auth::routes())
        .merge(spaces::routes())
        .merge(pull_requests::routes())
        .merge(git_http::routes())
        .with_state(state))
}

async fn healthz() -> axum::Json<Value> {
    axum::Json(json!({ "status": "ok" }))
}
