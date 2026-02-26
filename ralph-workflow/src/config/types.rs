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
    /// Commit command override
    pub(crate) commit_cmd: Option<String>,
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
    /// User templates directory for custom template overrides
    /// When set, templates in this directory take priority over embedded templates
    pub(crate) user_templates_dir: Option<PathBuf>,
    /// Developer context level (0=minimal, 1=normal)
    pub(crate) developer_context: u8,
    /// Reviewer context level (0=minimal/fresh eyes, 1=normal)
    pub(crate) reviewer_context: u8,
    /// Verbosity level
    pub(crate) verbosity: Verbosity,
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
    /// Show streaming quality metrics at the end of agent output.
    ///
    /// This field is `pub(crate)` as streaming metrics are an internal concern.
    /// External access is not required; metrics are displayed via CLI flag.
    pub(crate) show_streaming_metrics: bool,
    /// Maximum number of format correction retries during review output parsing.
    /// When the reviewer agent produces unparseable output, the orchestrator will
    /// retry up to this many times with a format correction prompt. Default: 5.
    pub(crate) review_format_retries: u32,
    /// Maximum continuation attempts when developer returns "partial" or "failed".
    ///
    /// # Semantics
    ///
    /// This value counts *continuation attempts* beyond the initial attempt.
    /// Total valid attempts per iteration is `1 + max_dev_continuations`.
    ///
    /// - `0` = no continuations (1 total attempt)
    /// - `2` = two continuations (3 total attempts)
    ///
    /// # Default Behavior
    ///
    /// **CRITICAL:** The system ALWAYS applies a default of 2 (3 total attempts) when this
    /// field is None. The Option wrapper exists ONLY for backward compatibility with direct
    /// Config construction (Config::default(), Config::test_default()).
    ///
    /// When loaded via config_from_unified():
    /// - UnifiedConfig::general.max_dev_continuations has serde default of 2
    /// - Converted to Some(2) in Config
    /// - Applied unconditionally in create_initial_state_with_config()
    ///
    /// This ensures dev loop is ALWAYS bounded, preventing infinite continuation cycles.
    ///
    /// Default: 2 continuations (3 total attempts per iteration).
    pub max_dev_continuations: Option<u32>,
    /// Maximum XSD retry attempts when agent output fails XML validation.
    /// Higher values allow more attempts to fix XML formatting before agent fallback.
    /// Default: 10 (10 retries before falling back to next agent).
    pub max_xsd_retries: Option<u32>,
    /// Maximum same-agent retry attempts for invocation failures that should not
    /// immediately trigger agent fallback (e.g., timeout/internal/unknown and other
    /// non-auth, non-rate-limit failures).
    ///
    /// # Semantics
    ///
    /// This value is a *failure budget* for the current agent: it counts consecutive
    /// failures that are routed through the reducer's same-agent retry path.
    ///
    /// With `max_same_agent_retries = 2`:
    /// - 1st failure → retry the same agent
    /// - 2nd failure → fall back to the next agent
    ///
    /// Default: 2 (one retry before falling back).
    pub max_same_agent_retries: Option<u32>,
    /// Maximum execution history entries to keep in memory (default: 1000).
    /// Prevents unbounded memory growth by dropping oldest entries when limit is reached.
    pub execution_history_limit: usize,
    /// Cloud runtime configuration (internal).
    pub(crate) cloud_config: CloudConfig,
}

/// Cloud runtime configuration (internal).
///
/// This struct is loaded from environment variables when cloud mode is enabled.
#[derive(Debug, Clone, Default)]
pub struct CloudConfig {
    /// Enable cloud reporting mode (internal env-config).
    pub enabled: bool,
    /// Cloud API base URL.
    pub api_url: Option<String>,
    /// Bearer token for API authentication.
    pub api_token: Option<String>,
    /// Run ID assigned by cloud orchestrator.
    pub run_id: Option<String>,
    /// Heartbeat interval in seconds.
    pub heartbeat_interval_secs: u32,
    /// Whether to continue on API failures.
    pub graceful_degradation: bool,
    /// Git remote configuration
    pub git_remote: GitRemoteConfig,
}

