//! cowiki MCP Server — standalone binary.
//!
//! Start independently from the main cowiki server:
//!   COWIKI_MCP_AUTH_TOKEN=<token> cargo run --bin cowiki-mcp -- --data-dir ./data
//!
//! In production, run before cowiki server so agents can connect.

use clap::Parser;
use std::sync::Arc;

use cowiki_core::gateway::WikiFsGateway;

#[derive(Parser)]
#[command(name = "cowiki-mcp", about = "MCP server for cowiki wiki operations")]
struct Args {
    /// Data directory for wiki repos
    #[arg(long, env = "COWIKI_DATA_DIR", default_value = "data")]
    data_dir: String,

    /// MCP server port (always binds 127.0.0.1:{port})
    #[arg(long, env = "COWIKI_MCP_PORT", default_value = "9380")]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let bind_addr = format!("127.0.0.1:{}", args.port);
    let auth_token = std::env::var("COWIKI_MCP_AUTH_TOKEN").ok();

    tracing::info!(
        data_dir = %args.data_dir,
        port = %args.port,
        bind_addr = %bind_addr,
        has_auth = auth_token.is_some(),
        "starting cowiki MCP server"
    );

    let gateway = Arc::new(WikiFsGateway::new(&args.data_dir));

    cowiki_mcp::start_mcp_server(gateway, &bind_addr, auth_token.as_deref()).await?;

    Ok(())
}
