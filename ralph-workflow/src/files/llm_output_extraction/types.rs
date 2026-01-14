//! LLM Output Extraction Types
//!
//! This module defines the core types used for LLM output extraction.

/// Parser types supported by the extraction system.
/// Matches `crate::agents::parser::JsonParserType` but kept separate to avoid circular deps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Claude CLI stream-json format (also used by CCS, Qwen)
    #[default]
    Claude,
    /// `OpenAI` Codex CLI format
    Codex,
    /// Google Gemini CLI format
    Gemini,
    /// `OpenCode` NDJSON format
    OpenCode,
    /// Generic/plain text (fallback)
    Generic,
}

impl OutputFormat {
    /// Parse format from string name
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claude" | "ccs" | "qwen" => Self::Claude,
            "codex" => Self::Codex,
            "gemini" => Self::Gemini,
            "opencode" => Self::OpenCode,
            _ => Self::Generic,
        }
    }
}

/// Result of LLM output extraction
#[derive(Debug, Clone)]
pub struct ExtractionOutput {
    /// The extracted content (always present if input was non-empty)
    pub content: String,
    /// Whether extraction used a structured format vs fallback
    pub was_structured: bool,
    /// The detected/used format
    pub format: OutputFormat,
    /// Any warning or diagnostic message
    pub warning: Option<String>,
}

impl ExtractionOutput {
    pub(crate) const fn structured(content: String, format: OutputFormat) -> Self {
        Self {
            content,
            was_structured: true,
            format,
            warning: None,
        }
    }

    pub(crate) fn fallback(content: String, warning: &str) -> Self {
        Self {
            content,
            was_structured: false,
            format: OutputFormat::Generic,
            warning: Some(warning.to_string()),
        }
    }

    pub(crate) fn empty() -> Self {
        Self {
            content: String::new(),
            was_structured: false,
            format: OutputFormat::Generic,
            warning: Some("No content found in output".to_string()),
        }
    }
}
