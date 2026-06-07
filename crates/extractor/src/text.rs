use async_trait::async_trait;

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};

pub struct TextExtractor;

#[async_trait]
impl SourceExtractor for TextExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Text]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::NoAuth
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        // Normalize whitespace: collapse multiple blank lines, trim trailing whitespace
        let text = input
            .content
            .lines()
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        let filename = input
            .filename
            .unwrap_or_else(|| "source-text.md".to_string());

        let suggested = if filename.ends_with(".md") {
            filename.clone()
        } else {
            format!("{}.md", filename.trim_end_matches(".txt"))
        };

        Ok(ExtractResult {
            text,
            suggested_filename: suggested,
            metadata: ExtractMetadata::default(),
            original_content: input.content.into_bytes(),
        })
    }
}
