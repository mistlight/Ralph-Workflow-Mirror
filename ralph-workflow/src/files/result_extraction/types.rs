//! Result extraction types.

/// Result of extracting content from an agent's JSON log.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// The raw content extracted from the log (if any)
    pub raw_content: Option<String>,
    /// Whether the content passed validation
    #[cfg(any(test, feature = "test-utils"))]
    pub is_valid: bool,
    /// Validation warning message
    #[cfg(any(test, feature = "test-utils"))]
    pub validation_warning: Option<String>,
}

impl ExtractionResult {
    /// Create a result with valid content
    pub const fn valid(content: String) -> Self {
        Self {
            raw_content: Some(content),
            #[cfg(any(test, feature = "test-utils"))]
            is_valid: true,
            #[cfg(any(test, feature = "test-utils"))]
            validation_warning: None,
        }
    }

    /// Create a result with invalid content
    pub fn invalid(content: String, _warning: &str) -> Self {
        Self {
            raw_content: Some(content),
            #[cfg(any(test, feature = "test-utils"))]
            is_valid: false,
            #[cfg(any(test, feature = "test-utils"))]
            validation_warning: Some(_warning.to_string()),
        }
    }

    /// Create an empty result (no content found)
    pub const fn empty() -> Self {
        Self {
            raw_content: None,
            #[cfg(any(test, feature = "test-utils"))]
            is_valid: false,
            #[cfg(any(test, feature = "test-utils"))]
            validation_warning: None,
        }
    }
}
