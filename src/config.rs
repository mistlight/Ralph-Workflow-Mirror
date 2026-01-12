//! Configuration Module
//!
//! Handles environment variables and configuration for Ralph.

use std::env;
use std::path::PathBuf;

/// Truncation limits for Quiet verbosity mode
mod truncate_limits {
    pub const QUIET_TEXT: usize = 80;
    pub const QUIET_TOOL_RESULT: usize = 60;
    pub const QUIET_TOOL_INPUT: usize = 40;
    pub const QUIET_USER: usize = 40;
    pub const QUIET_RESULT: usize = 300;
    pub const QUIET_COMMAND: usize = 60;
    pub const QUIET_AGENT_MSG: usize = 80;
    pub const QUIET_DEFAULT: usize = 60;

    pub const NORMAL_TEXT: usize = 400;
    pub const NORMAL_TOOL_RESULT: usize = 300;
    pub const NORMAL_TOOL_INPUT: usize = 200;
    pub const NORMAL_USER: usize = 200;
    pub const NORMAL_RESULT: usize = 1500;
    pub const NORMAL_COMMAND: usize = 200;
    pub const NORMAL_AGENT_MSG: usize = 400;
    pub const NORMAL_DEFAULT: usize = 300;

    pub const VERBOSE_TEXT: usize = 800;
    pub const VERBOSE_TOOL_RESULT: usize = 600;
    pub const VERBOSE_TOOL_INPUT: usize = 500;
    pub const VERBOSE_USER: usize = 400;
    pub const VERBOSE_RESULT: usize = 3000;
    pub const VERBOSE_COMMAND: usize = 400;
    pub const VERBOSE_AGENT_MSG: usize = 800;
    pub const VERBOSE_DEFAULT: usize = 600;

    /// Effectively unlimited for Full/Debug modes
    pub const UNLIMITED: usize = 999_999;
}

fn parse_env_bool(value: &str) -> Option<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => None,
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

/// Review depth levels for controlling review thoroughness
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ReviewDepth {
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
    /// Parse review depth from string
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "standard" | "default" | "normal" => Some(ReviewDepth::Standard),
            "comprehensive" | "thorough" | "full" => Some(ReviewDepth::Comprehensive),
            "security" | "secure" | "security-focused" => Some(ReviewDepth::Security),
            "incremental" | "diff" | "changed" => Some(ReviewDepth::Incremental),
            _ => None,
        }
    }

    /// Get a description for display
    pub(crate) fn description(&self) -> &'static str {
        match self {
            ReviewDepth::Standard => {
                "Balanced review covering functionality, quality, and security"
            }
            ReviewDepth::Comprehensive => "In-depth analysis with priority-ordered checks",
            ReviewDepth::Security => "Security-focused analysis emphasizing OWASP Top 10",
            ReviewDepth::Incremental => "Focused review of changed files only (git diff)",
        }
    }
}

/// Verbosity levels for output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verbosity {
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
            0 => Verbosity::Quiet,
            1 => Verbosity::Normal,
            2 => Verbosity::Verbose,
            3 => Verbosity::Full,
            _ => Verbosity::Debug,
        }
    }
}

impl Verbosity {
    /// Get truncation limit for content type
    ///
    /// Content types:
    /// - "text": Assistant text output
    /// - "tool_result": Tool execution results
    /// - "tool_input": Tool input parameters
    /// - "user": User messages
    /// - "result": Final result summaries
    /// - "command": Command execution strings
    /// - "agent_msg": Agent messages/thinking
    pub(crate) fn truncate_limit(&self, content_type: &str) -> usize {
        use truncate_limits::*;

        match self {
            Verbosity::Quiet => match content_type {
                "text" => QUIET_TEXT,
                "tool_result" => QUIET_TOOL_RESULT,
                "tool_input" => QUIET_TOOL_INPUT,
                "user" => QUIET_USER,
                "result" => QUIET_RESULT,
                "command" => QUIET_COMMAND,
                "agent_msg" => QUIET_AGENT_MSG,
                _ => QUIET_DEFAULT,
            },
            // Normal mode: increased limits for better usability
            // Previously too aggressive truncation made debugging difficult
            Verbosity::Normal => match content_type {
                "text" => NORMAL_TEXT,
                "tool_result" => NORMAL_TOOL_RESULT,
                "tool_input" => NORMAL_TOOL_INPUT,
                "user" => NORMAL_USER,
                "result" => NORMAL_RESULT,
                "command" => NORMAL_COMMAND,
                "agent_msg" => NORMAL_AGENT_MSG,
                _ => NORMAL_DEFAULT,
            },
            // Verbose is the default - show generous amounts of context
            // Users need to see what's happening to understand agent behavior
            Verbosity::Verbose => match content_type {
                "text" => VERBOSE_TEXT,
                "tool_result" => VERBOSE_TOOL_RESULT,
                "tool_input" => VERBOSE_TOOL_INPUT,
                "user" => VERBOSE_USER,
                "result" => VERBOSE_RESULT,
                "command" => VERBOSE_COMMAND,
                "agent_msg" => VERBOSE_AGENT_MSG,
                _ => VERBOSE_DEFAULT,
            },
            // Full shows everything (essentially unlimited)
            Verbosity::Full | Verbosity::Debug => UNLIMITED,
        }
    }