/// Git remote configuration (internal).
///
/// Loaded from environment variables when cloud mode is enabled.
#[derive(Debug, Clone)]
pub struct GitRemoteConfig {
    /// Authentication method for git operations
    pub auth_method: GitAuthMethod,
    /// Branch to push to (defaults to current branch)
    pub push_branch: Option<String>,
    /// Whether to create a PR instead of direct push
    pub create_pr: bool,
    /// PR title template (supports {run_id}, {prompt_summary} placeholders)
    pub pr_title_template: Option<String>,
    /// PR body template
    pub pr_body_template: Option<String>,
    /// Base branch for PR (defaults to main/master)
    pub pr_base_branch: Option<String>,
    /// Whether to force push (dangerous, disabled by default)
    pub force_push: bool,
    /// Remote name (defaults to "origin")
    pub remote_name: String,
}

#[derive(Debug, Clone)]
pub enum GitAuthMethod {
    /// Use SSH key (default for containers with mounted keys)
    SshKey {
        /// Path to private key (default: /root/.ssh/id_rsa or SSH_AUTH_SOCK)
        key_path: Option<String>,
    },
    /// Use token-based HTTPS authentication
    Token {
        /// Git token (from RALPH_GIT_TOKEN env var)
        token: String,
        /// Username for token auth (often "oauth2" or "x-access-token")
        username: String,
    },
    /// Use git credential helper (for cloud provider integrations)
    CredentialHelper {
        /// Helper command (e.g., "gcloud", "aws codecommit credential-helper")
        helper: String,
    },
}

/// Cloud configuration that is safe to store in reducer state / checkpoints.
///
/// This is a *redacted* view of [`CloudConfig`]: it carries only non-sensitive
/// fields required for pure orchestration.
///
/// In particular, it MUST NOT contain API tokens, git tokens, or any other
/// credential material.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CloudStateConfig {
    pub enabled: bool,
    pub api_url: Option<String>,
    pub run_id: Option<String>,
    pub heartbeat_interval_secs: u32,
    pub graceful_degradation: bool,
    pub git_remote: GitRemoteStateConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitRemoteStateConfig {
    pub auth_method: GitAuthStateMethod,
    pub push_branch: String,
    pub create_pr: bool,
    pub pr_title_template: Option<String>,
    pub pr_body_template: Option<String>,
    pub pr_base_branch: Option<String>,
    pub force_push: bool,
    pub remote_name: String,
}

