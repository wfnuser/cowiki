//! cowiki-server configuration.
//! Delegates to `cowiki_utils::CowikiConfig` for shared config,
//! adds server-specific fields (GitHub OAuth).

#[derive(Debug, Clone)]
pub struct Config {
    pub database: cowiki_utils::DatabaseConfig,
    pub server: cowiki_utils::ServerConfig,
    pub llm: cowiki_utils::LlmConfig,
    pub embedder: cowiki_utils::EmbedderConfig,
    pub agent: cowiki_utils::AgentConfig,
    pub auth: AuthConfig,
    pub frontend_url: String,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_redirect_uri: String,
    /// GitHub token for Copilot CLI ACP auth (COPILOT_GITHUB_TOKEN env).
    /// Required for `copilot --acp` even in BYOK mode.
    pub copilot_github_token: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let shared = cowiki_utils::CowikiConfig::load(None);

        // GitHub OAuth + Copilot: env vars only (loaded from .env via dotenvy).
        let auth = AuthConfig {
            github_client_id: std::env::var("GITHUB_CLIENT_ID").unwrap_or_default(),
            github_client_secret: std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default(),
            github_redirect_uri: std::env::var("GITHUB_REDIRECT_URI")
                .unwrap_or_else(|_| "http://localhost:3000/api/auth/github/callback".into()),
            copilot_github_token: std::env::var("COPILOT_GITHUB_TOKEN").ok(),
        };

        Config {
            database: shared.database,
            server: shared.server,
            llm: shared.llm,
            embedder: shared.embedder,
            agent: shared.agent,
            auth,
            frontend_url: shared.frontend_url,
        }
    }
}
