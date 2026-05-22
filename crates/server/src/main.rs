use axum::routing::{get, post};
use axum::Router;
use cowiki_core::compiler::Compiler;
use cowiki_core::openai::OpenAIClient;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

mod config;
mod error;
mod routes;

pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: config::Config,
    pub wiki_repo: cowiki_core::git::WikiRepo,
    pub compiler: Compiler,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let config = config::Config::from_env();

    // Database
    let db = cowiki_db::create_pool(&config.database_url)
        .await
        .expect("failed to connect to database");
    cowiki_db::run_migrations(&db)
        .await
        .expect("failed to run migrations");
    tracing::info!("database connected and migrations applied");

    // Git repo
    let wiki_repo = cowiki_core::git::WikiRepo::open_or_init(&config.data_dir)
        .expect("failed to init wiki repo");
    // Ensure default user branch (for backward compat)
    wiki_repo
        .ensure_user_branch("default")
        .expect("failed to create default user branch");
    tracing::info!("wiki repo initialized at {}/repo", config.data_dir);

    // Compiler
    let openai = OpenAIClient::new(&config.openai_api_key, &config.openai_base_url);
    let compiler = Compiler::new(openai);

    let state = Arc::new(AppState {
        db,
        config,
        wiki_repo,
        compiler,
    });

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
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
        .route("/api/workspaces/{slug}/invite", post(routes::workspace::invite))
        .route("/api/workspaces/{slug}/members", get(routes::workspace::list_members))
        // Pages
        .route("/api/pages", get(routes::pages::list_pages))
        .route("/api/pages", post(routes::pages::write_page))
        .route("/api/pages/{slug}", get(routes::pages::get_page))
        // Ingest
        .route("/api/ingest", post(routes::ingest::ingest))
        // Compile
        .route("/api/compile", post(routes::compile::compile))
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

    let port = std::env::var("COWIKI_PORT").unwrap_or_else(|_| "3000".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    tracing::info!("cowiki server listening on port {port}");
    axum::serve(listener, app).await.unwrap();
}
