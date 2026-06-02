pub mod auth;
pub mod compile;
pub mod ingest;
pub mod keys;
pub mod pages;
pub mod review;
pub mod search;
pub mod sources;
pub mod submit;
pub mod workspace;

use crate::error::{AppError, Result};

/// Validate source filename to prevent path traversal attacks.
pub fn validate_source_filename(filename: &str) -> Result<()> {
    if filename.is_empty()
        || filename.contains("..")
        || filename.contains('/')
        || filename.contains('\\')
        || filename.starts_with('.')
        || filename.len() > 255
    {
        return Err(AppError::BadRequest("invalid filename".into()));
    }
    Ok(())
}
