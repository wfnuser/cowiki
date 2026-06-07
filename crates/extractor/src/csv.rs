use async_trait::async_trait;

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};
use crate::decode_base64;

pub struct CsvExtractor;

#[async_trait]
impl SourceExtractor for CsvExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Csv]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::NoAuth
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        let bytes = if input.encoding.as_deref() == Some("base64") {
            decode_base64(&input.content)?
        } else {
            input.content.as_bytes().to_vec()
        };

        let text = String::from_utf8_lossy(&bytes);
        let original = bytes.clone();

        // Auto-detect delimiter: count occurrences of likely delimiters in first line
        let first_line = text.lines().next().unwrap_or("");
        let delimiters = [',', '\t', ';', '|'];
        let delimiter = delimiters
            .iter()
            .max_by_key(|&&d| first_line.chars().filter(|&c| c == d).count())
            .copied()
            .unwrap_or(',');

        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter as u8)
            .has_headers(true)
            .flexible(true)
            .from_reader(bytes.as_slice());

        let headers = reader
            .headers()
            .map_err(|e| ExtractError::ParseError(format!("CSV header: {}", e)))?
            .clone();

        let mut markdown = String::new();

        // Header row
        markdown.push_str("| ");
        markdown.push_str(&headers.iter().collect::<Vec<_>>().join(" | "));
        markdown.push_str(" |\n");

        // Separator row
        markdown.push_str("|");
        for _ in 0..headers.len() {
            markdown.push_str(" --- |");
        }
        markdown.push('\n');

        // Data rows
        for result in reader.records() {
            let record = result.map_err(|e| ExtractError::ParseError(format!("CSV row: {}", e)))?;
            markdown.push_str("| ");
            markdown.push_str(&record.iter().collect::<Vec<_>>().join(" | "));
            markdown.push_str(" |\n");
        }

        let filename = input
            .filename
            .as_deref()
            .unwrap_or("data.csv");
        let suggested = if filename.ends_with(".md") {
            filename.to_string()
        } else {
            format!("{}.md", filename.trim_end_matches(".csv"))
        };

        Ok(ExtractResult {
            text: markdown,
            suggested_filename: suggested,
            metadata: ExtractMetadata::default(),
            original_content: original,
        })
    }
}
