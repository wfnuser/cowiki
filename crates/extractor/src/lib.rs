pub mod error;
pub mod registry;
pub mod types;

pub mod csv;
pub mod docx;
pub mod github;
pub mod markdown;
pub mod ooxml_images;
pub mod pdf;
pub mod pptx;
pub mod rss;
pub mod text;
pub mod url;
pub mod xlsx;

use async_trait::async_trait;

pub use error::ExtractError;
pub use registry::ExtractorRegistry;
pub use types::{AuthStrategy, ExtractInput, ExtractMetadata, ExtractResult, SourceType};

/// Core trait for all source extractors.
#[async_trait]
pub trait SourceExtractor: Send + Sync {
    /// Source types this extractor handles.
    fn supported_types(&self) -> Vec<SourceType>;

    /// Authentication strategy required.
    fn auth_strategy(&self) -> AuthStrategy;

    /// Extract clean Markdown text from the input.
    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError>;
}

/// Create a default registry with all Phase 1 extractors registered.
pub fn create_default_registry() -> ExtractorRegistry {
    let mut registry = ExtractorRegistry::new();
    registry.register(Box::new(text::TextExtractor));
    registry.register(Box::new(url::UrlExtractor::new()));
    registry.register(Box::new(markdown::MarkdownExtractor));
    registry.register(Box::new(csv::CsvExtractor));
    registry.register(Box::new(pdf::PdfExtractor));
    registry.register(Box::new(docx::DocxExtractor));
    registry.register(Box::new(pptx::PptxExtractor));
    registry.register(Box::new(xlsx::XlsxExtractor));
    registry.register(Box::new(github::GitHubExtractor::new()));
    registry.register(Box::new(rss::RssExtractor::new()));
    registry
}

/// Helper: decode base64 content, returning raw bytes.
pub(crate) fn decode_base64(content: &str) -> Result<Vec<u8>, ExtractError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(content)
        .map_err(|e| ExtractError::Base64Decode(e.to_string()))
}

/// Helper: resolve input content to either raw bytes (if base64) or string.
pub(crate) enum ResolvedContent {
    Bytes(Vec<u8>),
    Text(String),
}

pub(crate) fn resolve_content(input: &ExtractInput) -> Result<ResolvedContent, ExtractError> {
    if input.encoding.as_deref() == Some("base64") {
        Ok(ResolvedContent::Bytes(decode_base64(&input.content)?))
    } else {
        Ok(ResolvedContent::Text(input.content.clone()))
    }
}
