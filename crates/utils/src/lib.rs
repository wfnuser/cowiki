//! Shared configuration for cowiki services.
//!
//! All config is loaded from environment variables (`.env` via dotenvy).
//! No TOML config file support — keep it simple.

use std::path::PathBuf;

pub mod token_usage;

// ── CLI args ────────────────────────────────────────────────────

/// Common CLI arguments for cowiki binaries.
#[derive(clap::Parser, Debug)]
pub struct CliArgs {
    /// Path to cowiki.conf configuration file (deprecated — use .env)
    #[arg(short = 'c', long = "config", env = "COWIKI_CONFIG")]
    pub config: Option<PathBuf>,
}

// ── Public config types ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CowikiConfig {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub llm: LlmConfig,
    pub embedder: EmbedderConfig,
    pub agent: AgentConfig,
    pub frontend_url: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub embedding_dimension: u32,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub data_dir: String,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub temperature: f64,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub dimension: u32,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub hard_timeout_secs: u64,
    pub soft_timeout_secs: u64,
    pub max_concurrent_per_workspace: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            hard_timeout_secs: 300,
            soft_timeout_secs: 1800,
            max_concurrent_per_workspace: 4,
        }
    }
}

// ── Config loader ───────────────────────────────────────────────

impl CowikiConfig {
    /// Load config from environment variables (`.env` via dotenvy).
    pub fn load(_args: Option<CliArgs>) -> Self {
        dotenvy::dotenv().ok();
        Self::from_env()
    }

    /// Load from environment variables only.
    pub fn from_env() -> Self {
        let server_port: u16 = std::env::var("COWIKI_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3000);
        let llm_api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        let llm_api_base =
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());

        let emb_api_key = std::env::var("COWIKI_EMBEDDER_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .unwrap_or_default();
        let emb_api_base = std::env::var("COWIKI_EMBEDDER_BASE_URL")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .unwrap_or_else(|_| llm_api_base.clone());

        let emb_dim: u32 = std::env::var("COWIKI_EMBEDDER_DIMENSION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1536);

        Self {
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "postgres://localhost/cowiki".into()),
                embedding_dimension: std::env::var("COWIKI_DATABASE_EMBEDDING_DIMENSION")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(emb_dim),
            },
            server: ServerConfig {
                port: server_port,
                data_dir: std::env::var("COWIKI_DATA_DIR").unwrap_or_else(|_| "./data".into()),
            },
            llm: LlmConfig {
                provider: std::env::var("COWIKI_LLM_PROVIDER").unwrap_or_else(|_| "openai".into()),
                model: std::env::var("COWIKI_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
                api_key: llm_api_key,
                api_base: llm_api_base,
                temperature: std::env::var("COWIKI_LLM_TEMPERATURE")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.3),
                max_tokens: std::env::var("COWIKI_LLM_MAX_TOKENS")
                    .ok()
                    .and_then(|s| s.parse().ok()),
            },
            embedder: EmbedderConfig {
                provider: std::env::var("COWIKI_EMBEDDER_PROVIDER")
                    .unwrap_or_else(|_| "openai".into()),
                model: std::env::var("COWIKI_EMBEDDER_MODEL")
                    .unwrap_or_else(|_| "text-embedding-3-small".into()),
                api_key: emb_api_key,
                api_base: emb_api_base,
                dimension: emb_dim,
            },
            frontend_url: std::env::var("FRONTEND_URL")
                .unwrap_or_else(|_| "http://localhost:5173".into()),
            agent: AgentConfig {
                hard_timeout_secs: std::env::var("COWIKI_AGENT_HARD_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300),
                soft_timeout_secs: std::env::var("COWIKI_AGENT_SOFT_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1800),
                max_concurrent_per_workspace: std::env::var("COWIKI_AGENT_MAX_CONCURRENT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(4),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // ── Config defaults ───────────────────────────────────────────

    #[test]
    #[serial]
    fn test_env_config_defaults() {
        let config = CowikiConfig::from_env();
        assert_eq!(config.server.port, 3000, "COWIKI_PORT default");
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.embedder.dimension, 1536);
    }

    #[test]
    #[serial]
    fn test_env_config_server_port() {
        std::env::set_var("COWIKI_PORT", "4000");
        let config = CowikiConfig::from_env();
        assert_eq!(config.server.port, 4000);
        std::env::remove_var("COWIKI_PORT");
    }

    #[test]
    #[serial]
    fn test_env_config_agent_defaults() {
        let config = CowikiConfig::from_env();
        assert_eq!(config.agent.hard_timeout_secs, 300);
        assert_eq!(config.agent.soft_timeout_secs, 1800);
        assert_eq!(config.agent.max_concurrent_per_workspace, 4);
    }
}
