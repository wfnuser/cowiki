use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

/// cowiki server
#[derive(Parser, Debug)]
#[command(name = "cowiki-server")]
pub struct CliArgs {
    /// Path to cowiki.conf configuration file
    #[arg(short = 'c', long = "config", env = "COWIKI_CONFIG")]
    pub config: Option<PathBuf>,
}

/// Raw TOML config file structure (all fields optional, env vars fill gaps)
#[derive(Debug, Clone, Deserialize, Default)]
struct TomlConfig {
    database: Option<TomlDatabase>,
    server: Option<TomlServer>,
    llm: Option<TomlLlm>,
    embedder: Option<TomlEmbedder>,
    github: Option<TomlGithub>,
    frontend: Option<TomlFrontend>,
}

#[derive(Debug, Clone, Deserialize)]
struct TomlDatabase {
    url: Option<String>,
    embedding_dimension: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct TomlServer {
    port: Option<u16>,
    data_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TomlLlm {
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct TomlEmbedder {
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    dimension: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct TomlGithub {
    client_id: Option<String>,
    client_secret: Option<String>,
    redirect_uri: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TomlFrontend {
    url: Option<String>,
}

// ── Public config types ──

#[derive(Debug, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub llm: LlmConfig,
    pub embedder: EmbedderConfig,
    pub auth: AuthConfig,
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
pub struct AuthConfig {
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_redirect_uri: String,
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

// ── Helpers ──

/// Discover config file path
fn discover_config_path(cli_path: Option<PathBuf>) -> Option<PathBuf> {
    // 1. CLI arg
    if let Some(p) = cli_path {
        if p.exists() {
            tracing::info!("using config from CLI: {}", p.display());
            return Some(p);
        }
        tracing::warn!("config file not found: {}", p.display());
    }

    // 2. COWIKI_CONFIG env var
    if let Ok(env_path) = std::env::var("COWIKI_CONFIG") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            tracing::info!("using config from COWIKI_CONFIG: {}", p.display());
            return Some(p);
        }
    }

    // 3. ./cowiki.conf
    let local = PathBuf::from("cowiki.conf");
    if local.exists() {
        tracing::info!("using config: ./cowiki.conf");
        return Some(local);
    }

    // 4. ~/.cowiki/cowiki.conf
    if let Some(home) = dirs_next_home() {
        let user_conf = home.join(".cowiki").join("cowiki.conf");
        if user_conf.exists() {
            tracing::info!("using config: {}", user_conf.display());
            return Some(user_conf);
        }
    }

    None
}

fn dirs_next_home() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "linux"))]
    {
        dirs::home_dir()
    }
}

impl Config {
    /// Load config: tries cowiki.conf first, falls back to env vars.
    /// Use `CliArgs::parse()` to get CLI args for --config.
    pub fn load(args: Option<CliArgs>) -> Self {
        let cli_config = args.and_then(|a| a.config);
        let config_path = discover_config_path(cli_config);

        match config_path {
            Some(path) => Self::from_config(&path),
            None => {
                tracing::info!("no cowiki.conf found, using environment variables");
                Self::from_env()
            }
        }
    }

