//! JSON parser type definitions for agent output parsing.
//!
//! This module defines the `JsonParserType` enum that determines how
//! each agent's output stream is parsed. Different agents use different
//! JSON streaming formats (Claude's stream-json, Codex's format, etc.).

use serde::Deserialize;

/// JSON parser type for agent output.
///
/// Different AI coding agents output their streaming JSON in different formats.
/// This enum determines which parser to use for a given agent's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JsonParserType {
    /// Claude's stream-json format (also used by Qwen Code, CCS, and other compatible CLIs).
    #[default]
    Claude,
    /// Codex's JSON format.
    Codex,
    /// Gemini's stream-json format.
    Gemini,
    /// `OpenCode`'s NDJSON format.
    OpenCode,
    /// Generic line-based output (no parsing, pass-through).
    Generic,
}

impl JsonParserType {
    /// Parse parser type from string.
    ///
    /// Supports common names: "claude", "ccs", "codex", "gemini", "opencode", "generic", "none", "raw".
    /// CCS (Claude Code Switch) wraps Claude Code, so "ccs" maps to the Claude parser.
    /// Unknown strings default to `Generic`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            // CCS wraps Claude Code, so it uses the same stream-json format
            "claude" | "ccs" => Self::Claude,
            "codex" => Self::Codex,
            "gemini" => Self::Gemini,
            "opencode" => Self::OpenCode,
            _ => Self::Generic,
        }
    }
}

impl std::fmt::Display for JsonParserType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude => write!(f, "claude"),
            Self::Codex => write!(f, "codex"),
            Self::Gemini => write!(f, "gemini"),
            Self::OpenCode => write!(f, "opencode"),
            Self::Generic => write!(f, "generic"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_parser_type_parse() {
        assert_eq!(JsonParserType::parse("claude"), JsonParserType::Claude);
        assert_eq!(JsonParserType::parse("CLAUDE"), JsonParserType::Claude);
        // CCS wraps Claude Code, so it uses the same parser
        assert_eq!(JsonParserType::parse("ccs"), JsonParserType::Claude);
        assert_eq!(JsonParserType::parse("CCS"), JsonParserType::Claude);
        assert_eq!(JsonParserType::parse("codex"), JsonParserType::Codex);
        assert_eq!(JsonParserType::parse("gemini"), JsonParserType::Gemini);
        assert_eq!(JsonParserType::parse("GEMINI"), JsonParserType::Gemini);
        assert_eq!(JsonParserType::parse("opencode"), JsonParserType::OpenCode);
        assert_eq!(JsonParserType::parse("OPENCODE"), JsonParserType::OpenCode);
        assert_eq!(JsonParserType::parse("generic"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("none"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("raw"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("unknown"), JsonParserType::Generic);
    }

    #[test]
    fn test_json_parser_type_display() {
        assert_eq!(format!("{}", JsonParserType::Claude), "claude");
        assert_eq!(format!("{}", JsonParserType::Codex), "codex");
        assert_eq!(format!("{}", JsonParserType::Gemini), "gemini");
        assert_eq!(format!("{}", JsonParserType::OpenCode), "opencode");
        assert_eq!(format!("{}", JsonParserType::Generic), "generic");
    }
}
