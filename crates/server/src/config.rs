//! cowiki-server configuration.
//! Delegates to `cowiki_utils::CowikiConfig` for shared config,
//! adds server-specific fields (GitHub OAuth).

pub use cowiki_utils::CliArgs;

#[derive(Debug, Clone)]
pub struct Config {
    pub database: cowiki_utils::DatabaseConfig,
    pub server: ServerSettings,
    pub llm: cowiki_utils::LlmConfig,
    pub embedder: cowiki_utils::EmbedderConfig,
    pub auth: AuthConfig,
    pub frontend_url: String,
}

/// Server settings with local-mode defaults resolved.
#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub port: u16,
    /// Wiki data directory: explicit config wins, else `~/cowiki` in local
    /// mode, `./data` for hosted deploys.
    pub data_dir: String,
    /// Single-user local mode: `/api/auth/local` signs you in without OAuth.
    /// Defaults to on when GitHub OAuth is not configured.
    pub local_mode: bool,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_redirect_uri: String,
}

impl Config {
    pub fn load(args: Option<CliArgs>) -> Self {
        let shared = cowiki_utils::CowikiConfig::load(args);

        // GitHub OAuth: env vars only (loaded from .env via dotenvy). Single source of truth.
        let auth = AuthConfig {
            github_client_id: std::env::var("GITHUB_CLIENT_ID").unwrap_or_default(),
            github_client_secret: std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default(),
            github_redirect_uri: std::env::var("GITHUB_REDIRECT_URI")
                .unwrap_or_else(|_| "http://localhost:3000/api/auth/github/callback".into()),
        };

        // Local mode: explicit config wins; otherwise on iff GitHub OAuth is
        // absent — hosted deploys (which set GITHUB_CLIENT_ID) are unaffected.
        let local_mode = shared
            .server
            .local_mode
            .unwrap_or_else(|| auth.github_client_id.is_empty());

        let server = ServerSettings {
            port: shared.server.port,
            data_dir: cowiki_utils::resolve_data_dir(shared.server.data_dir.as_deref(), local_mode),
            local_mode,
        };

        Config {
            database: shared.database,
            server,
            llm: shared.llm,
            embedder: shared.embedder,
            auth,
            frontend_url: shared.frontend_url,
        }
    }
}
