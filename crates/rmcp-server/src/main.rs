//! cowiki MCP Server — rmcp + hyper standalone process.
//!
//! Acts as an MCP→REST proxy: MCP tools call cowiki-server's HTTP API
//! instead of duplicating business logic. No cowiki-core/db deps needed.
//!
//! Port: COWIKI_MCP_PORT > COWIKI_PORT > [server].port > default 8080.

use hyper_util::{rt::TokioExecutor, server::conn::auto::Builder, service::TowerToHyperService};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

mod server;
use server::CowikiServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,cowiki_rmcp_server=debug".into()),
        )
        .init();

    let args = clap::Parser::parse();
    let config = cowiki_utils::CowikiConfig::load(Some(args));

    let port = config.mcp.port;
    let api_base = config.mcp.api_url.clone();
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