    pub fn from_config(path: &PathBuf) -> Self {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read config file {}: {e}", path.display()));

        let toml: TomlConfig = toml::from_str(&content)
            .unwrap_or_else(|e| panic!("failed to parse config file {}: {e}", path.display()));

        Self {
            database: {
                let url = toml
                    .database
                    .as_ref()
                    .and_then(|d| d.url.clone())
                    .unwrap_or_else(|| panic!("database.url is required in config file"));

                let embedding_dimension = toml
                    .database
                    .as_ref()
                    .and_then(|d| d.embedding_dimension)
                    .or_else(|| toml.embedder.as_ref().and_then(|e| e.dimension))
                    .unwrap_or(1536);

                DatabaseConfig {
                    url,
                    embedding_dimension,
                }
            },

            server: ServerConfig {
                port: toml.server.as_ref().and_then(|s| s.port).unwrap_or(3000),
                data_dir: toml
                    .server
                    .as_ref()
                    .and_then(|s| s.data_dir.clone())
                    .unwrap_or_else(|| "./data".into()),
            },

            llm: {
                let api_key = toml
                    .llm
                    .as_ref()
                    .and_then(|v| v.api_key.clone())
                    .unwrap_or_default();

                let api_base = toml
                    .llm
                    .as_ref()
                    .and_then(|v| v.api_base.clone())
                    .unwrap_or_else(|| "https://api.openai.com/v1".into());

                LlmConfig {
                    provider: toml
                        .llm
                        .as_ref()
                        .and_then(|v| v.provider.clone())
                        .unwrap_or_else(|| "openai".into()),
                    model: toml
                        .llm
                        .as_ref()
                        .and_then(|v| v.model.clone())
                        .unwrap_or_else(|| "gpt-4o-mini".into()),
                    api_key,
                    api_base,
                    temperature: toml.llm.as_ref().and_then(|v| v.temperature).unwrap_or(0.3),
                    max_tokens: toml.llm.as_ref().and_then(|v| v.max_tokens),
                }
            },

            embedder: {
                let api_key = toml
                    .embedder
                    .as_ref()
                    .and_then(|e| e.api_key.clone())
                    .filter(|k| !k.is_empty())
                    .or_else(|| toml.llm.as_ref().and_then(|l| l.api_key.clone()))
                    .unwrap_or_default();

                let api_base = toml
                    .embedder
                    .as_ref()
                    .and_then(|e| e.api_base.clone())
                    .filter(|u| !u.is_empty())
                    .or_else(|| toml.llm.as_ref().and_then(|l| l.api_base.clone()))
                    .unwrap_or_else(|| "https://api.openai.com/v1".into());

                EmbedderConfig {
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
                    api_key,
                    api_base,
                    dimension: toml
                        .embedder
                        .as_ref()
                        .and_then(|e| e.dimension)
                        .unwrap_or(1536),
                }
            },

            auth: AuthConfig {
                github_client_id: toml
                    .github
                    .as_ref()
                    .and_then(|g| g.client_id.clone())
                    .unwrap_or_default(),
                github_client_secret: toml
                    .github
                    .as_ref()
                    .and_then(|g| g.client_secret.clone())
                    .unwrap_or_default(),
                github_redirect_uri: toml
                    .github
                    .as_ref()
                    .and_then(|g| g.redirect_uri.clone())
                    .unwrap_or_else(|| "http://localhost:3000/api/auth/github/callback".into()),
            },

            frontend_url: toml
                .frontend
                .as_ref()
                .and_then(|f| f.url.clone())
                .unwrap_or_else(|| "http://localhost:5173".into()),
        }
    }

    /// Legacy: load purely from environment variables (backward compat)
    pub fn from_env() -> Self {
        let llm_api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required");
        let llm_api_base =
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());

        let emb_api_key = std::env::var("COWIKI_EMBEDDER_API_KEY")
            .ok()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_else(|| llm_api_key.clone());

        let emb_api_base = std::env::var("COWIKI_EMBEDDER_BASE_URL")
            .ok()
            .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
            .unwrap_or_else(|| llm_api_base.clone());