impl Default for GitRemoteStateConfig {
    fn default() -> Self {
        Self {
            auth_method: GitAuthStateMethod::SshKey { key_path: None },
            push_branch: String::new(),
            create_pr: false,
            pr_title_template: None,
            pr_body_template: None,
            pr_base_branch: None,
            force_push: false,
            remote_name: "origin".to_string(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum GitAuthStateMethod {
    SshKey { key_path: Option<String> },
    Token { username: String },
    CredentialHelper { helper: String },
}

impl Default for GitAuthStateMethod {
    fn default() -> Self {
        Self::SshKey { key_path: None }
    }
}

impl CloudStateConfig {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            api_url: None,
            run_id: None,
            heartbeat_interval_secs: 30,
            graceful_degradation: true,
            git_remote: GitRemoteStateConfig::default(),
        }
    }
}

impl From<&CloudConfig> for CloudStateConfig {
    fn from(cfg: &CloudConfig) -> Self {
        let auth_method = match &cfg.git_remote.auth_method {
            GitAuthMethod::SshKey { key_path } => GitAuthStateMethod::SshKey {
                key_path: key_path.clone(),
            },
            GitAuthMethod::Token { username, .. } => GitAuthStateMethod::Token {
                username: username.clone(),
            },
            GitAuthMethod::CredentialHelper { helper } => GitAuthStateMethod::CredentialHelper {
                helper: helper.clone(),
            },
        };

        Self {
            enabled: cfg.enabled,
            api_url: cfg.api_url.clone(),
            run_id: cfg.run_id.clone(),
            heartbeat_interval_secs: cfg.heartbeat_interval_secs,
            graceful_degradation: cfg.graceful_degradation,
            git_remote: GitRemoteStateConfig {
                auth_method,
                push_branch: cfg.git_remote.push_branch.clone().unwrap_or_default(),
                create_pr: cfg.git_remote.create_pr,
                pr_title_template: cfg.git_remote.pr_title_template.clone(),
                pr_body_template: cfg.git_remote.pr_body_template.clone(),
                pr_base_branch: cfg.git_remote.pr_base_branch.clone(),
                force_push: cfg.git_remote.force_push,
                remote_name: cfg.git_remote.remote_name.clone(),
            },
        }
    }
}

impl Default for GitAuthMethod {
    fn default() -> Self {
        Self::SshKey { key_path: None }
    }
}

impl Default for GitRemoteConfig {
    fn default() -> Self {
        Self {
            auth_method: GitAuthMethod::default(),
            push_branch: None,
            create_pr: false,
            pr_title_template: None,
            pr_body_template: None,
            pr_base_branch: None,
            force_push: false,
            remote_name: "origin".to_string(),
        }
    }
}

impl CloudConfig {
    /// Load cloud config from environment variables ONLY.
    /// Returns disabled config when cloud mode is not enabled.
    pub fn from_env() -> Self {
        let enabled = std::env::var("RALPH_CLOUD_MODE")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);

        if !enabled {
            return Self::disabled();
        }

        Self {
            enabled: true,
            api_url: std::env::var("RALPH_CLOUD_API_URL").ok(),
            api_token: std::env::var("RALPH_CLOUD_API_TOKEN").ok(),
            run_id: std::env::var("RALPH_CLOUD_RUN_ID").ok(),
            heartbeat_interval_secs: std::env::var("RALPH_CLOUD_HEARTBEAT_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            graceful_degradation: std::env::var("RALPH_CLOUD_GRACEFUL_DEGRADATION")
                .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
                .unwrap_or(true),
            git_remote: GitRemoteConfig::from_env(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            api_url: None,
            api_token: None,
            run_id: None,
            heartbeat_interval_secs: 30,
            graceful_degradation: true,
            git_remote: GitRemoteConfig::default(),
        }
    }

    /// Validate that required fields are present when enabled.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        let Some(api_url) = self.api_url.as_deref() else {
            return Err("RALPH_CLOUD_API_URL must be set when cloud mode is enabled".to_string());
        };
        if !api_url
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("https://")
        {
            return Err(
                "RALPH_CLOUD_API_URL must use https:// when cloud mode is enabled".to_string(),
            );
        }

        if self.api_token.as_deref().unwrap_or_default().is_empty() {
            return Err("RALPH_CLOUD_API_TOKEN must be set when cloud mode is enabled".to_string());
        }

        if self.run_id.as_deref().unwrap_or_default().is_empty() {
            return Err("RALPH_CLOUD_RUN_ID must be set when cloud mode is enabled".to_string());
        }

        // Validate git remote config when cloud mode is enabled.
        self.git_remote.validate()?;

        Ok(())
    }
}

impl GitRemoteConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.remote_name.trim().is_empty() {
            return Err("RALPH_GIT_REMOTE must not be empty".to_string());
        }

        if let Some(branch) = self.push_branch.as_deref() {
            let trimmed = branch.trim();
            if trimmed.is_empty() {
                return Err("RALPH_GIT_PUSH_BRANCH must not be empty when set".to_string());
            }
            if trimmed == "HEAD" {
                return Err(
                    "RALPH_GIT_PUSH_BRANCH must be a branch name (not literal 'HEAD')".to_string(),
                );
            }
        }

        match &self.auth_method {
            GitAuthMethod::SshKey { key_path } => {
                if let Some(path) = key_path.as_deref() {
                    if path.trim().is_empty() {
                        return Err("RALPH_GIT_SSH_KEY_PATH must not be empty when set".to_string());
                    }
                }
            }
            GitAuthMethod::Token { token, username } => {
                if token.trim().is_empty() {
                    return Err(
                        "RALPH_GIT_TOKEN must be set when RALPH_GIT_AUTH_METHOD=token".to_string(),
                    );
                }
                if username.trim().is_empty() {
                    return Err(
                        "RALPH_GIT_TOKEN_USERNAME must not be empty when RALPH_GIT_AUTH_METHOD=token"
                            .to_string(),
                    );
                }
            }
            GitAuthMethod::CredentialHelper { helper } => {
                if helper.trim().is_empty() {
                    return Err(
                        "RALPH_GIT_CREDENTIAL_HELPER must be set when RALPH_GIT_AUTH_METHOD=credential-helper"
                            .to_string(),
                    );
                }
            }
        }

        Ok(())
    }
}

