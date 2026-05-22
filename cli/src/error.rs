use std::fmt;

#[derive(Debug)]
pub enum CliError {
    Network(reqwest::Error),
    Api { status: u16, message: String },
    Config(String),
    Unexpected(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Network(e) => {
                if e.is_connect() {
                    write!(f, "Cannot connect to server. Is cowiki running? ({e})")
                } else if e.is_timeout() {
                    write!(f, "Request timed out. Try again or check your connection.")
                } else {
                    write!(f, "Network error: {e}")
                }
            }
            CliError::Api { status, message } => {
                write!(f, "API error (HTTP {status}): {message}")
            }
            CliError::Config(msg) => {
                write!(f, "Config error: {msg}")
            }
            CliError::Unexpected(msg) => {
                write!(f, "Unexpected error: {msg}")
            }
        }
    }
}

impl std::error::Error for CliError {}

impl From<reqwest::Error> for CliError {
    fn from(e: reqwest::Error) -> Self {
        CliError::Network(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_404_display() {
        let err = CliError::Api {
            status: 404,
            message: "page not found".into(),
        };
        assert_eq!(format!("{err}"), "API error (HTTP 404): page not found");
    }

    #[test]
    fn test_api_error_500_display() {
        let err = CliError::Api {
            status: 500,
            message: "internal server error".into(),
        };
        assert_eq!(
            format!("{err}"),
            "API error (HTTP 500): internal server error"
        );
    }

    #[test]
    fn test_config_error_display() {
        let err = CliError::Config("missing server url".into());
        assert_eq!(format!("{err}"), "Config error: missing server url");
    }

    #[test]
    fn test_unexpected_error_display() {
        let err = CliError::Unexpected("something went wrong".into());
        assert_eq!(format!("{err}"), "Unexpected error: something went wrong");
    }
}
