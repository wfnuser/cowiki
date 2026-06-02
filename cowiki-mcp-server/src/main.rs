//! cowiki MCP Server — rmcp + hyper standalone process.
//!
//! Acts as an MCP→REST proxy: MCP tools call cowiki-server's HTTP API
//! instead of duplicating business logic. No cowiki-core/db deps needed.
//!
//! Configuration: COWIKI_MCP_PORT (default 8080), COWIKI_BASE_URL (default http://localhost:3000).
//! Priority: CLI args > env vars > defaults.

use clap::Parser;
use hyper_util::{rt::TokioExecutor, server::conn::auto::Builder, service::TowerToHyperService};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

mod server;
use server::CowikiServer;

#[derive(Parser, Debug)]
#[command(name = "cowiki-mcp")]
struct Cli {
    /// MCP server listen port
    #[arg(long, env = "COWIKI_MCP_PORT")]
    port: Option<u16>,
    /// cowiki-server REST API base URL
    #[arg(long, env = "COWIKI_BASE_URL")]
    server_url: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,cowiki_mcp_server=debug".into()),
        )
        .init();

    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    let port = cli
        .port
        .or_else(|| std::env::var("COWIKI_MCP_PORT").ok().and_then(|v| v.parse().ok()))
        .unwrap_or(8080);
    let api_base = cli
        .server_url
        .or_else(|| std::env::var("COWIKI_BASE_URL").ok())
        .unwrap_or_else(|| "http://localhost:3000".into());

    let bind_addr = format!("0.0.0.0:{port}");
    tracing::info!("MCP server on {bind_addr}, backend: {api_base}");

    let server = CowikiServer::new(api_base);
    let mcp_service: StreamableHttpService<CowikiServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(server.clone()),
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );

    let tower_service = TowerToHyperService::new(mcp_service);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {bind_addr}");

    loop {
        let (stream, addr) = tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            accept = listener.accept() => match accept {
                Ok(c) => c,
                Err(e) => { tracing::error!("accept: {e}"); continue; }
            },
        };
        let svc = tower_service.clone();
        tokio::spawn(async move {
            if let Err(e) = Builder::new(TokioExecutor::default())
                .serve_connection(hyper_util::rt::TokioIo::new(stream), svc).await
            {
                if !e.to_string().contains("connection") {
                    tracing::error!("conn {addr}: {e}");
                }
            }
        });
    }

    #[allow(unreachable_code)]
    Ok(())
}
