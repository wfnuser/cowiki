use thiserror::Error;

use crate::types::SourceType;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("unsupported source type: {0:?}")]
    UnsupportedType(SourceType),

    #[error("authentication required: {0}")]
    AuthRequired(String),

    #[error("extraction failed for {source_type:?}: {message}")]
    ExtractionFailed { source_type: SourceType, message: String },

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("base64 decode failed: {0}")]
    Base64Decode(String),

    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("parse failed: {0}")]
    ParseError(String),
}
