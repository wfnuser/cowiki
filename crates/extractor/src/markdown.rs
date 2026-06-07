use async_trait::async_trait;

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};
use crate::decode_base64;

pub struct MarkdownExtractor;

#[async_trait]
impl SourceExtractor for MarkdownExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Markdown]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::NoAuth
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        // Decode base64 if needed
        let text = if input.encoding.as_deref() == Some("base64") {
            String::from_utf8(decode_base64(&input.content)?)
                .map_err(|e| ExtractError::InvalidInput(format!("invalid UTF-8 in markdown: {}", e)))?
        } else {
            input.content.clone()
        };

        // Parse frontmatter if present for metadata
        let mut metadata = ExtractMetadata::default();

        let body = if text.starts_with("---") {
            let parts: Vec<&str> = text.splitn(3, "---").collect();
            if parts.len() >= 3 {
                let fm = parts[1];
                for line in fm.lines() {
                    let line = line.trim();
                    if let Some(value) = line.strip_prefix("title:") {
                        metadata.title = Some(value.trim().trim_matches('"').to_string());
                    } else if let Some(value) = line.strip_prefix("author:") {
                        metadata.author = Some(value.trim().trim_matches('"').to_string());
                    } else if let Some(value) = line.strip_prefix("source_url:") {
                        metadata.source_url = Some(value.trim().trim_matches('"').to_string());
                    }
                }
                parts[2].to_string()
            } else {
                text.clone()
            }
        } else {
            text.clone()
        };

        // Normalize: collapse 3+ blank lines, trim trailing whitespace
        let mut normalized = String::new();
        let mut blank_count = 0;
        for line in body.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                blank_count += 1;
                if blank_count <= 2 {
                    normalized.push('\n');
                }
            } else {
                blank_count = 0;
                normalized.push_str(trimmed);
                normalized.push('\n');
            }
        }

        let filename = input
            .filename
            .unwrap_or_else(|| "source.md".to_string());

        let suggested = if filename.ends_with(".md") {
            filename
        } else {
            format!("{}.md", filename)
        };

        let original = if input.encoding.as_deref() == Some("base64") {
            decode_base64(&input.content)?
        } else {
            input.content.as_bytes().to_vec()
        };

        Ok(ExtractResult {
            text: normalized.trim().to_string(),
            suggested_filename: suggested,
            metadata,
            original_content: original,
        })
    }
}