    /// Returns true if this verbosity level should show debug information
    pub(crate) fn is_debug(&self) -> bool {
        matches!(self, Verbosity::Debug)
    }

    /// Returns true if this verbosity level is at least Verbose
    pub(crate) fn is_verbose(&self) -> bool {
        matches!(
            self,
            Verbosity::Verbose | Verbosity::Full | Verbosity::Debug
        )
    }

    /// Returns true if tool inputs should be shown (Normal and above)
    ///
    /// Tool inputs provide crucial context for understanding what the agent is doing.
    /// They are now shown at Normal level and above for better usability.
    pub(crate) fn show_tool_input(&self) -> bool {
        !matches!(self, Verbosity::Quiet)
    }
}

/// Ralph configuration
#[derive(Debug, Clone)]
pub(crate) struct Config {
    /// Developer (driver) agent (set via CLI, env, or agent_chain)
    pub(crate) developer_agent: Option<String>,
    /// Reviewer agent (set via CLI, env, or agent_chain)
    pub(crate) reviewer_agent: Option<String>,
    /// Developer command override
    pub(crate) developer_cmd: Option<String>,
    /// Reviewer command override
    pub(crate) reviewer_cmd: Option<String>,
    /// Developer model override (e.g., "-m opencode/glm-4.7-free")
    /// Passed to the agent's model_flag parameter
    pub(crate) developer_model: Option<String>,
    /// Reviewer model override (e.g., "-m opencode/claude-sonnet-4")
    /// Passed to the agent's model_flag parameter
    pub(crate) reviewer_model: Option<String>,
    /// Developer provider override (e.g., "opencode", "anthropic", "openai")
    /// When set, constructs the model flag as "-m {provider}/{model_name}"
    pub(crate) developer_provider: Option<String>,
    /// Reviewer provider override (e.g., "opencode", "anthropic", "openai")
    /// When set, constructs the model flag as "-m {provider}/{model_name}"
    pub(crate) reviewer_provider: Option<String>,
    /// Number of developer iterations
    pub(crate) developer_iters: u32,
    /// Number of reviewer re-review passes after fix
    pub(crate) reviewer_reviews: u32,
    /// Fast check command (optional)
    pub(crate) fast_check_cmd: Option<String>,
    /// Full check command (optional)
    pub(crate) full_check_cmd: Option<String>,
    /// Interactive mode (keep agent in foreground)
    pub(crate) interactive: bool,
    /// Path to save last prompt
    pub(crate) prompt_path: PathBuf,
    /// Path to agents configuration file (default: .agent/agents.toml)
    pub(crate) agents_config_path: PathBuf,
    /// Developer context level (0=minimal, 1=normal)
    pub(crate) developer_context: u8,
    /// Reviewer context level (0=minimal/fresh eyes, 1=normal)
    pub(crate) reviewer_context: u8,
    /// Verbosity level
    pub(crate) verbosity: Verbosity,
    /// Commit message
    pub(crate) commit_msg: String,
    /// Whether to auto-detect project stack for review guidelines
    pub(crate) auto_detect_stack: bool,
    /// Whether to enable checkpoint/resume functionality
    pub(crate) checkpoint_enabled: bool,
    /// Whether to run strict PROMPT.md validation
    pub(crate) strict_validation: bool,
    /// Review depth level (standard, comprehensive, security, incremental)
    pub(crate) review_depth: ReviewDepth,
    /// Isolation mode: when true, NOTES.md and ISSUES.md are not generated and
    /// any existing ones are deleted at the start of each run. This prevents
    /// context contamination from previous runs. Default: true.
    pub(crate) isolation_mode: bool,
}

