use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// All supported source types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// Auto-detect from filename extension or magic bytes
    Auto,
    /// Plain text (passthrough)
    Text,
    /// Web page URL
    Url,
    /// PDF document
    Pdf,
    /// Word document
    Docx,
    /// PowerPoint presentation
    Pptx,
    /// Excel spreadsheet
    Xlsx,
    /// Comma/tab-separated values
    Csv,
    /// Markdown document (validate + normalize)
    Markdown,
    /// GitHub repository
    GitHubRepo,
    /// GitHub issue or pull request
    GitHubIssue,
    /// RSS, Atom, or JSON Feed
    Rss,
}

impl SourceType {
    /// Parse from a string. Returns None for unknown types.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "text" => Some(Self::Text),
            "url" => Some(Self::Url),
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            "pptx" => Some(Self::Pptx),
            "xlsx" => Some(Self::Xlsx),
            "csv" => Some(Self::Csv),
            "markdown" | "md" => Some(Self::Markdown),
            "github_repo" | "github-repo" => Some(Self::GitHubRepo),
            "github_issue" | "github-issue" => Some(Self::GitHubIssue),
            "rss" | "atom" | "feed" => Some(Self::Rss),
            _ => None,
        }
    }

    /// Try to detect source type from a filename extension.
    pub fn from_extension(filename: &str) -> Option<Self> {
        let ext = filename.rsplit('.').next()?.to_lowercase();
        match ext.as_str() {
            "pdf" => Some(Self::Pdf),
            "docx" | "doc" => Some(Self::Docx),
            "pptx" | "ppt" => Some(Self::Pptx),
            "xlsx" | "xls" => Some(Self::Xlsx),
            "csv" => Some(Self::Csv),
            "md" | "markdown" => Some(Self::Markdown),
            "txt" => Some(Self::Text),
            _ => None,
        }
    }
}

/// Classification of authentication requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStrategy {
    /// No authentication needed
    NoAuth,
    /// API token/key from user settings
    ApiKey,
    /// Browser cookie session (future: headless browser support)
    Cookie,
}

/// Input to an extractor.
#[derive(Debug, Clone)]
pub struct ExtractInput {
    /// Source type (explicit or Auto)
    pub source_type: SourceType,
    /// Content: URL string, raw text, or base64-encoded bytes
    pub content: String,
    /// Encoding: "base64" for binary, None for plain text/URL
    pub encoding: Option<String>,
    /// Original filename (for auto-detection and original file preservation)
    pub filename: Option<String>,
    /// Runtime configuration (API tokens, language preferences, etc.)
    pub config: HashMap<String, String>,
}

/// Result of a successful extraction.
#[derive(Debug, Clone)]
pub struct ExtractResult {
    /// Clean Markdown text ready for LLM compilation
    pub text: String,
    /// Suggested filename for the extracted markdown (e.g. "report.md")
    pub suggested_filename: String,
    /// Extracted metadata (title, author, source URL, etc.)
    pub metadata: ExtractMetadata,
    /// Raw bytes of the original file (for storage)
    pub original_content: Vec<u8>,
}

/// Metadata extracted from a source.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExtractMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub source_url: Option<String>,
    pub language: Option<String>,
    pub page_count: Option<usize>,
    pub extra: HashMap<String, String>,
}
