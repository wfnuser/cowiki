//! cowiki-server configuration.
//! Delegates to `cowiki_utils::CowikiConfig` for shared config,
//! adds server-specific fields (GitHub OAuth).

pub use cowiki_utils::CliArgs;

#[derive(Debug, Clone)]
pub struct Config {
    pub database: cowiki_utils::DatabaseConfig,
    pub server: cowiki_utils::ServerConfig,
    pub llm: cowiki_utils::LlmConfig,
    pub embedder: cowiki_utils::EmbedderConfig,
    pub auth: AuthConfig,
    pub frontend_url: String,
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

        // GitHub OAuth: from TOML if available, else env vars
        let github = cowiki_utils::discover_config_path(None).and_then(|path| {
            let content = std::fs::read_to_string(&path).ok()?;
            let toml: cowiki_utils::TomlConfig = toml::from_str(&content).ok()?;
            toml.github
        });

        let auth = AuthConfig {
            github_client_id: github.as_ref().and_then(|g| g.client_id.clone())
                .or_else(|| std::env::var("GITHUB_CLIENT_ID").ok())
                .unwrap_or_default(),
            github_client_secret: github.as_ref().and_then(|g| g.client_secret.clone())
                .or_else(|| std::env::var("GITHUB_CLIENT_SECRET").ok())
                .unwrap_or_default(),
            github_redirect_uri: github.as_ref().and_then(|g| g.redirect_uri.clone())
                .or_else(|| std::env::var("GITHUB_REDIRECT_URI").ok())
                .unwrap_or_else(|| "http://localhost:3000/api/auth/github/callback".into()),
        };

        Config {
            database: shared.database,
            server: shared.server,
            llm: shared.llm,
            embedder: shared.embedder,
            auth,
            frontend_url: shared.frontend_url,
        }
    }
}