impl Config {
    /// Load configuration from environment variables
    ///
    /// Note: developer_agent and reviewer_agent are NOT given hardcoded defaults here.
    /// The agent_chain configuration in agents.toml is the single source of truth
    /// for default agent selection. CLI/env vars can override the agent_chain.
    pub(crate) fn from_env() -> Self {
        let developer_agent = env::var("RALPH_DEVELOPER_AGENT")
            .or_else(|_| env::var("RALPH_DRIVER_AGENT"))
            .ok();
        let reviewer_agent = env::var("RALPH_REVIEWER_AGENT").ok();

        let developer_cmd = env::var("RALPH_DEVELOPER_CMD").ok();
        let reviewer_cmd = env::var("RALPH_REVIEWER_CMD").ok();

        let developer_model = env::var("RALPH_DEVELOPER_MODEL").ok();
        let reviewer_model = env::var("RALPH_REVIEWER_MODEL").ok();

        let developer_provider = env::var("RALPH_DEVELOPER_PROVIDER").ok();
        let reviewer_provider = env::var("RALPH_REVIEWER_PROVIDER").ok();

        Self {
            developer_agent,
            reviewer_agent,
            developer_cmd,
            reviewer_cmd,
            developer_model,
            reviewer_model,
            developer_provider,
            reviewer_provider,
            developer_iters: env::var("RALPH_DEVELOPER_ITERS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            reviewer_reviews: env::var("RALPH_REVIEWER_REVIEWS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
            fast_check_cmd: env::var("FAST_CHECK_CMD").ok().filter(|s| !s.is_empty()),
            full_check_cmd: env::var("FULL_CHECK_CMD").ok().filter(|s| !s.is_empty()),
            interactive: env::var("RALPH_INTERACTIVE")
                .ok()
                .and_then(|s| parse_env_bool(&s))
                .unwrap_or(true),
            prompt_path: PathBuf::from(
                env::var("RALPH_PROMPT_PATH")
                    .unwrap_or_else(|_| ".agent/last_prompt.txt".to_string()),
            ),
            agents_config_path: PathBuf::from(
                env::var("RALPH_AGENTS_CONFIG")
                    .unwrap_or_else(|_| ".agent/agents.toml".to_string()),
            ),
            developer_context: env::var("RALPH_DEVELOPER_CONTEXT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
            reviewer_context: env::var("RALPH_REVIEWER_CONTEXT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            verbosity: env::var("RALPH_VERBOSITY")
                .ok()
                .and_then(|s| s.parse::<u8>().ok())
                .map(Verbosity::from)
                .unwrap_or(Verbosity::Verbose),
            commit_msg: "chore: apply PROMPT loop + review/fix/review".to_string(),
            auto_detect_stack: env::var("RALPH_AUTO_DETECT_STACK")
                .ok()
                .and_then(|s| parse_env_bool(&s))
                .unwrap_or(true),
            checkpoint_enabled: env::var("RALPH_CHECKPOINT_ENABLED")
                .ok()
                .and_then(|s| parse_env_bool(&s))
                .unwrap_or(true),
            strict_validation: env::var("RALPH_STRICT_VALIDATION")
                .ok()
                .and_then(|s| parse_env_bool(&s))
                .unwrap_or(false),
            review_depth: env::var("RALPH_REVIEW_DEPTH")
                .ok()
                .and_then(|s| ReviewDepth::from_str(&s))
                .unwrap_or_default(),
            // Isolation mode is ON by default to prevent context contamination.
            // Set RALPH_ISOLATION_MODE=0 to disable (allows NOTES.md/ISSUES.md).
            isolation_mode: env::var("RALPH_ISOLATION_MODE")
                .ok()
                .and_then(|s| parse_env_bool(&s))
                .unwrap_or(true),
        }
    }

    /// Set the commit message
    pub(crate) fn with_commit_msg(mut self, msg: String) -> Self {
        self.commit_msg = msg;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_parse_env_bool() {
        assert_eq!(parse_env_bool("1"), Some(true));
        assert_eq!(parse_env_bool("true"), Some(true));
        assert_eq!(parse_env_bool(" TRUE "), Some(true));
        assert_eq!(parse_env_bool("on"), Some(true));
        assert_eq!(parse_env_bool("yes"), Some(true));

        assert_eq!(parse_env_bool("0"), Some(false));
        assert_eq!(parse_env_bool("false"), Some(false));
        assert_eq!(parse_env_bool(" FALSE "), Some(false));
        assert_eq!(parse_env_bool("off"), Some(false));
        assert_eq!(parse_env_bool("no"), Some(false));

        assert_eq!(parse_env_bool(""), None);
        assert_eq!(parse_env_bool("maybe"), None);
    }

    #[test]
    fn test_config_bool_env_parsing() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Ensure default behavior when unset
        env::remove_var("RALPH_INTERACTIVE");
        let cfg = Config::from_env();
        assert!(cfg.interactive);

        // Accept common truthy values
        env::set_var("RALPH_INTERACTIVE", "true");
        let cfg = Config::from_env();
        assert!(cfg.interactive);

        // Accept common falsy values
        env::set_var("RALPH_INTERACTIVE", "0");
        let cfg = Config::from_env();
        assert!(!cfg.interactive);

        // Clean up
        env::remove_var("RALPH_INTERACTIVE");
    }

    #[test]
    fn test_verbosity_from_u8() {
        assert_eq!(Verbosity::from(0), Verbosity::Quiet);
        assert_eq!(Verbosity::from(1), Verbosity::Normal);
        assert_eq!(Verbosity::from(2), Verbosity::Verbose);
        assert_eq!(Verbosity::from(3), Verbosity::Full);
        assert_eq!(Verbosity::from(4), Verbosity::Debug);
        assert_eq!(Verbosity::from(100), Verbosity::Debug);
    }

    #[test]
    fn test_truncate_limits() {
        // Quiet has reduced limits
        assert_eq!(Verbosity::Quiet.truncate_limit("text"), 80);
        assert_eq!(Verbosity::Quiet.truncate_limit("tool_input"), 40);

        // Normal has increased limits for better usability
        assert_eq!(Verbosity::Normal.truncate_limit("text"), 400);
        assert_eq!(Verbosity::Normal.truncate_limit("tool_input"), 200);

        // Verbose (default) has generous limits for understanding agent behavior
        assert_eq!(Verbosity::Verbose.truncate_limit("text"), 800);
        assert_eq!(Verbosity::Verbose.truncate_limit("tool_input"), 500);

        // Full and Debug have unlimited
        assert_eq!(Verbosity::Full.truncate_limit("text"), 999999);
        assert_eq!(Verbosity::Debug.truncate_limit("text"), 999999);
    }

    #[test]
    fn test_verbosity_helpers() {
        assert!(!Verbosity::Quiet.is_debug());
        assert!(!Verbosity::Normal.is_debug());
        assert!(!Verbosity::Verbose.is_debug());
        assert!(!Verbosity::Full.is_debug());
        assert!(Verbosity::Debug.is_debug());

        assert!(!Verbosity::Quiet.is_verbose());
        assert!(!Verbosity::Normal.is_verbose());
        assert!(Verbosity::Verbose.is_verbose());
        assert!(Verbosity::Full.is_verbose());
        assert!(Verbosity::Debug.is_verbose());

        // show_tool_input: true for Normal and above, false for Quiet
        assert!(!Verbosity::Quiet.show_tool_input());
        assert!(Verbosity::Normal.show_tool_input());
        assert!(Verbosity::Verbose.show_tool_input());
        assert!(Verbosity::Full.show_tool_input());
        assert!(Verbosity::Debug.show_tool_input());
    }

    #[test]
    fn test_config_defaults() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Clear environment variables that might affect defaults
        env::remove_var("RALPH_DEVELOPER_AGENT");
        env::remove_var("RALPH_DRIVER_AGENT");
        env::remove_var("RALPH_REVIEWER_AGENT");
        env::remove_var("RALPH_DEVELOPER_ITERS");
        env::remove_var("RALPH_REVIEWER_REVIEWS");
        env::remove_var("RALPH_VERBOSITY");
        env::remove_var("RALPH_AUTO_DETECT_STACK");
        env::remove_var("RALPH_CHECKPOINT_ENABLED");
        env::remove_var("RALPH_STRICT_VALIDATION");
        env::remove_var("RALPH_REVIEW_DEPTH");

        let config = Config::from_env();
        // Agent selection is NOT hardcoded - it comes from agent_chain in agents.toml
        // If no env var is set, these should be None
        assert!(config.developer_agent.is_none());
        assert!(config.reviewer_agent.is_none());
        assert_eq!(config.developer_iters, 5);
        assert_eq!(config.reviewer_reviews, 2);
        // Default verbosity is now Verbose
        assert_eq!(config.verbosity, Verbosity::Verbose);
        // New config options defaults
        assert!(config.auto_detect_stack);
        assert!(config.checkpoint_enabled);
        assert!(!config.strict_validation);
        // Isolation mode is ON by default to prevent context contamination
        assert!(config.isolation_mode);
    }

    #[test]
    fn test_new_config_options_from_env() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Test auto_detect_stack
        env::set_var("RALPH_AUTO_DETECT_STACK", "false");
        let cfg = Config::from_env();
        assert!(!cfg.auto_detect_stack);
        env::remove_var("RALPH_AUTO_DETECT_STACK");

        // Test checkpoint_enabled
        env::set_var("RALPH_CHECKPOINT_ENABLED", "0");
        let cfg = Config::from_env();
        assert!(!cfg.checkpoint_enabled);
        env::remove_var("RALPH_CHECKPOINT_ENABLED");

        // Test strict_validation
        env::set_var("RALPH_STRICT_VALIDATION", "true");
        let cfg = Config::from_env();
        assert!(cfg.strict_validation);
        env::remove_var("RALPH_STRICT_VALIDATION");
    }

    #[test]
    fn test_review_depth_from_str() {
        // Standard aliases
        assert_eq!(
            ReviewDepth::from_str("standard"),
            Some(ReviewDepth::Standard)
        );
        assert_eq!(
            ReviewDepth::from_str("default"),
            Some(ReviewDepth::Standard)
        );
        assert_eq!(ReviewDepth::from_str("normal"), Some(ReviewDepth::Standard));

        // Comprehensive aliases
        assert_eq!(
            ReviewDepth::from_str("comprehensive"),
            Some(ReviewDepth::Comprehensive)
        );
        assert_eq!(
            ReviewDepth::from_str("thorough"),
            Some(ReviewDepth::Comprehensive)
        );
        assert_eq!(
            ReviewDepth::from_str("full"),
            Some(ReviewDepth::Comprehensive)
        );

        // Security aliases
        assert_eq!(
            ReviewDepth::from_str("security"),
            Some(ReviewDepth::Security)
        );
        assert_eq!(ReviewDepth::from_str("secure"), Some(ReviewDepth::Security));
        assert_eq!(
            ReviewDepth::from_str("security-focused"),
            Some(ReviewDepth::Security)
        );

        // Incremental aliases
        assert_eq!(
            ReviewDepth::from_str("incremental"),
            Some(ReviewDepth::Incremental)
        );
        assert_eq!(
            ReviewDepth::from_str("diff"),
            Some(ReviewDepth::Incremental)
        );
        assert_eq!(
            ReviewDepth::from_str("changed"),
            Some(ReviewDepth::Incremental)
        );

        // Case insensitivity
        assert_eq!(
            ReviewDepth::from_str("SECURITY"),
            Some(ReviewDepth::Security)
        );
        assert_eq!(
            ReviewDepth::from_str("Comprehensive"),
            Some(ReviewDepth::Comprehensive)
        );

        // Invalid values
        assert_eq!(ReviewDepth::from_str("invalid"), None);
        assert_eq!(ReviewDepth::from_str(""), None);
    }

    #[test]
    fn test_review_depth_default() {
        assert_eq!(ReviewDepth::default(), ReviewDepth::Standard);
    }

    #[test]
    fn test_review_depth_description() {
        assert!(ReviewDepth::Standard.description().contains("Balanced"));
        assert!(ReviewDepth::Comprehensive
            .description()
            .contains("In-depth"));
        assert!(ReviewDepth::Security.description().contains("OWASP"));
        assert!(ReviewDepth::Incremental.description().contains("git diff"));
    }

    #[test]
    fn test_review_depth_from_env() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Default is Standard
        env::remove_var("RALPH_REVIEW_DEPTH");
        let cfg = Config::from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Standard);

