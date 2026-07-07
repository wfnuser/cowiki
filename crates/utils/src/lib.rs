//! Shared configuration for cowiki services.
//!
//! Supports loading from `cowiki.conf` (TOML) with environment variable
//! fallback. Shared by `cowiki-server`.

use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

// ── CLI args (reusable) ───────────────────────────────────────

/// Common CLI arguments for cowiki binaries.
#[derive(Parser, Debug)]
pub struct CliArgs {
    /// Path to cowiki.conf configuration file
    #[arg(short = 'c', long = "config", env = "COWIKI_CONFIG")]
    pub config: Option<PathBuf>,
}

// ── Raw TOML structure ────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TomlConfig {
    pub database: Option<TomlDatabase>,
    pub server: Option<TomlServer>,
    pub llm: Option<TomlLlm>,
    pub embedder: Option<TomlEmbedder>,
    pub frontend: Option<TomlFrontend>,
    #[serde(rename = "mcp-server")]
    pub mcp_server: Option<TomlMcpServer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlDatabase {
    pub url: Option<String>,
    pub embedding_dimension: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlServer {
    pub port: Option<u16>,
    pub data_dir: Option<String>,
    pub local_mode: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlLlm {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlEmbedder {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub dimension: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlFrontend {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlMcpServer {
    pub port: Option<u16>,
    pub api_url: Option<String>,
}

// ── Public config types ───────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CowikiConfig {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub llm: LlmConfig,
    pub embedder: EmbedderConfig,
    pub mcp: McpConfig,
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
    /// Wiki data directory as configured; `None` when not set anywhere.
    /// Consumers resolve the default (`resolve_data_dir`) so local mode can
    /// pick `~/cowiki` while hosted deploys keep `./data`.
    pub data_dir: Option<String>,
    /// Single-user local mode (no login). `None` = not configured; the server
    /// defaults it to "on" when GitHub OAuth is not configured.
    pub local_mode: Option<bool>,
}

/// Expand a leading `~/` to the user's home directory.
pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

/// Resolve the wiki data directory: explicit config wins (with `~` expansion);
/// otherwise `~/cowiki` in local mode, `./data` for hosted deploys.
pub fn resolve_data_dir(configured: Option<&str>, local_mode: bool) -> String {
    match configured {
        Some(dir) if !dir.is_empty() => expand_tilde(dir),
        _ if local_mode => expand_tilde("~/cowiki"),
        _ => "./data".into(),
    }
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
}

// ── Config discovery ──────────────────────────────────────────

/// Discover config file path: CLI arg → COWIKI_CONFIG env → ./cowiki.conf → ~/.cowiki/cowiki.conf
pub fn discover_config_path(cli_path: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(ref p) = cli_path {
        if p.exists() {
            tracing::info!("using config from CLI: {}", p.display());
            return cli_path;
        }
        tracing::warn!("config file not found: {}", p.display());
    }

    if let Ok(env_path) = std::env::var("COWIKI_CONFIG") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            tracing::info!("using config from COWIKI_CONFIG: {}", p.display());
            return Some(p);
        }
    }

    let local = PathBuf::from("cowiki.conf");
    if local.exists() {
        tracing::info!("using config: ./cowiki.conf");
        return Some(local);
    }

    if let Some(home) = home_dir() {
        let user_conf = home.join(".cowiki").join("cowiki.conf");
        if user_conf.exists() {
            tracing::info!("using config: {}", user_conf.display());
            return Some(user_conf);
        }
    }

    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

// ── Config loader ─────────────────────────────────────────────

impl CowikiConfig {
    /// Load config from cowiki.conf (with CLI arg override) or env vars.
    pub fn load(args: Option<CliArgs>) -> Self {
        dotenvy::dotenv().ok();
        let cli_config = args.and_then(|a| a.config);

        match discover_config_path(cli_config) {
            Some(path) => Self::from_file(&path),
            None => {
                tracing::info!("no cowiki.conf found, using environment variables");
                Self::from_env()
            }
        }
    }

    /// Load from a TOML config file.
    pub fn from_file(path: &PathBuf) -> Self {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read config file {}: {e}", path.display()));

        let toml: TomlConfig = toml::from_str(&content)
            .unwrap_or_else(|e| panic!("failed to parse config file {}: {e}", path.display()));

        let llm_api_key = toml
            .llm
            .as_ref()
            .and_then(|l| l.api_key.clone())
            .unwrap_or_default();
        let llm_api_base = toml
            .llm
            .as_ref()
            .and_then(|l| l.api_base.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".into());

        let emb_api_key = toml
            .embedder
            .as_ref()
            .and_then(|e| e.api_key.clone())
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| llm_api_key.clone());
        let emb_api_base = toml
            .embedder
            .as_ref()
            .and_then(|e| e.api_base.clone())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| llm_api_base.clone());

        let emb_dim = toml
            .embedder
            .as_ref()
            .and_then(|e| e.dimension)
            .or_else(|| toml.database.as_ref().and_then(|d| d.embedding_dimension))
            .unwrap_or(1536);

        Self {
            database: DatabaseConfig {
                url: toml
                    .database
                    .as_ref()
                    .and_then(|d| d.url.clone())
                    .unwrap_or_else(|| panic!("database.url is required")),
                embedding_dimension: emb_dim,
            },
            server: ServerConfig {
                port: toml.server.as_ref().and_then(|s| s.port).unwrap_or(3000),
                data_dir: std::env::var("COWIKI_DATA_DIR")
                    .ok()
                    .or_else(|| toml.server.as_ref().and_then(|s| s.data_dir.clone())),
                local_mode: std::env::var("COWIKI_LOCAL_MODE")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .or_else(|| toml.server.as_ref().and_then(|s| s.local_mode)),
            },
            llm: LlmConfig {
                provider: toml
                    .llm
                    .as_ref()
                    .and_then(|l| l.provider.clone())
                    .unwrap_or_else(|| "openai".into()),
                model: toml
                    .llm
                    .as_ref()
                    .and_then(|l| l.model.clone())
                    .unwrap_or_else(|| "gpt-4o-mini".into()),
                api_key: llm_api_key,
                api_base: llm_api_base,
                temperature: toml.llm.as_ref().and_then(|l| l.temperature).unwrap_or(0.3),
                max_tokens: toml.llm.as_ref().and_then(|l| l.max_tokens),
            },
            embedder: EmbedderConfig {
                provider: toml
                    .embedder
                    .as_ref()
                    .and_then(|e| e.provider.clone())
                    .unwrap_or_else(|| "openai".into()),
                model: toml
                    .embedder
                    .as_ref()
                    .and_then(|e| e.model.clone())
                    .unwrap_or_else(|| "text-embedding-3-small".into()),
                api_key: emb_api_key,
                api_base: emb_api_base,
                dimension: emb_dim,
            },
            frontend_url: toml
                .frontend
                .as_ref()
                .and_then(|f| f.url.clone())
                .unwrap_or_else(|| "http://localhost:5173".into()),
            mcp: McpConfig {
                port: toml
                    .mcp_server
                    .as_ref()
                    .and_then(|m| m.port)
                    .unwrap_or(8080),
                api_url: toml
                    .mcp_server
                    .as_ref()
                    .and_then(|m| m.api_url.clone())
                    .unwrap_or_else(|| "http://localhost:3000/".into()),
            },
        }
    }

    /// Load from environment variables only.
    pub fn from_env() -> Self {
        // COWIKI_PORT → cowiki-server REST API port; COWIKI_MCP_PORT → MCP server port
        let server_port: u16 = std::env::var("COWIKI_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3000);
        let mcp_port: u16 = std::env::var("COWIKI_MCP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8080);

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
                data_dir: std::env::var("COWIKI_DATA_DIR").ok(),
                local_mode: std::env::var("COWIKI_LOCAL_MODE")
                    .ok()
                    .and_then(|s| s.parse().ok()),
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
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // ── Config defaults ───────────────────────────────────────

    #[test]
    #[serial]
    fn test_env_config_defaults() {
        let config = CowikiConfig::from_env();
        assert_eq!(config.server.port, 3000, "COWIKI_PORT default");
        assert_eq!(config.mcp.port, 8080, "COWIKI_MCP_PORT default");
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.embedder.dimension, 1536);
    }

    #[test]
    #[serial]
    fn test_env_config_server_port() {
        std::env::set_var("COWIKI_PORT", "4000");
        let config = CowikiConfig::from_env();
        assert_eq!(config.server.port, 4000);
        assert_eq!(config.mcp.port, 8080, "MCP port unchanged");
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

    // ── Config parsing from TOML ──────────────────────────────

    #[test]
    fn test_from_file_basic() {
        let dir = std::env::temp_dir();
        let path = dir.join("cowiki_test_basic.conf");
        std::fs::write(
            &path,
            r#"
[database]
url = "postgres://test:test@localhost/testdb"

[server]
port = 4200
data_dir = "/tmp/cowiki-test"

[llm]
provider = "test-provider"
model = "test-model"
api_key = "sk-test"

[embedder]
dimension = 768
"#,
        )
        .unwrap();

        let config = CowikiConfig::from_file(&path);
        assert_eq!(config.database.url, "postgres://test:test@localhost/testdb");
        assert_eq!(config.server.port, 4200);
        assert_eq!(config.server.data_dir.as_deref(), Some("/tmp/cowiki-test"));
        assert_eq!(config.llm.provider, "test-provider");
        assert_eq!(config.llm.model, "test-model");
        assert_eq!(config.llm.api_key, "sk-test");
        assert_eq!(config.embedder.dimension, 768);
        assert_eq!(config.database.embedding_dimension, 768); // inherit from embedder

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_discover_config_none() {
        let path = discover_config_path(None);
        assert!(path.is_none() || !path.as_ref().unwrap().exists());
    }

    // ── Tool parameter types (shared between crates) ──────────

    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct IngestParams {
        source_type: String,
        content: String,
        filename: Option<String>,
        branch: Option<String>,
    }

    #[test]
    fn test_ingest_params_deserialize() {
        let json = r#"{"source_type":"url","content":"https://example.com","filename":"test.md"}"#;
        let params: IngestParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.source_type, "url");
        assert_eq!(params.filename, Some("test.md".into()));
        assert!(params.branch.is_none());
    }

    #[test]
    fn test_ingest_params_minimal() {
        let json = r#"{"source_type":"text","content":"hello"}"#;
        let params: IngestParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.filename, None);
        assert_eq!(params.branch, None);
    }

    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct SearchParams {
        query: String,
        branch: Option<String>,
        limit: Option<i64>,
    }

    #[test]
    fn test_search_params_with_limit() {
        let json = r#"{"query":"ml","limit":5}"#;
        let p: SearchParams = serde_json::from_str(json).unwrap();
        assert_eq!(p.query, "ml");
        assert_eq!(p.limit, Some(5));
        assert!(p.branch.is_none());
    }

    #[test]
    fn test_search_params_defaults() {
        let p: SearchParams = serde_json::from_str(r#"{"query":"x"}"#).unwrap();
        assert_eq!(p.limit, None);
        assert_eq!(p.branch, None);
    }

    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct WriteParams {
        slug: String,
        body: String,
        title: Option<String>,
        summary: Option<String>,
    }

    #[test]
    fn test_write_params_full() {
        let json = r##"{"slug":"hello","body":"# HI","title":"Hello","summary":"Greet"}"##;
        let p: WriteParams = serde_json::from_str(json).unwrap();
        assert_eq!(p.slug, "hello");
        assert_eq!(p.title, Some("Hello".into()));
    }

    #[test]
    fn test_write_params_minimal() {
        let p: WriteParams = serde_json::from_str(r#"{"slug":"x","body":"y"}"#).unwrap();
        assert_eq!(p.slug, "x");
        assert!(p.title.is_none());
        assert!(p.summary.is_none());
    }

    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct ReviewDecideParams {
        id: uuid::Uuid,
        action: String,
    }

    #[test]
    fn test_review_decide_approve() {
        let id = uuid::Uuid::new_v4();
        let json = format!(r#"{{"id":"{id}","action":"approve"}}"#);
        let p: ReviewDecideParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p.id, id);
        assert_eq!(p.action, "approve");
    }

    #[test]
    fn test_review_decide_reject() {
        let id = uuid::Uuid::new_v4();
        let json = format!(r#"{{"id":"{id}","action":"reject"}}"#);
        let p: ReviewDecideParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p.action, "reject");
    }
}
