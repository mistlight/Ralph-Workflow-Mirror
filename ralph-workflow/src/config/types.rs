//! Configuration types and enums.
//!
//! This module defines the core configuration types used throughout Ralph:
//! - [`Config`]: The main configuration struct
//! - [`ReviewDepth`]: Review thoroughness levels
//! - [`Verbosity`]: Output verbosity levels

use super::truncation;
use std::path::PathBuf;

/// Review depth levels for controlling review thoroughness.
///
/// # Variants
///
/// * `Standard` - Balanced review covering functionality, quality, and security
/// * `Comprehensive` - In-depth analysis with priority-ordered checks
/// * `Security` - Security-focused analysis emphasizing OWASP Top 10
/// * `Incremental` - Focused review of changed files only (git diff)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewDepth {
    /// Standard review - balanced coverage of functionality, quality, and security
    #[default]
    Standard,
    /// Comprehensive review - in-depth analysis with priority-ordered checks
    Comprehensive,
    /// Security-focused review - emphasizes security analysis above all else
    Security,
    /// Incremental review - focuses only on changed files (git diff)
    Incremental,
}

impl ReviewDepth {
    /// Parse review depth from string.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to parse
    ///
    /// # Returns
    ///
    /// Returns `Some(ReviewDepth)` if the string matches a known alias,
    /// `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(ReviewDepth::from_str("standard"), Some(ReviewDepth::Standard));
    /// assert_eq!(ReviewDepth::from_str("security"), Some(ReviewDepth::Security));
    /// ```
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "standard" | "default" | "normal" => Some(Self::Standard),
            "comprehensive" | "thorough" | "full" => Some(Self::Comprehensive),
            "security" | "secure" | "security-focused" => Some(Self::Security),
            "incremental" | "diff" | "changed" => Some(Self::Incremental),
            _ => None,
        }
    }

    /// Get a description for display.
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Standard => "Balanced review covering functionality, quality, and security",
            Self::Comprehensive => "In-depth analysis with priority-ordered checks",
            Self::Security => "Security-focused analysis emphasizing OWASP Top 10",
            Self::Incremental => "Focused review of changed files only (git diff)",
        }
    }
}

/// Verbosity levels for output.
///
/// # Variants
///
/// * `Quiet` (0) - Minimal output, aggressive truncation
/// * `Normal` (1) - Balanced output with moderate truncation
/// * `Verbose` (2) - Expanded output limits (default)
/// * `Full` (3) - No truncation, show all content
/// * `Debug` (4) - Maximum verbosity, includes raw JSON and detailed info
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// Quiet - minimal output, aggressive truncation
    Quiet = 0,
    /// Normal - balanced output with moderate truncation
    Normal = 1,
    /// Verbose - expanded output limits (default)
    Verbose = 2,
    /// Full - no truncation, show all content
    Full = 3,
    /// Debug - maximum verbosity, includes raw JSON and detailed info
    Debug = 4,
}

impl From<u8> for Verbosity {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Quiet,
            1 => Self::Normal,
            2 => Self::Verbose,
            3 => Self::Full,
            _ => Self::Debug,
        }
    }
}

impl Verbosity {
    /// Get truncation limit for content type.
    ///
    /// # Arguments
    ///
    /// * `content_type` - The type of content:
    ///   - "text": Assistant text output
    ///   - "`tool_result"`: Tool execution results
    ///   - "`tool_input"`: Tool input parameters
    ///   - "user": User messages
    ///   - "result": Final result summaries
    ///   - "command": Command execution strings
    ///   - "`agent_msg"`: Agent messages/thinking
    ///
    /// # Returns
    ///
    /// The maximum number of characters to display for the given content type.
    pub(crate) fn truncate_limit(self, content_type: &str) -> usize {
        truncation::get_limit(self as u8, content_type)
    }

    /// Returns true if this verbosity level should show debug information.
    pub(crate) const fn is_debug(self) -> bool {
        matches!(self, Self::Debug)
    }

    /// Returns true if this verbosity level is at least Verbose.
    pub(crate) const fn is_verbose(self) -> bool {
        matches!(self, Self::Verbose | Self::Full | Self::Debug)
    }

    /// Returns true if tool inputs should be shown (Normal and above).
    ///
    /// Tool inputs provide crucial context for understanding what the agent is doing.
    /// They are shown at Normal level and above for better usability.
    pub(crate) const fn show_tool_input(self) -> bool {
        !matches!(self, Self::Quiet)
    }
}