        // Test comprehensive
        env::set_var("RALPH_REVIEW_DEPTH", "comprehensive");
        let cfg = Config::from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Comprehensive);

        // Test security
        env::set_var("RALPH_REVIEW_DEPTH", "security");
        let cfg = Config::from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Security);

        // Test incremental
        env::set_var("RALPH_REVIEW_DEPTH", "incremental");
        let cfg = Config::from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Incremental);

        // Invalid falls back to default
        env::set_var("RALPH_REVIEW_DEPTH", "invalid_value");
        let cfg = Config::from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Standard);

        env::remove_var("RALPH_REVIEW_DEPTH");
    }

    #[test]
    fn test_isolation_mode_env_parsing() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Default is true (isolation on)
        env::remove_var("RALPH_ISOLATION_MODE");
        let cfg = Config::from_env();
        assert!(cfg.isolation_mode);

        // Can disable with common falsy values
        env::set_var("RALPH_ISOLATION_MODE", "0");
        let cfg = Config::from_env();
        assert!(!cfg.isolation_mode);

        env::set_var("RALPH_ISOLATION_MODE", "false");
        let cfg = Config::from_env();
        assert!(!cfg.isolation_mode);

        // Can explicitly enable
        env::set_var("RALPH_ISOLATION_MODE", "1");
        let cfg = Config::from_env();
        assert!(cfg.isolation_mode);

        // Clean up
        env::remove_var("RALPH_ISOLATION_MODE");
    }
}