impl GitRemoteConfig {
    pub fn from_env() -> Self {
        let auth_method = match std::env::var("RALPH_GIT_AUTH_METHOD")
            .unwrap_or_else(|_| "ssh".to_string())
            .to_lowercase()
            .as_str()
        {
            "token" => {
                let token = std::env::var("RALPH_GIT_TOKEN").unwrap_or_default();
                let username = std::env::var("RALPH_GIT_TOKEN_USERNAME")
                    .unwrap_or_else(|_| "x-access-token".to_string());
                GitAuthMethod::Token { token, username }
            }
            "credential-helper" => {
                let helper = std::env::var("RALPH_GIT_CREDENTIAL_HELPER")
                    .unwrap_or_else(|_| "gcloud".to_string());
                GitAuthMethod::CredentialHelper { helper }
            }
            _ => {
                let key_path = std::env::var("RALPH_GIT_SSH_KEY_PATH").ok();
                GitAuthMethod::SshKey { key_path }
            }
        };

        Self {
            auth_method,
            push_branch: std::env::var("RALPH_GIT_PUSH_BRANCH").ok(),
            create_pr: std::env::var("RALPH_GIT_CREATE_PR")
                .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                .unwrap_or(false),
            pr_title_template: std::env::var("RALPH_GIT_PR_TITLE").ok(),
            pr_body_template: std::env::var("RALPH_GIT_PR_BODY").ok(),
            pr_base_branch: std::env::var("RALPH_GIT_PR_BASE_BRANCH").ok(),
            force_push: std::env::var("RALPH_GIT_FORCE_PUSH")
                .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                .unwrap_or(false),
            remote_name: std::env::var("RALPH_GIT_REMOTE").unwrap_or_else(|_| "origin".to_string()),
        }
    }
}

impl Config {
    /// Get the user templates directory.
    #[must_use]
    pub const fn user_templates_dir(&self) -> Option<&std::path::PathBuf> {
        self.user_templates_dir.as_ref()
    }

    /// Create a test-appropriate Config with safe defaults.
    ///
    /// This function creates a Config suitable for integration tests,
    /// with all agent execution disabled and isolation mode enabled.
    /// It does NOT load from environment variables or config files.
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            developer_agent: Some("codex".to_string()),
            reviewer_agent: Some("codex".to_string()),
            developer_cmd: None,
            reviewer_cmd: None,
            commit_cmd: None,
            developer_model: None,
            reviewer_model: None,
            developer_provider: None,
            reviewer_provider: None,
            reviewer_json_parser: None,
            features: FeatureFlags {
                checkpoint_enabled: true,
                force_universal_prompt: false,
            },
            developer_iters: 0,
            reviewer_reviews: 0,
            fast_check_cmd: None,
            full_check_cmd: None,
            behavior: BehavioralFlags {
                interactive: false,
                auto_detect_stack: false,
                strict_validation: false,
            },
            prompt_path: PathBuf::from(".agent/last_prompt.txt"),
            user_templates_dir: None,
            developer_context: 0,
            reviewer_context: 0,
            verbosity: Verbosity::Quiet,
            review_depth: ReviewDepth::Standard,
            isolation_mode: true,
            git_user_name: Some("Test".to_string()),
            git_user_email: Some("test@example.com".to_string()),
            show_streaming_metrics: false,
            review_format_retries: 5,
            max_dev_continuations: Some(2),
            max_xsd_retries: Some(10),
            max_same_agent_retries: Some(2),
            execution_history_limit: 1000,
            cloud_config: CloudConfig::disabled(),
        }
    }

    /// Set isolation mode and return self (builder pattern).
    #[must_use]
    pub fn with_isolation_mode(mut self, isolation_mode: bool) -> Self {
        self.isolation_mode = isolation_mode;
        self
    }

    /// Set developer iterations and return self (builder pattern).
    #[must_use]
    pub fn with_developer_iters(mut self, iters: u32) -> Self {
        self.developer_iters = iters;
        self
    }

    /// Set reviewer reviews and return self (builder pattern).
    #[must_use]
    pub fn with_reviewer_reviews(mut self, reviews: u32) -> Self {
        self.reviewer_reviews = reviews;
        self
    }

    /// Set auto_detect_stack and return self (builder pattern).
    #[must_use]
    pub fn with_auto_detect_stack(mut self, auto_detect: bool) -> Self {
        self.behavior.auto_detect_stack = auto_detect;
        self
    }

    /// Set verbosity and return self (builder pattern).
    #[must_use]
    pub fn with_verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    /// Set review_depth and return self (builder pattern).
    #[must_use]
    pub fn with_review_depth(mut self, review_depth: ReviewDepth) -> Self {
        self.review_depth = review_depth;
        self
    }

    /// Set developer_agent and return self (builder pattern).
    #[must_use]
    pub fn with_developer_agent(mut self, agent: String) -> Self {
        self.developer_agent = Some(agent);
        self
    }

    /// Set reviewer_agent and return self (builder pattern).
    #[must_use]
    pub fn with_reviewer_agent(mut self, agent: String) -> Self {
        self.reviewer_agent = Some(agent);
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        super::loader::default_config()
    }
}