/// Behavioral flags for Ralph configuration.
///
/// Groups user interaction and validation-related boolean settings.
#[derive(Debug, Clone, Copy, Default)]
pub struct BehavioralFlags {
    /// Interactive mode (keep agent in foreground)
    pub(crate) interactive: bool,
    /// Whether to auto-detect project stack for review guidelines
    pub(crate) auto_detect_stack: bool,
    /// Whether to run strict PROMPT.md validation
    pub(crate) strict_validation: bool,
}

/// Feature flags for Ralph configuration.
///
/// Groups optional feature toggle settings.
#[derive(Debug, Clone, Copy, Default)]
pub struct FeatureFlags {
    /// Whether to enable checkpoint/resume functionality
    pub(crate) checkpoint_enabled: bool,
    /// Force universal review prompt for all agents (default: auto-detect)
    /// When true, the universal/simplified prompt is always used for review
    pub(crate) force_universal_prompt: bool,
    /// Whether to enable automatic rebase before and after pipeline
    pub(crate) auto_rebase_enabled: bool,
}

/// Ralph configuration.
///
/// This struct holds all configuration options for Ralph, populated from
/// environment variables and CLI arguments. Default values are applied
/// via [`Default::default()`].
#[derive(Debug, Clone)]
// Configuration options naturally use many boolean flags. These represent
// independent feature toggles, not a state machine, so bools are appropriate.
pub struct Config {
    /// Developer (driver) agent (set via CLI, env, or `agent_chain`)
    pub(crate) developer_agent: Option<String>,
    /// Reviewer agent (set via CLI, env, or `agent_chain`)
    pub(crate) reviewer_agent: Option<String>,
    /// Developer command override
    pub(crate) developer_cmd: Option<String>,
    /// Reviewer command override
    pub(crate) reviewer_cmd: Option<String>,
    /// Developer model override (e.g., "-m opencode/glm-4.7-free")
    /// Passed to the agent's `model_flag` parameter
    pub(crate) developer_model: Option<String>,
    /// Reviewer model override (e.g., "-m opencode/claude-sonnet-4")
    /// Passed to the agent's `model_flag` parameter
    pub(crate) reviewer_model: Option<String>,
    /// Developer provider override (e.g., "opencode", "anthropic", "openai")
    /// When set, constructs the model flag as "-m {`provider}/{model_name`}"
    pub(crate) developer_provider: Option<String>,
    /// Reviewer provider override (e.g., "opencode", "anthropic", "openai")
    /// When set, constructs the model flag as "-m {`provider}/{model_name`}"
    pub(crate) reviewer_provider: Option<String>,
    /// JSON parser override for the reviewer agent (claude, codex, gemini, opencode, generic)
    /// When set, overrides the agent's configured `json_parser` setting
    pub(crate) reviewer_json_parser: Option<String>,
    /// Feature flags (checkpoint, universal prompt)
    pub(crate) features: FeatureFlags,
    /// Number of developer iterations
    pub(crate) developer_iters: u32,
    /// Number of reviewer re-review passes after fix
    pub(crate) reviewer_reviews: u32,
    /// Fast check command (optional)
    pub(crate) fast_check_cmd: Option<String>,
    /// Full check command (optional)
    pub(crate) full_check_cmd: Option<String>,
    /// Behavioral flags (interactive, auto-detect, strict validation)
    pub(crate) behavior: BehavioralFlags,
    /// Path to save last prompt
    pub(crate) prompt_path: PathBuf,
    /// Developer context level (0=minimal, 1=normal)
    pub(crate) developer_context: u8,
    /// Reviewer context level (0=minimal/fresh eyes, 1=normal)
    pub(crate) reviewer_context: u8,
    /// Verbosity level
    pub(crate) verbosity: Verbosity,
    /// Commit message
    pub(crate) commit_msg: String,
    /// Review depth level (standard, comprehensive, security, incremental)
    pub(crate) review_depth: ReviewDepth,
    /// Isolation mode: when true, NOTES.md and ISSUES.md are not generated and
    /// any existing ones are deleted at the start of each run. This prevents
    /// context contamination from previous runs. Default: true.
    pub(crate) isolation_mode: bool,
    /// Git user name for commits (optional, falls back to git config)
    pub(crate) git_user_name: Option<String>,
    /// Git user email for commits (optional, falls back to git config)
    pub(crate) git_user_email: Option<String>,
}

impl Config {
    /// Set the commit message.
    pub(crate) fn with_commit_msg(mut self, msg: String) -> Self {
        self.commit_msg = msg;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        super::loader::load_config().0
    }
}
