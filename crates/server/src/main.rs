use axum::routing::{get, post};
use axum::Router;
use clap::Parser;
use cowiki_core::compiler::Compiler;
use cowiki_core::ai::embedder::{create_embedder, EmbedderConfig};
use cowiki_core::ai::llm::{create_llm, LlmConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

mod config;
mod error;
mod routes;

use config::CliArgs;

pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: config::Config,
    pub wiki_repo: cowiki_core::git::WikiRepo,       // default repo (backward compat)
    pub repo_manager: cowiki_core::git::WikiRepoManager, // per-workspace repos
    pub compiler: Compiler,
}

// ── Usage endpoint response ──

#[derive(serde::Serialize)]
struct UsageResponse {
    llm: HashMap<String, cowiki_core::ai::token_usage::TokenUsage>,
    vlm: HashMap<String, cowiki_core::ai::token_usage::TokenUsage>,
    embedder: HashMap<String, cowiki_core::ai::token_usage::TokenUsage>,
}

async fn get_usage(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<UsageResponse> {
    axum::Json(UsageResponse {
        llm: state.compiler.llm_usage(),
        vlm: state.compiler.vlm_usage(),
        embedder: state.compiler.embedder_usage(),
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let cli_args = CliArgs::parse();
    let config = config::Config::load(Some(cli_args));

    // Database
    let db = cowiki_db::create_pool(&config.database.url)
        .await
        .expect("failed to connect to database");
    cowiki_db::run_migrations(&db, config.database.embedding_dimension)
        .await
        .expect("failed to run migrations");
    tracing::info!("database connected and migrations applied");

    // Git repos
    let repo_manager = cowiki_core::git::WikiRepoManager::new(&config.server.data_dir);
    // Default repo for backward compat
    let wiki_repo = cowiki_core::git::WikiRepo::open_or_init(&config.server.data_dir)
        .expect("failed to init default wiki repo");
    wiki_repo
        .ensure_user_branch("default")
        .expect("failed to create default user branch");
    tracing::info!("wiki repos dir: {}", config.server.data_dir);

    let llm = create_llm(LlmConfig {
        provider: config.llm.provider.clone(),
        model: config.llm.model.clone(),
        api_key: config.llm.api_key.clone(),
        api_base: config.llm.api_base.clone(),
        temperature: config.llm.temperature,
        max_tokens: config.llm.max_tokens,
    });

    let embedder = create_embedder(EmbedderConfig {
        provider: config.embedder.provider.clone(),
        model: config.embedder.model.clone(),
        api_key: config.embedder.api_key.clone(),
        api_base: config.embedder.api_base.clone(),
        dimension: config.embedder.dimension,
    });

    // Compiler
    let compiler = Compiler::new(llm, None, embedder);

    let port = config.server.port.to_string();

    let state = Arc::new(AppState {
        db,
        config,
        wiki_repo,
        repo_manager,
        compiler,
    });

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        // Usage
        .route("/api/usage", get(get_usage))
        // Auth
        .route("/api/auth/register", post(routes::auth::register))
        .route("/api/auth/me", get(routes::auth::me))
        .route("/api/auth/regenerate-key", post(routes::auth::regenerate_key))
        .route("/api/auth/github", get(routes::auth::github_login))
        .route("/api/auth/github/callback", get(routes::auth::github_callback))
        // Workspaces
        .route("/api/workspaces", get(routes::workspace::list_workspaces))
        .route("/api/workspaces", post(routes::workspace::create_workspace))
        .route("/api/workspaces/public", get(routes::workspace::list_public_workspaces))
        .route("/api/workspaces/{slug}/join", post(routes::workspace::join_workspace))
        .route("/api/workspaces/{slug}/rename", post(routes::workspace::rename_workspace))
        .route("/api/workspaces/{slug}/invite", post(routes::workspace::invite))
        .route("/api/workspaces/{slug}/members", get(routes::workspace::list_members))
        // Pages (legacy — uses default repo)
        .route("/api/pages", get(routes::pages::list_pages))
        .route("/api/pages", post(routes::pages::write_page))
        .route("/api/folders", post(routes::pages::create_folder))
        .route("/api/pages/{slug}", get(routes::pages::get_page))
        // Pages (workspace-scoped — uses per-workspace repo)
        .route("/api/workspaces/{ws_slug}/pages", get(routes::pages::list_pages_ws))
        .route("/api/workspaces/{ws_slug}/pages", post(routes::pages::write_page_ws))
        .route("/api/workspaces/{ws_slug}/folders", post(routes::pages::create_folder_ws))
        .route("/api/workspaces/{ws_slug}/pages/{slug}", get(routes::pages::get_page_ws))
        // Ingest
        .route("/api/ingest", post(routes::ingest::ingest))
        .route("/api/workspaces/{ws_slug}/ingest", post(routes::ingest::ingest_ws))
        // Compile
        .route("/api/compile", post(routes::compile::compile))
        .route("/api/workspaces/{ws_slug}/compile", post(routes::compile::compile_ws))
        // Submit
        .route("/api/submit", post(routes::submit::submit))
        // Reviews
        .route("/api/reviews", get(routes::review::list_reviews))
        .route("/api/reviews/{id}", get(routes::review::get_review))
        .route("/api/reviews/{id}", post(routes::review::review_action))
        // Search
        .route("/api/search", get(routes::search::search))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    tracing::info!("cowiki server listening on port {port}");
    axum::serve(listener, app).await.unwrap();
}
