use crate::ExtractError;
use std::time::Duration;

const MAX_BODY_BYTES: usize = 10 * 1024 * 1024; // 10 MB
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; cowiki-extractor/0.1; +https://github.com/cowiki)";

/// Fetch the HTML content at `url`, enforcing a 10 MB size limit.
pub async fn fetch_url(url: &str) -> Result<String, ExtractError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;

    let response = client.get(url).send().await?;

    // Check Content-Length header first to avoid downloading oversized responses
    if let Some(content_length) = response.content_length() {
        if content_length as usize > MAX_BODY_BYTES {
            return Err(ExtractError::TooLarge);
        }
    }

    // Stream the body, bailing out if it exceeds the limit
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(ExtractError::TooLarge);
    }

    let html = String::from_utf8_lossy(&bytes).into_owned();
    Ok(html)
}
