use async_trait::async_trait;
use feed_rs::parser;

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};

pub struct RssExtractor {
    client: reqwest::Client,
}

impl RssExtractor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for RssExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SourceExtractor for RssExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Rss]
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
            .map_err(|e| ExtractError::HttpError(format!("failed to fetch feed {}: {}", url, e)))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ExtractError::HttpError(format!(
                "HTTP {} for feed {}",
                status, url
            )));
        }

        let body = resp
            .bytes()
            .await
            .map_err(|e| ExtractError::HttpError(format!("failed to read feed body: {}", e)))?;

        let feed = parser::parse(&body[..])
            .map_err(|e| ExtractError::ParseError(format!("failed to parse feed: {}", e)))?;

        let mut markdown = String::new();
        let mut metadata = ExtractMetadata::default();

        // Feed title
        let feed_title = feed.title.map(|t| t.content).unwrap_or_else(|| "Untitled Feed".to_string());
        markdown.push_str(&format!("# {}\n\n", feed_title));
        metadata.title = Some(feed_title.clone());

        // Feed description
        if let Some(subtitle) = &feed.description {
            markdown.push_str(&format!("{}\n\n", subtitle.content));
        }

        // Links
        let entry_count = feed.entries.len();
        markdown.push_str(&format!("**{} entries**\n\n", entry_count));

        // Entries
        for entry in &feed.entries {
            let entry_title = entry
                .title
                .as_ref()
                .map(|t| t.content.clone())
                .unwrap_or_else(|| "Untitled".to_string());

            let date = entry
                .published
                .or(entry.updated)
                .map(|d| d.to_rfc3339())
                .unwrap_or_default();

            let summary = entry
                .summary
                .as_ref()
                .map(|s| s.content.clone())
                .or_else(|| {
                    entry.content.as_ref().and_then(|c| {
                        c.body.clone().or_else(|| {
                            Some(c.content_type.to_string())
                        }).or_else(|| Some(String::new()))
                    })
                })
                .unwrap_or_default();

            let link = entry
                .links
                .first()
                .map(|l| l.href.clone())
                .unwrap_or_default();

            // Clean HTML tags from summary if present
            let clean_summary = if summary.contains('<') {
                html2md::parse_html(&summary)
            } else {
                summary
            };

            let date_str = if !date.is_empty() {
                let d = date.split('T').next().unwrap_or(&date);
                format!("**{}** — ", d)
            } else {
                String::new()
            };

            markdown.push_str(&format!("## {}{}\n\n", date_str, entry_title));

            if !clean_summary.trim().is_empty() {
                // Limit summary length
                let summary_text = if clean_summary.len() > 500 {
                    format!("{}...", &clean_summary[..500])
                } else {
                    clean_summary
                };
                markdown.push_str(&format!("{}\n\n", summary_text.trim()));
            }

            if !link.is_empty() {
                markdown.push_str(&format!("[Read more]({})\n\n", link));
            }
        }

        metadata.source_url = Some(url.clone());

        Ok(ExtractResult {
            text: markdown.trim().to_string(),
            suggested_filename: format!(
                "{}.md",
                feed_title.to_lowercase().replace(' ', "-")
            ),
            metadata,
            original_content: url.as_bytes().to_vec(),
        })
    }
}
