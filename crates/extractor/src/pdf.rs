use async_trait::async_trait;

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};
use crate::decode_base64;

pub struct PdfExtractor;

#[async_trait]
impl SourceExtractor for PdfExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Pdf]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::NoAuth
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        let bytes = if input.encoding.as_deref() == Some("base64") {
            decode_base64(&input.content)?
        } else {
            return Err(ExtractError::InvalidInput(
                "PDF requires base64 encoding".into(),
            ));
        };

        let original = bytes.clone();

        let pages = pdf_extract::extract_text_from_mem(&bytes)
            .map_err(|e| ExtractError::ExtractionFailed {
                source_type: SourceType::Pdf,
                message: format!("pdf-extract failed: {}", e),
            })?;

        let markdown = pages
            .split('\n')
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n");

        // Try to extract title from first non-empty line
        let title = markdown
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string());

        let page_count = pages.split('\x0c').count();

        let filename = input.filename.as_deref().unwrap_or("document.pdf");
        let suggested = format!("{}.md", filename.trim_end_matches(".pdf"));

        let mut metadata = ExtractMetadata::default();
        metadata.title = title;
        metadata.page_count = Some(page_count);

        Ok(ExtractResult {
            text: markdown,
            suggested_filename: suggested,
            metadata,
            original_content: original,
        })
    }
}
