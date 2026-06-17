//! cowiki MCP Server — standalone binary.
//!
//! Start independently from the main cowiki server:
//!   cargo run --bin cowiki-mcp -- --data-dir ./data --port 9380
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

    /// MCP server bind address (e.g., 127.0.0.1:9380)
    #[arg(long, env = "COWIKI_MCP_PORT", default_value = "127.0.0.1:9380")]
    port: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    tracing::info!(
        data_dir = %args.data_dir,
        port = %args.port,
        "starting cowiki MCP server"
    );

    let gateway = Arc::new(WikiFsGateway::new(&args.data_dir));

    cowiki_mcp::start_mcp_server(gateway, &args.port).await?;

    Ok(())
}
