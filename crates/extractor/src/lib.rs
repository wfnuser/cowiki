pub mod clean;
pub mod fetch;
pub mod metadata;
pub mod readability_extract;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractResult {
    pub url: String,
    pub title: String,
    pub text: String,
    pub metadata: Metadata,
    pub headings: Vec<Heading>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub description: Option<String>,
    pub author: Option<String>,
    pub domain: String,
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("fetch error: {0}")]
    Fetch(#[from] reqwest::Error),
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("extraction error: {0}")]
    Extract(String),
    #[error("content too large")]
    TooLarge,
}

/// Orchestrates: fetch → metadata → readability → clean
pub async fn extract_url(target_url: &str) -> Result<ExtractResult, ExtractError> {
    // Parse URL to extract domain
    let parsed = url::Url::parse(target_url)?;
    let domain = parsed.host_str().unwrap_or("").to_string();

    tracing::debug!(url = target_url, "fetching URL");
    let html = fetch::fetch_url(target_url).await?;

    let title = metadata::extract_title(&html);
    let meta = metadata::extract_metadata(&html, &domain);
    let headings = metadata::extract_headings(&html);

    let raw_text = readability_extract::extract_content(&html, target_url);
    let text = clean::clean_text(&raw_text);

    Ok(ExtractResult {
        url: target_url.to_string(),
        title,
        text,
        metadata: meta,
        headings,
    })
}
