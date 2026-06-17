use axum::routing::{delete, get, post};
use axum::Router;
use clap::Parser;
use cowiki_core::ai::embedder::{create_embedder, EmbedderConfig};
use cowiki_core::ai::llm::{create_llm, LlmConfig};
use cowiki_core::compile::Compiler;
use cowiki_core::gateway::WikiFsGateway;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

mod config;
mod error;
mod routes;

use config::CliArgs;

pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: config::Config,
    pub repo_manager: cowiki_core::git::WikiRepoManager,
    pub compiler: Compiler,
    /// WikiFsGateway for tool execution
    pub wiki_fs_gateway: Arc<WikiFsGateway>,
    /// Global AgentManager (single instance for all workspaces)
    pub agent_manager: Arc<cowiki_agents::manager::AgentManager>,
}

// ── Usage endpoint response ──

#[derive(serde::Serialize)]
struct UsageResponse {
    llm: HashMap<String, cowiki_core::ai::token_usage::TokenUsage>,
    embedder: HashMap<String, cowiki_core::ai::token_usage::TokenUsage>,
}

async fn get_usage(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<UsageResponse> {
    axum::Json(UsageResponse {
        llm: state.compiler.llm_usage(),
        embedder: state.compiler.embedder_usage(),
    })
}

// ── Agent events SSE ──────────────────────────────────────────

async fn agent_events(
    axum::extract::Path(_ws): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Sse<
    impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    let (tx, rx) = tokio::sync::mpsc::channel::<
        Result<axum::response::sse::Event, std::convert::Infallible>,
    >(64);

    let agent_mgr = Arc::clone(&state.agent_manager);
    let mut event_rx = agent_mgr.subscribe_events();

    tokio::spawn(async move {
        // Send initial connected message
        let _ = tx
            .send(Ok(axum::response::sse::Event::default().data(
                r#"{"type":"connected","message":"Agent events stream established"}"#,
            )))
            .await;

        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        let _ = tx
                            .send(Ok(axum::response::sse::Event::default().data(json)))
                            .await;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "SSE event lag");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    axum::response::Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx))
        .keep_alive(axum::response::sse::KeepAlive::default())
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
    tracing::info!("wiki repos dir: {}", config.server.data_dir);

    // Validate pi binary availability
    let pi_binary = std::env::var("PI_BINARY").unwrap_or_else(|_| "pi".into());
    match which::which(&pi_binary) {
        Ok(path) => {
            tracing::info!("pi binary found at {}", path.display());
        }
        Err(_) => {
            tracing::warn!(
                "pi binary '{}' NOT found on PATH. Agent-based compile will fail.",
                pi_binary
            );
        }
    }

    // Validate claude binary
    let claude_binary = std::env::var("CLAUDE_BINARY").unwrap_or_else(|_| "claude".into());
    match which::which(&claude_binary) {
        Ok(path) => {
            tracing::info!("claude binary found at {}", path.display());
        }
        Err(_) => {
            tracing::warn!(
                "claude binary '{}' NOT found on PATH. Claude agent will be unavailable.",
                claude_binary
            );
        }
    }

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

    let compiler = Compiler::new(llm, embedder);

    let port = config.server.port.to_string();
    let data_dir = config.server.data_dir.clone();

    let expire_db = db.clone();

    let wiki_fs_gateway = Arc::new(WikiFsGateway::new(&data_dir).with_db(db.clone()));

    // Create global AgentManager (single instance for all workspaces)
    let mcp_port = std::env::var("COWIKI_MCP_PORT").unwrap_or_else(|_| "9380".into());
    let default_workspace_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(&data_dir)
        .join("default")
        .join("repo");
    let agent_manager = Arc::new(cowiki_agents::manager::AgentManager::new(
        format!("http://127.0.0.1:{}/mcp/sse", mcp_port),
        default_workspace_path,
        cowiki_utils::LlmConfig {
            provider: config.llm.provider.clone(),
            model: config.llm.model.clone(),
            api_key: config.llm.api_key.clone(),
            api_base: config.llm.api_base.clone(),
            temperature: config.llm.temperature,
            max_tokens: config.llm.max_tokens,
        },
    ));

    // MCP server is started separately (see crates/mcp for standalone binary).
    // In e2e tests: cargo run --bin cowiki-mcp & before cowiki server.

    let state = Arc::new(AppState {
        db,
        config,
        repo_manager,
        compiler,
        wiki_fs_gateway,
        agent_manager,
    });

    // Background task: expire stale invitations
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            match cowiki_db::workspaces::expire_stale_invitations(&expire_db).await {
                Ok(n) if n > 0 => tracing::info!("expired {n} stale invitation(s)"),
                Err(e) => tracing::error!("failed to expire stale invitations: {e}"),
                _ => {}
            }
        }
    });

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/usage", get(get_usage))
        // Auth
        .route("/api/auth/register", post(routes::auth::register))
        .route("/api/auth/me", get(routes::auth::me))
        .route(
            "/api/auth/regenerate-key",
            post(routes::auth::regenerate_key),
        )
        .route("/api/auth/github", get(routes::auth::github_login))
        .route(
            "/api/auth/github/callback",
            get(routes::auth::github_callback),
        )
        // Workspaces
        .route("/api/workspaces", get(routes::workspace::list_workspaces))
        .route("/api/workspaces", post(routes::workspace::create_workspace))
        .route(
            "/api/workspaces/public",
            get(routes::workspace::list_public_workspaces),
        )
        .route(
            "/api/workspaces/{slug}/join",
            post(routes::workspace::join_workspace),
        )
        .route(
            "/api/workspaces/{slug}/rename",
            post(routes::workspace::rename_workspace),
        )
        .route(
            "/api/workspaces/{slug}/invite",
            post(routes::workspace::invite),
        )
        .route(
            "/api/workspaces/{slug}/members",
            get(routes::workspace::list_members),
        )
        .route(
            "/api/workspaces/{slug}/invitations",
            get(routes::workspace::list_invitations),
        )
        .route(
            "/api/workspaces/{slug}/invitations/{id}/resend",
            post(routes::workspace::resend_invitation),
        )
        .route(
            "/api/workspaces/{slug}/invitations/{id}",
            delete(routes::workspace::revoke_invitation),
        )
        .route(
            "/api/invitations/pending",
            get(routes::workspace::list_pending_invitations),
        )
        .route(
            "/api/invitations/{id}/accept",
            post(routes::workspace::accept_invitation),
        )
        .route(
            "/api/invitations/{id}/reject",
            post(routes::workspace::reject_invitation),
        )
        .route(
            "/api/workspaces/{slug}/members/remove",
            post(routes::workspace::remove_member),
        )
        .route(
            "/api/workspaces/{slug}/members/role",
            post(routes::workspace::change_member_role),
        )
        .route(
            "/api/workspaces/{slug}/transfer-ownership",
            post(routes::transfers::initiate_transfer),
        )
        .route(
            "/api/transfers/pending",
            get(routes::transfers::list_pending_transfers),
        )
        .route(
            "/api/transfers/{id}/accept",
            post(routes::transfers::accept_transfer),
        )
        .route(
            "/api/transfers/{id}/reject",
            post(routes::transfers::reject_transfer),
        )
        .route(
            "/api/transfers/{id}",
            delete(routes::transfers::cancel_transfer),
        )
        .route(
            "/api/workspaces/{slug}",
            delete(routes::workspace::delete_workspace),
        )
        // Pages
        .route(
            "/api/workspaces/{ws_slug}/pages",
            get(routes::pages::list_pages_ws),
        )
        .route(
            "/api/workspaces/{ws_slug}/pages",
            post(routes::pages::write_page_ws),
        )
        .route(
            "/api/workspaces/{ws_slug}/folders",
            post(routes::pages::create_folder_ws),
        )
        .route(
            "/api/workspaces/{ws_slug}/paths/rename",
            post(routes::pages::rename_path_ws),
        )
        .route(
            "/api/workspaces/{ws_slug}/paths/delete",
            post(routes::pages::delete_path_ws),
        )
        .route(
            "/api/workspaces/{ws_slug}/pages/{*slug}",
            get(routes::pages::get_page_ws),
        )
        // Ingest
        .route(
            "/api/workspaces/{ws_slug}/ingest",
            post(routes::ingest::ingest_ws),
        )
        // Compile
        .route(
            "/api/workspaces/{ws_slug}/compile",
            post(routes::compile::compile_ws),
        )
        // Sources
        .route(
            "/api/workspaces/{ws_slug}/sources",
            get(routes::sources::list_sources),
        )
        .route(
            "/api/workspaces/{ws_slug}/sources/{filename}",
            get(routes::sources::get_source),
        )
        // Submit
        .route(
            "/api/workspaces/{ws_slug}/submit",
            post(routes::submit::submit),
        )
        .route(
            "/api/workspaces/{ws_slug}/rebase",
            post(routes::submit::rebase),
        )
        // Reviews
        .route(
            "/api/workspaces/{ws_slug}/reviews",
            get(routes::review::list_reviews),
        )
        .route(
            "/api/workspaces/{ws_slug}/reviews/{id}",
            get(routes::review::get_review),
        )
        .route(
            "/api/workspaces/{ws_slug}/reviews/{id}",
            post(routes::review::review_action),
        )
        .route(
            "/api/workspaces/{ws_slug}/reviews/{id}/edit",
            post(routes::review::edit_review),
        )
        .route(
            "/api/workspaces/{ws_slug}/reviews/{submission_id}/comments",
            get(routes::comments::list_comments),
        )
        .route(
            "/api/workspaces/{ws_slug}/reviews/{submission_id}/comments",
            post(routes::comments::create_comment),
        )
        .route(
            "/api/workspaces/{ws_slug}/reviews/{submission_id}/comments/{comment_id}/resolve",
            post(routes::comments::resolve_comment),
        )
        .route(
            "/api/workspaces/{ws_slug}/reviews/{submission_id}/comments/{comment_id}/unresolve",
            post(routes::comments::unresolve_comment),
        )
        .route(
            "/api/workspaces/{ws_slug}/reviews/{submission_id}/comments/{comment_id}",
            delete(routes::comments::delete_comment),
        )
        // Search
        .route(
            "/api/workspaces/{ws_slug}/search",
            get(routes::search::search),
        )
        // API Keys
        .route("/api/keys", get(routes::keys::list_keys))
        .route("/api/keys", post(routes::keys::create_key))
        .route("/api/keys/{id}", delete(routes::keys::revoke_key))
        // User search
        .route("/api/users/search", get(routes::users::search_users))
        // Notifications
        .route(
            "/api/notifications",
            get(routes::notifications::list_notifications),
        )
        .route(
            "/api/notifications/unread-count",
            get(routes::notifications::unread_count),
        )
        .route(
            "/api/notifications/read-all",
            post(routes::notifications::mark_all_read),
        )
        .route(
            "/api/notifications/{id}/read",
            post(routes::notifications::mark_read),
        )
        // Agent events SSE
        .route("/api/agents/{ws}/events", get(agent_events))
        .layer(
            TraceLayer::new_for_http()
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    tracing::info!("cowiki server listening on port {port}");
    axum::serve(listener, app).await.unwrap();
}
