use htmlmd_core::{convert, ConversionOptions};
use std::io::Read;
use std::time::Duration;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use url::Url;

pub const EXTRACTOR_NAME: &str = "cowiki-html-to-markdown-v1";
const MAX_HTML_BYTES: u64 = 5 * 1024 * 1024;
const MAX_MARKDOWN_BYTES: u64 = 2 * 1024 * 1024;

pub struct WebSourceSnapshot {
    pub title: String,
    pub source_url: String,
    pub captured_at: String,
    pub markdown: String,
}

pub fn fetch_markdown_snapshot(value: &str) -> Result<WebSourceSnapshot, String> {
    let requested_url = validate_url(value)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("CoWiki/0.1 web-source-ingest")
        .build()
        .map_err(|error| format!("cannot prepare web request: {error}"))?;
    let response = client
        .get(requested_url)
        .send()
        .map_err(|error| format!("cannot fetch web source: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "web source returned HTTP {}",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_HTML_BYTES)
    {
        return Err(format!(
            "web source is too large (maximum {MAX_HTML_BYTES} bytes)"
        ));
    }
    if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
        let content_type = content_type
            .to_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !content_type.starts_with("text/html")
            && !content_type.starts_with("application/xhtml+xml")
        {
            return Err("URL does not point to an HTML page".to_string());
        }
    }

    let final_url = response.url().clone();
    let mut html = Vec::new();
    response
        .take(MAX_HTML_BYTES + 1)
        .read_to_end(&mut html)
        .map_err(|error| format!("cannot read web source: {error}"))?;
    if html.len() as u64 > MAX_HTML_BYTES {
        return Err(format!(
            "web source is too large (maximum {MAX_HTML_BYTES} bytes)"
        ));
    }

    let html = String::from_utf8_lossy(&html);
    let mut options = ConversionOptions::gfm();
    options.cleanup.metadata.title = true;
    options.cleanup.base_url = Some(final_url.to_string());
    options.cleanup.main_content_selector = Some("article, main, [role=main]".to_string());
    options.cleanup.remove_tags.extend(
        ["nav", "header", "footer", "aside"]
            .into_iter()
            .map(str::to_string),
    );
    options.limits.max_input_bytes = MAX_HTML_BYTES;
    options.limits.max_output_bytes = MAX_MARKDOWN_BYTES;
    options.limits.max_node_count = 200_000;
    options.limits.max_attribute_len = 100_000;
    options.strict = true;

    let converted = convert(&html, &options)
        .map_err(|error| format!("cannot convert web source to Markdown: {error}"))?;
    let markdown = converted.markdown.trim().to_string();
    if markdown.is_empty() {
        return Err("web source contains no readable content".to_string());
    }
    let title = converted
        .title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| final_url.host_str().unwrap_or("Web source").to_string());
    let captured_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| format!("cannot format capture time: {error}"))?;

    Ok(WebSourceSnapshot {
        title,
        source_url: final_url.to_string(),
        captured_at,
        markdown,
    })
}

fn validate_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value.trim()).map_err(|_| "Enter a valid HTTP(S) URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Enter a valid HTTP(S) URL without embedded credentials".to_string());
    }
    Ok(url)
}
