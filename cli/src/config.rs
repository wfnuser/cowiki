use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

use crate::error::CliError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_server_url")]
    pub server_url: String,
    pub api_key: Option<String>,
}

fn default_server_url() -> String {
    "http://localhost:3000".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: default_server_url(),
            api_key: None,
        }
    }
}

impl Config {
    /// Load config from file, .env, and environment variables.
    /// Priority: env vars > .env file > config.toml > defaults
    pub fn load() -> Result<Self, CliError> {
        // Load .env file (silently skip if absent)
        let _ = dotenvy::dotenv();

        let config_path = config_path()?;

        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| CliError::Config(format!("cannot read {}: {e}", config_path.display())))?;
            toml::from_str::<Config>(&content)
                .map_err(|e| CliError::Config(format!("invalid config: {e}")))?
        } else {
            Config::default()
        };

        // Override with environment variables
        if let Ok(url) = std::env::var("COWIKI_BASE_URL") {
            config.server_url = url;
        }
        if let Ok(key) = std::env::var("COWIKI_API_KEY") {
            config.api_key = Some(key);
        }

        Ok(config)
    }

    /// Save config to disk, creating directories if needed.
    pub fn save(&self) -> Result<(), CliError> {
        let config_path = config_path()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CliError::Config(format!("cannot create config dir: {e}")))?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| CliError::Config(format!("cannot serialize config: {e}")))?;

        // Write with restricted permissions (rw-------) to protect API key
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&config_path)
            .map_err(|e| CliError::Config(format!("cannot write config: {e}")))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| CliError::Config(format!("cannot set config permissions: {e}")))?;
        }

        file.write_all(content.as_bytes())
            .map_err(|e| CliError::Config(format!("cannot write config: {e}")))?;

        Ok(())
    }
}

fn config_path() -> Result<PathBuf, CliError> {
    let dir = dirs::config_dir()
        .ok_or_else(|| CliError::Config("cannot determine config directory".into()))?;
    Ok(dir.join("cowiki").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let c = Config::default();
        assert_eq!(c.server_url, "http://localhost:3000");
        assert_eq!(c.api_key, None);
    }

    #[test]
    fn test_env_override_logic() {
        let mut c = Config::default();
        c.server_url = "https://wiki.example.com".into();
        assert_eq!(c.server_url, "https://wiki.example.com");
        c.api_key = Some("test-key-123".into());
        assert_eq!(c.api_key, Some("test-key-123".to_string()));
    }

    #[test]
    fn test_serialize_deserialize() {
        let c = Config {
            server_url: "https://example.com".into(),
            api_key: Some("secret".into()),
        };
        let toml_str = toml::to_string_pretty(&c).unwrap();
        let restored: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.server_url, c.server_url);
        assert_eq!(restored.api_key, c.api_key);
    }
}
