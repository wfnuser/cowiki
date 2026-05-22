use crate::error::CliError;
use crate::types::*;

pub struct CowikiClient {
    server_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl CowikiClient {
    pub fn new(server_url: String, api_key: Option<String>) -> Self {
        // Warn if Bearer token would be sent over non-HTTPS remote connection
        let is_local = server_url.starts_with("http://localhost")
            || server_url.starts_with("http://127.")
            || server_url.starts_with("http://[::1]");
        let is_https = server_url.starts_with("https://");
        if !is_local && !is_https && api_key.is_some() {
            eprintln!(
                "\u{26a0}\u{fe0f}  WARNING: Server URL '{}' is not HTTPS. \
                 Your API key will be sent in cleartext.",
                server_url
            );
        }

        Self {
            server_url,
            api_key,
            client: reqwest::Client::new(),
        }
    }

    fn auth_header(&self) -> Option<String> {
        self.api_key
            .as_ref()
            .map(|key| format!("Bearer {key}"))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, CliError> {
        let mut req = self.client.get(url);
        if let Some(h) = self.auth_header() {
            req = req.header("Authorization", &h);
        }
        let resp = req.send().await?;
        handle_response(resp).await
    }

    async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T, CliError> {
        let mut req = self.client.post(url).json(body);
        if let Some(h) = self.auth_header() {
            req = req.header("Authorization", &h);
        }
        let resp = req.send().await?;
        handle_response(resp).await
    }

    // ── Auth ──────────────────────────────────────────

    pub async fn register(
        &self,
        name: &str,
        email: Option<&str>,
    ) -> Result<AuthResponse, CliError> {
        let body = RegisterRequest {
            name: name.into(),
            email: email.map(|s| s.into()),
        };
        self.post_json(&format!("{}/api/auth/register", self.server_url), &body)
            .await
    }

    pub async fn get_me(&self) -> Result<UserInfo, CliError> {
        self.get_json(&format!("{}/api/auth/me", self.server_url))
            .await
    }

    // ── Pages ─────────────────────────────────────────

    pub async fn list_pages(&self, branch: &str) -> Result<Vec<PageMeta>, CliError> {
        self.get_json(&format!(
            "{}/api/pages?branch={}",
            self.server_url,
            urlencoding(branch)
        ))
        .await
    }

    pub async fn get_page(&self, slug: &str, branch: &str) -> Result<PageFull, CliError> {
        self.get_json(&format!(
            "{}/api/pages/{}?branch={}",
            self.server_url,
            urlencoding(slug),
            urlencoding(branch)
        ))
        .await
    }

    pub async fn write_page(&self, req: WritePageRequest) -> Result<WriteResponse, CliError> {
        self.post_json(&format!("{}/api/pages", self.server_url), &req)
            .await
    }

    // ── Ingest ────────────────────────────────────────

    pub async fn ingest(&self, req: IngestRequest) -> Result<IngestResponse, CliError> {
        self.post_json(&format!("{}/api/ingest", self.server_url), &req)
            .await
    }

    // ── Compile ───────────────────────────────────────

    pub async fn compile(&self, req: CompileRequest) -> Result<CompileResponse, CliError> {
        self.post_json(&format!("{}/api/compile", self.server_url), &req)
            .await
    }

    // ── Search ────────────────────────────────────────

    pub async fn search(
        &self,
        query: &str,
        limit: Option<u32>,
        branch: Option<&str>,
    ) -> Result<Vec<SearchResult>, CliError> {
        let branch = branch.unwrap_or("main");
        let limit = limit.unwrap_or(10);
        self.get_json(&format!(
            "{}/api/search?q={}&limit={}&branch={}",
            self.server_url,
            urlencoding(query),
            limit,
            urlencoding(branch)
        ))
        .await
    }

    // ── Submit ────────────────────────────────────────

    pub async fn submit(&self, req: SubmitRequest) -> Result<SubmitResponse, CliError> {
        self.post_json(&format!("{}/api/submit", self.server_url), &req)
            .await
    }

    // ── Reviews ───────────────────────────────────────

    pub async fn list_reviews(&self) -> Result<Vec<Submission>, CliError> {
        self.get_json(&format!("{}/api/reviews", self.server_url))
            .await
    }

    pub async fn get_review(&self, id: &str) -> Result<ReviewDetail, CliError> {
        self.get_json(&format!("{}/api/reviews/{id}", self.server_url))
            .await
    }

    pub async fn approve_review(&self, id: &str) -> Result<(), CliError> {
        let body = serde_json::json!({"action": "approve"});
        let _: serde_json::Value = self
            .post_json(&format!("{}/api/reviews/{id}", self.server_url), &body)
            .await?;
        Ok(())
    }

    pub async fn reject_review(&self, id: &str) -> Result<(), CliError> {
        let body = serde_json::json!({"action": "reject"});
        let _: serde_json::Value = self
            .post_json(&format!("{}/api/reviews/{id}", self.server_url), &body)
            .await?;
        Ok(())
    }
}

/// URL-encode a string for query parameters (RFC 3986 percent-encoding).
fn urlencoding(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => result.push(b as char),
            _ => result.push_str(&format!("%{:02X}", b)),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencoding_simple() {
        assert_eq!(urlencoding("hello"), "hello");
    }

    #[test]
    fn test_urlencoding_spaces() {
        assert_eq!(urlencoding("hello world"), "hello%20world");
    }

    #[test]
    fn test_urlencoding_special_chars() {
        let encoded = urlencoding("hello#world&foo?bar=1");
        assert_eq!(encoded, "hello%23world%26foo%3Fbar%3D1");
    }

    #[test]
    fn test_urlencoding_slash() {
        // Slashes are unreserved in path segments but we encode them for safety
        let encoded = urlencoding("user/723666c1-b756-4b81");
        assert!(encoded.contains("%2F"));
    }

    #[test]
    fn test_urlencoding_chinese() {
        let encoded = urlencoding("中文");
        assert!(encoded.contains('%'));
        assert!(!encoded.contains('中'));
    }

    #[test]
    fn test_urlencoding_empty() {
        assert_eq!(urlencoding(""), "");
    }

    #[test]
    fn test_urlencoding_percent() {
        let encoded = urlencoding("50%");
        assert_eq!(encoded, "50%25");
    }
}

async fn handle_response<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
) -> Result<T, CliError> {
    let status = resp.status();
    if status.is_success() {
        resp.json::<T>().await.map_err(|e| {
            CliError::Unexpected(format!("failed to parse response: {e}"))
        })
    } else {
        let message = resp
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".into());
        Err(CliError::Api {
            status: status.as_u16(),
            message,
        })
    }
}
