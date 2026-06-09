use axum::routing::{delete, get, post};
use axum::Router;
use clap::Parser;
use cowiki_core::compiler::Compiler;
use cowiki_db::embed::{create_embedder, EmbedderConfig};
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
    pub repo_manager: cowiki_core::git::WikiRepoManager, // per-workspace repos
    pub compiler: Compiler,
}

// ── Usage endpoint response ──

#[derive(serde::Serialize)]
struct UsageResponse {
    embedder: HashMap<String, cowiki_utils::token_usage::TokenUsage>,
}

async fn get_usage(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<UsageResponse> {
    axum::Json(UsageResponse {
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

    // Git repos (one per workspace, created lazily)
    let repo_manager = cowiki_core::git::WikiRepoManager::new(&config.server.data_dir);
    tracing::info!("wiki repos dir: {}", config.server.data_dir);

    let embedder = create_embedder(EmbedderConfig {
        provider: config.embedder.provider.clone(),
        model: config.embedder.model.clone(),
        api_key: config.embedder.api_key.clone(),
        api_base: config.embedder.api_base.clone(),
        dimension: config.embedder.dimension,
    });

    // Compiler (legacy — will be replaced by agent dispatch)
    let compiler = Compiler::new(embedder);

    let port = config.server.port.to_string();

    let state = Arc::new(AppState {
        db,
        config,
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
        // Invitations (accept/reject/pending)
        .route("/api/invitations/pending", get(routes::workspace::list_pending_invitations))
        .route("/api/invitations/{id}/accept", post(routes::workspace::accept_invitation))
        .route("/api/invitations/{id}/reject", post(routes::workspace::reject_invitation))
        // Member management (owner only)
        .route("/api/workspaces/{slug}/members/remove", post(routes::workspace::remove_member))
        .route("/api/workspaces/{slug}/members/role", post(routes::workspace::change_member_role))
        // Workspace deletion (owner only)
        .route("/api/workspaces/{slug}", delete(routes::workspace::delete_workspace))
        // Pages (workspace-scoped — uses per-workspace repo)
        .route("/api/workspaces/{ws_slug}/pages", get(routes::pages::list_pages_ws))
        .route("/api/workspaces/{ws_slug}/pages", post(routes::pages::write_page_ws))
        .route("/api/workspaces/{ws_slug}/folders", post(routes::pages::create_folder_ws))
        .route("/api/workspaces/{ws_slug}/pages/{*slug}", get(routes::pages::get_page_ws))
        // Ingest
        .route("/api/workspaces/{ws_slug}/ingest", post(routes::ingest::ingest_ws))
        // Compile
        .route("/api/workspaces/{ws_slug}/compile", post(routes::compile::compile_ws))
        // Sources
        .route("/api/workspaces/{ws_slug}/sources", get(routes::sources::list_sources))
        .route("/api/workspaces/{ws_slug}/sources/{filename}", get(routes::sources::get_source))
        // Submit (workspace-scoped)
        .route("/api/workspaces/{ws_slug}/submit", post(routes::submit::submit))
        // Reviews (workspace-scoped)
        .route("/api/workspaces/{ws_slug}/reviews", get(routes::review::list_reviews))
        .route("/api/workspaces/{ws_slug}/reviews/{id}", get(routes::review::get_review))
        .route("/api/workspaces/{ws_slug}/reviews/{id}", post(routes::review::review_action))
        // Search
        .route("/api/search", get(routes::search::search))
        // API Keys — multi-key management
        .route("/api/keys", get(routes::keys::list_keys))
        .route("/api/keys", post(routes::keys::create_key))
        .route("/api/keys/{id}", delete(routes::keys::revoke_key))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    tracing::info!("cowiki server listening on port {port}");
    axum::serve(listener, app).await.unwrap();
}
