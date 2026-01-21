//! Result extraction types.

/// Result of extracting content from an agent's JSON log.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// The raw content extracted from the log (if any)
    pub raw_content: Option<String>,
    /// Whether the content passed validation (test-only field)
    #[cfg_attr(not(any(test, feature = "test-utils")), allow(dead_code))]
    #[cfg(any(test, feature = "test-utils"))]
    pub is_valid: bool,
    #[cfg(not(any(test, feature = "test-utils")))]
    #[cfg_attr(not(any(test, feature = "test-utils")), allow(dead_code))]
    is_valid: bool,
    /// Validation warning message (test-only field)
    #[cfg(any(test, feature = "test-utils"))]
    pub validation_warning: Option<String>,
    #[cfg(not(any(test, feature = "test-utils")))]
    #[cfg_attr(not(any(test, feature = "test-utils")), allow(dead_code))]
    validation_warning: Option<String>,
}

impl ExtractionResult {
    /// Create a result with valid content
    pub const fn valid(content: String) -> Self {
        Self {
            raw_content: Some(content),
            is_valid: true,
            validation_warning: None,
        }
    }

    /// Create a result with invalid content
    pub fn invalid(content: String, warning: &str) -> Self {
        Self {
            raw_content: Some(content),
            is_valid: false,
            validation_warning: Some(warning.to_string()),
        }
    }

    /// Create an empty result (no content found)
    pub const fn empty() -> Self {
        Self {
            raw_content: None,
            is_valid: false,
            validation_warning: None,
        }
    }
}
