pub mod config;
pub mod db;
pub mod error;
pub mod model;

use axum::Router;
use axum::routing::get;
use config::Config;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
}

pub fn build_router(config: Config, pool: PgPool) -> Router {
    let state = AppState {
        config: Arc::new(config),
        pool,
    };
    Router::new()
        .route("/healthz", get(healthz))
        .with_state(state)
}

async fn healthz() -> axum::Json<Value> {
    axum::Json(json!({ "status": "ok" }))
}
