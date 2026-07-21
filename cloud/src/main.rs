use cowiki_cloud::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cowiki_cloud=info,tower_http=info".into()),
        )
        .json()
        .init();

    let config = Config::from_env()?;
    std::fs::create_dir_all(&config.repo_root)?;
    let metadata = std::fs::metadata(&config.repo_root)?;
    if !metadata.is_dir() || metadata.permissions().readonly() {
        return Err("COWIKI_REPO_ROOT must be a writable directory".into());
    }

    let pool = cowiki_cloud::db::connect_and_migrate(config.database_url.as_str()).await?;
    let bind_addr = config.bind_addr;
    let app = cowiki_cloud::build_router(config, pool)?;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(%bind_addr, "CoWiki Cloud listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
