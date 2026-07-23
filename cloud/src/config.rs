use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use url::Url;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: Url,
    pub repo_root: PathBuf,
    pub public_origin: Url,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub token_pepper: String,
    pub bind_addr: SocketAddr,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid {name}: {reason}")]
    Invalid { name: &'static str, reason: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_values(std::env::vars())
    }

    pub fn from_values<I>(values: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let values = values.into_iter().collect::<HashMap<_, _>>();
        let required = |name: &'static str| {
            values
                .get(name)
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .ok_or(ConfigError::Missing(name))
        };

        let database_url = parse_url("DATABASE_URL", &required("DATABASE_URL")?)?;
        if database_url.scheme() != "postgres" && database_url.scheme() != "postgresql" {
            return Err(ConfigError::Invalid {
                name: "DATABASE_URL",
                reason: "CoWiki Cloud requires PostgreSQL".to_string(),
            });
        }

        let repo_root = PathBuf::from(required("COWIKI_REPO_ROOT")?);
        if !repo_root.is_absolute() {
            return Err(ConfigError::Invalid {
                name: "COWIKI_REPO_ROOT",
                reason: "path must be absolute".to_string(),
            });
        }

        let public_origin = parse_url("COWIKI_PUBLIC_ORIGIN", &required("COWIKI_PUBLIC_ORIGIN")?)?;
        if !matches!(public_origin.scheme(), "http" | "https")
            || public_origin.host().is_none()
            || !public_origin.username().is_empty()
            || public_origin.password().is_some()
            || !matches!(public_origin.path(), "" | "/")
            || public_origin.query().is_some()
            || public_origin.fragment().is_some()
        {
            return Err(ConfigError::Invalid {
                name: "COWIKI_PUBLIC_ORIGIN",
                reason: "origin must be a root http(s) URL without credentials, query, or fragment"
                    .to_string(),
            });
        }

        let token_pepper = required("COWIKI_TOKEN_PEPPER")?;
        if token_pepper.len() < 32 {
            return Err(ConfigError::Invalid {
                name: "COWIKI_TOKEN_PEPPER",
                reason: "value must be at least 32 bytes".to_string(),
            });
        }

        let bind_addr = values
            .get("COWIKI_BIND_ADDR")
            .map(String::as_str)
            .unwrap_or("0.0.0.0:8787")
            .parse::<SocketAddr>()
            .map_err(|error| ConfigError::Invalid {
                name: "COWIKI_BIND_ADDR",
                reason: error.to_string(),
            })?;

        Ok(Self {
            database_url,
            repo_root,
            public_origin,
            github_client_id: required("GITHUB_CLIENT_ID")?,
            github_client_secret: required("GITHUB_CLIENT_SECRET")?,
            token_pepper,
            bind_addr,
        })
    }
}

fn parse_url(name: &'static str, value: &str) -> Result<Url, ConfigError> {
    Url::parse(value).map_err(|error| ConfigError::Invalid {
        name,
        reason: error.to_string(),
    })
}
