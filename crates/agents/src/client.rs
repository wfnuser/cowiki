use reqwest::Client as HttpClient;
use std::time::Duration;
use url::Url;

use crate::error::AgentError;
use crate::protocol::{AgentRequest, AgentResponse};

/// HTTP client for communicating with remote agent harnesses.
///
/// Each harness is a separate HTTP/gRPC process exposing a `/agent/run` endpoint.
/// The client sends `AgentRequest` and receives `AgentResponse`.
///
/// # URL Validation
/// All harness endpoints are validated before requests are made:
/// - Only `http://` and `https://` schemes are allowed
/// - MVP: endpoints are restricted to localhost only
#[derive(Clone)]
pub struct AgentClient {
    http: HttpClient,
}

impl AgentClient {
    pub fn new() -> Self {
        Self {
            http: HttpClient::new(),
        }
    }

    /// Create a client with a custom timeout.
    ///
    /// Returns an error if the HTTP client cannot be built
    /// (e.g., missing TLS backend in a stripped container).
    pub fn with_timeout(timeout_secs: u64) -> Result<Self, AgentError> {
        Ok(Self {
            http: HttpClient::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .map_err(|e| {
                    AgentError::InvalidConfig(format!("failed to build HTTP client: {e}"))
                })?,
        })
    }

    /// Validate a harness URL before making requests.
    ///
    /// Ensures the URL uses http/https and points to localhost (MVP restriction).
    fn validate_harness_url(endpoint: &str) -> Result<Url, AgentError> {
        let url = Url::parse(endpoint)
            .map_err(|e| AgentError::InvalidUrl(format!("invalid harness URL '{endpoint}': {e}")))?;

        // Allow only http/https
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(AgentError::InvalidUrl(format!(
                "disallowed URL scheme '{}' in harness endpoint",
                url.scheme()
            )));
        }

        // MVP: restrict to localhost
        if let Some(host) = url.host_str() {
            if host != "localhost" && host != "127.0.0.1" && host != "[::1]" {
                return Err(AgentError::InvalidUrl(format!(
                    "harness endpoints must be localhost, got '{host}'"
                )));
            }
        }

        Ok(url)
    }

    /// Send a request to an agent harness endpoint and await the response.
    ///
    /// # Arguments
    /// * `endpoint` - Full URL of the harness, e.g. "http://localhost:9100/agent/run"
    /// * `request` - The `AgentRequest` payload
    ///
    /// # Errors
    /// Returns `AgentError::InvalidUrl` if the endpoint is not a valid localhost URL.
    /// Returns `AgentError::Timeout` if the request times out.
    /// Returns `AgentError::HttpStatus` if the harness returns a non-2xx status.
    /// Returns `AgentError::Protocol` if the response cannot be parsed.
    pub async fn run(
        &self,
        endpoint: &str,
        request: AgentRequest,
    ) -> Result<AgentResponse, AgentError> {
        // Validate URL before making any request
        let _validated = Self::validate_harness_url(endpoint)?;

        tracing::info!(
            task_type = %request.task_type,
            endpoint = %endpoint,
            "dispatching agent request"
        );

        let resp = self
            .http
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AgentError::Timeout
                } else {
                    AgentError::Transport(e.to_string())
                }
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(%status, %body, "agent harness returned error");
            return Err(AgentError::HttpStatus(status.as_u16()));
        }

        let response: AgentResponse = resp.json().await.map_err(|e| {
            AgentError::Protocol(format!("failed to parse response: {e}"))
        })?;

        if !response.success {
            tracing::warn!(
                error = ?response.error,
                rounds = response.rounds,
                "agent reported failure"
            );
        } else {
            tracing::info!(
                rounds = response.rounds,
                input_tokens = response.usage.as_ref().map(|u| u.input_tokens),
                output_tokens = response.usage.as_ref().map(|u| u.output_tokens),
                "agent completed successfully"
            );
        }

        Ok(response)
    }

    /// Check if a harness is healthy.
    ///
    /// Derives the health URL by replacing the last path segment of the
    /// harness endpoint with `/health`. If the endpoint has a non-standard
    /// path structure, this method returns `Ok(false)`.
    pub async fn health_check(&self, endpoint: &str) -> Result<bool, AgentError> {
        // Validate and parse the endpoint URL
        let url = Self::validate_harness_url(endpoint)?;

        // Build health URL by replacing the last path segment with "health"
        let health_url = build_health_url(&url);

        match self.http.get(&health_url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => {
                tracing::debug!(%health_url, error = %e, "health check failed");
                Ok(false)
            }
        }
    }
}

impl Default for AgentClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a health-check URL from a harness agent/run endpoint.
///
/// Replaces the last path segment with "health".
/// E.g., `http://localhost:9100/agent/run` → `http://localhost:9100/agent/health`
fn build_health_url(endpoint: &Url) -> String {
    let mut url = endpoint.clone();
    if let Some(mut segments) = url.path_segments().map(|s| s.collect::<Vec<_>>()) {
        if !segments.is_empty() {
            segments.pop(); // remove "run"
            segments.push("health");
        }
        let new_path = format!("/{}", segments.join("/"));
        url.set_path(&new_path);
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_localhost_url() {
        assert!(AgentClient::validate_harness_url("http://localhost:9100/agent/run").is_ok());
        assert!(AgentClient::validate_harness_url("http://127.0.0.1:9100/agent/run").is_ok());
    }

    #[test]
    fn test_reject_non_localhost() {
        assert!(AgentClient::validate_harness_url("http://example.com:9100/agent/run").is_err());
        assert!(AgentClient::validate_harness_url("http://10.0.0.1:9100/agent/run").is_err());
    }

    #[test]
    fn test_reject_non_http_scheme() {
        assert!(AgentClient::validate_harness_url("file:///etc/passwd").is_err());
        assert!(AgentClient::validate_harness_url("ftp://localhost:21/agent/run").is_err());
    }

    #[test]
    fn test_health_url_from_agent_run() {
        let url = Url::parse("http://localhost:9100/agent/run").unwrap();
        assert_eq!(build_health_url(&url), "http://localhost:9100/agent/health");
    }

    #[test]
    fn test_health_url_root_path() {
        let url = Url::parse("http://localhost:9100/run").unwrap();
        assert_eq!(build_health_url(&url), "http://localhost:9100/health");
    }
}