#[cfg(test)]
mod cloud_config_tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_cloud_config_disabled_by_default() {
        std::env::remove_var("RALPH_CLOUD_MODE");
        let config = CloudConfig::from_env();
        assert!(!config.enabled);
    }

    #[test]
    #[serial]
    fn test_cloud_config_enabled_with_env_var() {
        std::env::set_var("RALPH_CLOUD_MODE", "true");
        std::env::set_var("RALPH_CLOUD_API_URL", "https://api.example.com");
        std::env::set_var("RALPH_CLOUD_API_TOKEN", "secret");
        std::env::set_var("RALPH_CLOUD_RUN_ID", "run123");

        let config = CloudConfig::from_env();
        assert!(config.enabled);
        assert_eq!(config.api_url, Some("https://api.example.com".to_string()));
        assert_eq!(config.run_id, Some("run123".to_string()));

        std::env::remove_var("RALPH_CLOUD_MODE");
        std::env::remove_var("RALPH_CLOUD_API_URL");
        std::env::remove_var("RALPH_CLOUD_API_TOKEN");
        std::env::remove_var("RALPH_CLOUD_RUN_ID");
    }

    #[test]
    #[serial]
    fn test_cloud_config_validation_requires_fields() {
        let config = CloudConfig {
            enabled: true,
            api_url: None,
            api_token: None,
            run_id: None,
            heartbeat_interval_secs: 30,
            graceful_degradation: true,
            git_remote: GitRemoteConfig::default(),
        };

        assert!(config.validate().is_err());
    }

    #[test]
    #[serial]
    fn test_git_auth_method_from_env() {
        std::env::set_var("RALPH_GIT_AUTH_METHOD", "token");
        std::env::set_var("RALPH_GIT_TOKEN", "ghp_test");

        let config = GitRemoteConfig::from_env();
        match config.auth_method {
            GitAuthMethod::Token { token, .. } => {
                assert_eq!(token, "ghp_test");
            }
            _ => panic!("Expected Token auth method"),
        }

        std::env::remove_var("RALPH_GIT_AUTH_METHOD");
        std::env::remove_var("RALPH_GIT_TOKEN");
    }

    #[test]
    fn test_cloud_config_disabled_validation_passes() {
        let config = CloudConfig::disabled();
        assert!(
            config.validate().is_ok(),
            "Disabled cloud config should always validate"
        );
    }

    #[test]
    fn test_cloud_config_validation_rejects_non_https_api_url() {
        let config = CloudConfig {
            enabled: true,
            api_url: Some("http://api.example.com".to_string()),
            api_token: Some("secret".to_string()),
            run_id: Some("run123".to_string()),
            heartbeat_interval_secs: 30,
            graceful_degradation: true,
            git_remote: GitRemoteConfig::default(),
        };
        assert!(
            config.validate().is_err(),
            "Cloud API URL must be https:// when cloud mode is enabled"
        );
    }

    #[test]
    fn test_cloud_config_validation_requires_git_token_for_token_auth() {
        let config = CloudConfig {
            enabled: true,
            api_url: Some("https://api.example.com".to_string()),
            api_token: Some("secret".to_string()),
            run_id: Some("run123".to_string()),
            heartbeat_interval_secs: 30,
            graceful_degradation: true,
            git_remote: GitRemoteConfig {
                auth_method: GitAuthMethod::Token {
                    token: "".to_string(),
                    username: "x-access-token".to_string(),
                },
                ..GitRemoteConfig::default()
            },
        };
        assert!(
            config.validate().is_err(),
            "Token auth requires a non-empty RALPH_GIT_TOKEN"
        );
    }
}
