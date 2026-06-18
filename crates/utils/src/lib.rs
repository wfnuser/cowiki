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
    pub mcp: McpConfig,
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
pub struct McpConfig {
    pub port: u16,
    pub api_url: String,
    /// Shared secret for MCP server auth. When set, the server passes it to
    /// agents via .mcp.json and the MCP binary validates it. Auto-generated
    /// at startup if not configured (logged for the operator to use).
    pub auth_token: Option<String>,
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
        let mcp_port: u16 = std::env::var("COWIKI_MCP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9380);

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

        // MCP auth token: from env var, or auto-generate + log for operator
        let mcp_auth_token = std::env::var("COWIKI_MCP_AUTH_TOKEN").ok().or_else(|| {
            let token = format!(
                "cw_mcp_{}",
                uuid::Uuid::new_v4().to_string().replace('-', "")
            );
            tracing::info!(
                "COWIKI_MCP_AUTH_TOKEN not set — auto-generated token: {token}\n\
                 Set this in your .env file and pass it to the MCP binary:\n\
                 COWIKI_MCP_AUTH_TOKEN={token} cargo run --bin cowiki-mcp"
            );
            Some(token)
        });

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
            mcp: McpConfig {
                port: mcp_port,
                api_url: std::env::var("COWIKI_API_URL")
                    .unwrap_or_else(|_| "http://localhost:3000/".into()),
                auth_token: mcp_auth_token,
            },
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
        assert_eq!(config.mcp.port, 9380, "COWIKI_MCP_PORT default");
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.embedder.dimension, 1536);
    }

    #[test]
    #[serial]
    fn test_env_config_server_port() {
        std::env::set_var("COWIKI_PORT", "4000");
        let config = CowikiConfig::from_env();
        assert_eq!(config.server.port, 4000);
        assert_eq!(config.mcp.port, 9380, "MCP port unchanged");
        std::env::remove_var("COWIKI_PORT");
    }

    #[test]
    #[serial]
    fn test_env_config_mcp_port() {
        std::env::set_var("COWIKI_MCP_PORT", "9090");
        let config = CowikiConfig::from_env();
        assert_eq!(config.server.port, 3000, "server port unchanged");
        assert_eq!(config.mcp.port, 9090);
        std::env::remove_var("COWIKI_MCP_PORT");
    }

    #[test]
    #[serial]
    fn test_env_config_both_ports_independent() {
        std::env::set_var("COWIKI_PORT", "4000");
        std::env::set_var("COWIKI_MCP_PORT", "9090");
        let config = CowikiConfig::from_env();
        assert_eq!(config.server.port, 4000);
        assert_eq!(config.mcp.port, 9090);
        std::env::remove_var("COWIKI_PORT");
        std::env::remove_var("COWIKI_MCP_PORT");
    }

    #[test]
    #[serial]
    fn test_env_config_api_url() {
        std::env::set_var("COWIKI_API_URL", "http://remote:3000/api");
        let config = CowikiConfig::from_env();
        assert_eq!(config.mcp.api_url, "http://remote:3000/api");
        std::env::remove_var("COWIKI_API_URL");
    }

    #[test]
    #[serial]
    fn test_mcp_auth_token_from_env() {
        std::env::set_var("COWIKI_MCP_AUTH_TOKEN", "test-token-123");
        let config = CowikiConfig::from_env();
        assert_eq!(config.mcp.auth_token.as_deref(), Some("test-token-123"));
        std::env::remove_var("COWIKI_MCP_AUTH_TOKEN");
    }

    #[test]
    #[serial]
    fn test_mcp_auth_token_auto_generated() {
        // When COWIKI_MCP_AUTH_TOKEN is not set, should auto-generate one
        std::env::remove_var("COWIKI_MCP_AUTH_TOKEN");
        let config = CowikiConfig::from_env();
        assert!(
            config.mcp.auth_token.is_some(),
            "should auto-generate a token"
        );
        let token = config.mcp.auth_token.unwrap();
        assert!(
            token.starts_with("cw_mcp_"),
            "token should have cw_mcp_ prefix: {token}"
        );
    }
}
