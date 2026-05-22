/// Supported VLM provider names.
pub const VALID_PROVIDERS: &[&str] = &["openai"];

/// Check if a provider name is valid.
pub fn is_valid_provider(name: &str) -> bool {
    VALID_PROVIDERS.contains(&name)
}

/// Default provider name.
pub fn default_provider() -> &'static str {
    "openai"
}
