use async_trait::async_trait;
use base64::Engine;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};

pub struct UrlExtractor {
    client: reqwest::Client,
}

impl UrlExtractor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (compatible; cowiki-extractor/0.1)")
                .build()
                .unwrap_or_default(),
        }
    }
}

impl Default for UrlExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Score an HTML element for "main content" likelihood.
/// Uses a simplified readability heuristic: text length minus penalty for links.
fn score_element(element: &scraper::ElementRef) -> f64 {
    let text: String = element.text().collect();
    let text_len = text.trim().len() as f64;

    // Count links inside this element
    let link_selector = Selector::parse("a").unwrap();
    let link_count = element.select(&link_selector).count() as f64;

    // Penalty: too many links relative to text suggests nav/sidebar
    let link_penalty = if text_len > 0.0 {
        (link_count / text_len) * 500.0
    } else {
        0.0
    };

    text_len - link_penalty
}

/// Find the main content element in an HTML document.
fn find_main_content(document: &Html) -> Option<String> {
    // Try semantic elements first
    let semantic_selectors = [
        "article",
        "main",
        "[role=main]",
        "#content",
        ".content",
        ".post-content",
        ".article-content",
        ".entry-content",
        "#article",
        ".article",
    ];

    for sel_str in &semantic_selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = document.select(&sel).next() {
                let inner = el.inner_html();
                if inner.trim().len() > 100 {
                    return Some(inner);
                }
            }
        }
    }

    // Fallback: score all block elements and find the highest
    let block_selector = Selector::parse("div, section, article, main").unwrap();
    let mut best_score = 0.0f64;
    let mut best_html: Option<String> = None;

    for el in document.select(&block_selector) {
        let score = score_element(&el);
        if score > best_score {
            let inner = el.inner_html();
            // Only consider elements with substantial text
            if inner.trim().len() > 200 {
                best_score = score;
                best_html = Some(inner);
            }
        }
    }

    // If nothing found, return the body content
    if best_html.is_none() {
        if let Ok(body_sel) = Selector::parse("body") {
            if let Some(body) = document.select(&body_sel).next() {
                return Some(body.inner_html());
            }
        }
    }

    best_html
}

/// Download remote images referenced in markdown and replace with base64 data URIs.
async fn embed_remote_images(client: &reqwest::Client, markdown: &str) -> String {
    use regex::Regex;
    // Match ![...](http://...) or ![...](https://...)
    let re = Regex::new(r"!\[([^\]]*)\]\((https?://[^\)]+)\)").unwrap();

    let mut result = markdown.to_string();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for cap in re.captures_iter(markdown) {
        let full_match = cap.get(0).unwrap();
        let alt_text = cap.get(1).unwrap().as_str();
        let img_url = cap.get(2).unwrap().as_str();

        match download_image_as_data_uri(client, img_url).await {
            Ok(data_uri) => {
                let replacement = format!("![{}]({})", alt_text, data_uri);
                replacements.push((full_match.start(), full_match.end(), replacement));
            }
            Err(e) => {
                tracing::warn!("Failed to download image {}: {:?}", img_url, e);
            }
        }
    }

    // Apply replacements in reverse order (to preserve offsets)
    for (start, end, replacement) in replacements.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }

    result
}

/// Download an image from a URL and return it as a base64 data URI.
async fn download_image_as_data_uri(
    client: &reqwest::Client,
    url: &str,
) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();

    let bytes = resp.bytes().await.map_err(|e| format!("read error: {}", e))?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(format!("image too large: {} bytes", bytes.len()));
    }

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", content_type, b64))
}

#[async_trait]
impl SourceExtractor for UrlExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Url]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::NoAuth
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        let url = &input.content;

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ExtractError::HttpError(format!("failed to fetch {}: {}", url, e)))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ExtractError::HttpError(format!("HTTP {} for {}", status, url)));
        }

        let html_text = resp
            .text()
            .await
            .map_err(|e| ExtractError::HttpError(format!("failed to read body: {}", e)))?;

        // Extract title and main content (keep document scope limited)
        let (title, markdown) = {
            let document = Html::parse_document(&html_text);
            let title = Selector::parse("title")
                .ok()
                .and_then(|sel| document.select(&sel).next())
                .map(|el| el.text().collect::<String>().trim().to_string());

            let md = if let Some(content_html) = find_main_content(&document) {
            html2md::parse_html(&content_html).trim().to_string()
        } else {
            // Fallback: convert entire page
            html2md::parse_html(&html_text).trim().to_string()
        };
            (title, md)
        }; // document dropped here

        // Clean up: collapse blank lines
        let mut cleaned = String::new();
        let mut blank_count = 0usize;
        for line in markdown.lines() {
            if line.trim().is_empty() {
                blank_count += 1;
                if blank_count <= 2 {
                    cleaned.push('\n');
                }
            } else {
                blank_count = 0;
                cleaned.push_str(line);
                cleaned.push('\n');
            }
        }

        // Download and embed remote images as base64 data URIs
        let final_text = embed_remote_images(&self.client, &cleaned).await;

        let short_hash = format!("{:x}", Sha256::digest(url.as_bytes()));
        let suggested = format!("url-{}.md", &short_hash[..8]);

        let mut metadata = ExtractMetadata::default();
        metadata.title = title;
        metadata.source_url = Some(url.clone());

        Ok(ExtractResult {
            text: final_text.trim().to_string(),
            suggested_filename: suggested,
            metadata,
            original_content: url.as_bytes().to_vec(),
        })
    }
}
