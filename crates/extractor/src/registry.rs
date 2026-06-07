use std::collections::HashMap;
use std::sync::Arc;

use tracing::debug;

use crate::error::ExtractError;
use crate::types::{ExtractInput, ExtractResult, SourceType};
use crate::SourceExtractor;

/// Registry that maps SourceType → Extractor for automatic dispatch.
pub struct ExtractorRegistry {
    extractors: HashMap<SourceType, Arc<Box<dyn SourceExtractor>>>,
}

impl ExtractorRegistry {
    pub fn new() -> Self {
        Self {
            extractors: HashMap::new(),
        }
    }

    /// Register an extractor. Its supported_types() determine which
    /// SourceType keys it handles.
    pub fn register(&mut self, extractor: Box<dyn SourceExtractor>) {
        let extractor: Arc<Box<dyn SourceExtractor>> = Arc::new(extractor);
        for ty in extractor.supported_types() {
            debug!(source_type = ?ty, "registered extractor");
            self.extractors.insert(ty, Arc::clone(&extractor));
        }
    }

    /// Extract content using the appropriate extractor for the given SourceType.
    /// If source_type is Auto, detect from filename extension.
    pub async fn extract(&self, mut input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        let source_type = if input.source_type == SourceType::Auto {
            let detected = self.detect_type(&input);
            debug!(?detected, filename = ?input.filename, "auto-detected source type");
            input.source_type = detected.clone();
            detected
        } else {
            input.source_type.clone()
        };

        let extractor = self
            .extractors
            .get(&source_type)
            .ok_or_else(|| ExtractError::UnsupportedType(source_type.clone()))?;

        extractor.extract(input).await
    }

    /// Detect source type from filename extension, falling back to magic bytes.
    fn detect_type(&self, input: &ExtractInput) -> SourceType {
        // Try filename extension first
        if let Some(ref filename) = input.filename {
            if let Some(ty) = SourceType::from_extension(filename) {
                return ty;
            }
        }

        // Try magic bytes
        if input.encoding.as_deref() == Some("base64") {
            if let Ok(bytes) = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &input.content,
            ) {
                if let Some(kind) = infer::get(&bytes) {
                    match kind.extension() {
                        "pdf" => return SourceType::Pdf,
                        "docx" => return SourceType::Docx,
                        "xlsx" => return SourceType::Xlsx,
                        _ => {}
                    }
                }
            }
        }

        // Fall back to text
        SourceType::Text
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
