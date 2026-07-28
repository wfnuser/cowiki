pub mod auth;
pub mod config;
pub mod content;
pub mod db;
pub mod error;
pub mod git_http;
pub mod git_repo;
pub mod invitations;
pub mod model;
pub mod pull_requests;
pub mod spaces;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::routing::get;
use config::Config;
use git_repo::GitRepoStore;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;

const MAX_API_REQUEST_BYTES: usize = 1024 * 1024;
const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub repos: GitRepoStore,
}

pub fn build_router(config: Config, pool: PgPool) -> Result<Router, git_repo::GitRepoError> {
    let repos = GitRepoStore::new(&config.repo_root)?;
    let browser_origin =
        HeaderValue::from_str(&config.public_origin.origin().ascii_serialization())
            .map_err(|error| git_repo::GitRepoError::Git(error.to_string()))?;
    let state = AppState {
        config: Arc::new(config),
        pool,
        repos,
    };
    let api = Router::new()
        .merge(auth::routes())
        .merge(spaces::routes())
        .merge(content::routes())
        .merge(invitations::routes())
        .merge(pull_requests::routes())
        .layer(DefaultBodyLimit::max(MAX_API_REQUEST_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            API_REQUEST_TIMEOUT,
        ))
        .layer(
            CorsLayer::new()
                .allow_origin(browser_origin)
                .allow_methods([Method::GET, Method::POST, Method::DELETE])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        );
    Ok(Router::new()
        .route("/healthz", get(healthz))
        .merge(api)
        .merge(git_http::routes())
        .with_state(state))
}

async fn healthz() -> axum::Json<Value> {
    axum::Json(json!({ "status": "ok" }))
}