        let embedder_dimension: u32 = std::env::var("COWIKI_EMBEDDER_DIMENSION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1536);

        Self {
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL").expect("DATABASE_URL required"),
                embedding_dimension: std::env::var("COWIKI_DATABASE_EMBEDDING_DIMENSION")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(embedder_dimension),
            },
            server: ServerConfig {
                port: std::env::var("COWIKI_PORT")
                    .unwrap_or_else(|_| "3000".into())
                    .parse()
                    .unwrap_or(3000),
                data_dir: std::env::var("COWIKI_DATA_DIR").unwrap_or_else(|_| "./data".into()),
            },
            llm: LlmConfig {
                provider: std::env::var("COWIKI_LLM_PROVIDER")
                    .or_else(|_| std::env::var("COWIKI_VLM_PROVIDER"))
                    .unwrap_or_else(|_| "openai".into()),
                model: std::env::var("COWIKI_LLM_MODEL")
                    .or_else(|_| std::env::var("COWIKI_VLM_MODEL"))
                    .unwrap_or_else(|_| "gpt-4o-mini".into()),
                api_key: llm_api_key,
                api_base: llm_api_base,
                temperature: std::env::var("COWIKI_LLM_TEMPERATURE")
                    .or_else(|_| std::env::var("COWIKI_VLM_TEMPERATURE"))
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.3),
                max_tokens: std::env::var("COWIKI_LLM_MAX_TOKENS")
                    .or_else(|_| std::env::var("COWIKI_VLM_MAX_TOKENS"))
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
                dimension: embedder_dimension,
            },
            auth: AuthConfig {
                github_client_id: std::env::var("GITHUB_CLIENT_ID").unwrap_or_default(),
                github_client_secret: std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default(),
                github_redirect_uri: std::env::var("GITHUB_REDIRECT_URI")
                    .unwrap_or_else(|_| "http://localhost:3000/api/auth/github/callback".into()),
            },
            frontend_url: std::env::var("FRONTEND_URL")
                .unwrap_or_else(|_| "http://localhost:5173".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn write_toml(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn temp_conf_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static CNT: AtomicU32 = AtomicU32::new(0);
        let n = CNT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("cowiki_test_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_full_config_all_fields() {
        let dir = temp_conf_dir();
        let conf = write_toml(&dir, "cowiki.conf", r#"[database]
url = "postgres://test:test@localhost/test"
embedding_dimension = 768

[server]
port = 8080
data_dir = "/tmp/cowiki"

[llm]
provider = "openai"
model = "gpt-4o"
api_key = "sk-llm-key"
api_base = "https://llm.example.com/v1"
temperature = 0.7
max_tokens = 8192

[embedder]
provider = "openai"
model = "text-embedding-3-large"
api_key = "sk-emb-key"
api_base = "https://emb.example.com/v1"
dimension = 3072

[github]
client_id = "gh-client-id"
client_secret = "gh-secret"
redirect_uri = "https://example.com/callback"

[frontend]
url = "https://app.example.com"
"#);

        let config = Config::from_config(&conf);

        assert_eq!(config.database.url, "postgres://test:test@localhost/test");
        assert_eq!(config.database.embedding_dimension, 768);
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.data_dir, "/tmp/cowiki");

        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.llm.model, "gpt-4o");
        assert_eq!(config.llm.api_key, "sk-llm-key");
        assert_eq!(config.llm.api_base, "https://llm.example.com/v1");
        assert!((config.llm.temperature - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.llm.max_tokens, Some(8192));

        assert_eq!(config.embedder.provider, "openai");
        assert_eq!(config.embedder.model, "text-embedding-3-large");
        assert_eq!(config.embedder.api_key, "sk-emb-key");
        assert_eq!(config.embedder.api_base, "https://emb.example.com/v1");
        assert_eq!(config.embedder.dimension, 3072);

        assert_eq!(config.auth.github_client_id, "gh-client-id");
        assert_eq!(config.auth.github_client_secret, "gh-secret");
        assert_eq!(config.auth.github_redirect_uri, "https://example.com/callback");
        assert_eq!(config.frontend_url, "https://app.example.com");
    }

    #[test]
    fn test_minimal_config_defaults() {
        let dir = temp_conf_dir();
        let conf = write_toml(&dir, "cowiki.conf", r#"[database]
url = "postgres://localhost/test"
"#);

        let config = Config::from_config(&conf);

        assert_eq!(config.database.url, "postgres://localhost/test");
        assert_eq!(config.database.embedding_dimension, 1536);
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.data_dir, "./data");

        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.llm.model, "gpt-4o-mini");
        assert_eq!(config.llm.api_key, "");
        assert_eq!(config.llm.api_base, "https://api.openai.com/v1");
        assert!((config.llm.temperature - 0.3).abs() < f64::EPSILON);
        assert_eq!(config.llm.max_tokens, None);

        assert_eq!(config.embedder.provider, "openai");
        assert_eq!(config.embedder.model, "text-embedding-3-small");
        assert_eq!(config.embedder.api_key, "");
        assert_eq!(config.embedder.api_base, "https://api.openai.com/v1");
        assert_eq!(config.embedder.dimension, 1536);

        assert_eq!(config.auth.github_client_id, "");
        assert_eq!(config.auth.github_client_secret, "");
        assert_eq!(config.auth.github_redirect_uri, "http://localhost:3000/api/auth/github/callback");
        assert_eq!(config.frontend_url, "http://localhost:5173");
    }

    #[test]
    #[should_panic(expected = "database.url is required")]
    fn test_missing_database_url_panics() {
        let dir = temp_conf_dir();
        let conf = write_toml(&dir, "cowiki.conf", "[server]\nport = 9999\n");
        Config::from_config(&conf);
    }

    #[test]
    fn test_embedding_dimension_falls_back_to_embedder() {
        let dir = temp_conf_dir();
        let conf = write_toml(&dir, "cowiki.conf", r#"[database]
url = "postgres://localhost/test"

[embedder]
dimension = 2048
"#);
        let config = Config::from_config(&conf);
        assert_eq!(config.database.embedding_dimension, 2048);
    }

    #[test]
    fn test_embedder_falls_back_to_llm_credentials() {
        let dir = temp_conf_dir();
        let conf = write_toml(&dir, "cowiki.conf", r#"[database]
url = "postgres://localhost/test"

[llm]
api_key = "sk-shared"
api_base = "https://shared.example.com/v1"

[embedder]
model = "custom-model"
"#);
        let config = Config::from_config(&conf);
        assert_eq!(config.embedder.api_key, "sk-shared");
        assert_eq!(config.embedder.api_base, "https://shared.example.com/v1");
        assert_eq!(config.embedder.model, "custom-model");
    }

    #[test]
    fn test_embedder_own_credentials_override_llm() {
        let dir = temp_conf_dir();
        let conf = write_toml(&dir, "cowiki.conf", r#"[database]
url = "postgres://localhost/test"

[llm]
api_key = "sk-llm"
api_base = "https://llm.example.com/v1"

[embedder]
api_key = "sk-emb"
api_base = "https://emb.example.com/v1"
"#);
        let config = Config::from_config(&conf);
        assert_eq!(config.embedder.api_key, "sk-emb");
        assert_eq!(config.embedder.api_base, "https://emb.example.com/v1");
        assert_eq!(config.llm.api_key, "sk-llm");
        assert_eq!(config.llm.api_base, "https://llm.example.com/v1");
    }
}
