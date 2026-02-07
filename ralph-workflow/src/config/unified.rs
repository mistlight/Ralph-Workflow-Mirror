//! Unified Configuration Types
//!
//! This module defines the unified configuration format for Ralph,
//! consolidating all settings into a single `~/.config/ralph-workflow.toml` file.
//!
//! # Configuration Structure
//!
//! ```toml
//! [general]
//! verbosity = 2
//! interactive = true
//! isolation_mode = true
//!
//! [agents.claude]
//! cmd = "claude -p"
//! # ...
//!
//! [ccs_aliases]
//! work = "ccs work"
//! personal = "ccs personal"
//!
//! [agent_chain]
//! developer = ["ccs/work", "claude"]
//! reviewer = ["claude"]
//! ```

use crate::agents::fallback::FallbackConfig;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::io;
use std::path::PathBuf;

/// Default unified config template embedded at compile time.
pub const DEFAULT_UNIFIED_CONFIG: &str = include_str!("../../examples/ralph-workflow.toml");

/// Result of config initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigInitResult {
    /// Config was created successfully.
    Created,
    /// Config already exists.
    AlreadyExists,
}

/// Default path for the unified configuration file.
pub const DEFAULT_UNIFIED_CONFIG_NAME: &str = "ralph-workflow.toml";

/// Get the path to the unified config file.
///
/// Returns `~/.config/ralph-workflow.toml` by default.
///
/// If `XDG_CONFIG_HOME` is set, uses `{XDG_CONFIG_HOME}/ralph-workflow.toml`.
pub fn unified_config_path() -> Option<PathBuf> {
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        let xdg = xdg.trim();
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join(DEFAULT_UNIFIED_CONFIG_NAME));
        }
    }

    dirs::home_dir().map(|d| d.join(".config").join(DEFAULT_UNIFIED_CONFIG_NAME))
}

/// General configuration behavioral flags.
///
/// Groups user interaction and validation-related boolean settings for `GeneralConfig`.
#[derive(Debug, Clone, Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct GeneralBehaviorFlags {
    /// Interactive mode (keep agent in foreground).
    pub interactive: bool,
    /// Auto-detect project stack for review guidelines.
    pub auto_detect_stack: bool,
    /// Strict PROMPT.md validation.
    pub strict_validation: bool,
}

/// General configuration workflow automation flags.
///
/// Groups workflow automation features for `GeneralConfig`.
#[derive(Debug, Clone, Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct GeneralWorkflowFlags {
    /// Enable checkpoint/resume functionality.
    pub checkpoint_enabled: bool,
}

/// General configuration execution behavior flags.
///
/// Groups execution behavior settings for `GeneralConfig`.
#[derive(Debug, Clone, Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct GeneralExecutionFlags {
    /// Force universal review prompt for all agents.
    pub force_universal_prompt: bool,
    /// Isolation mode (prevent context contamination).
    pub isolation_mode: bool,
}

/// General configuration section.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(default)]
// Configuration options naturally use many boolean flags. These represent
// independent feature toggles, not a state machine, so bools are appropriate.
pub struct GeneralConfig {
    /// Verbosity level (0-4).
    pub verbosity: u8,
    /// Behavioral flags (interactive, auto-detect, strict validation)
    #[serde(default)]
    pub behavior: GeneralBehaviorFlags,
    /// Workflow automation flags (checkpoint, auto-rebase)
    #[serde(default, flatten)]
    pub workflow: GeneralWorkflowFlags,
    /// Execution behavior flags (universal prompt, isolation mode)
    #[serde(default, flatten)]
    pub execution: GeneralExecutionFlags,
    /// Number of developer iterations.
    pub developer_iters: u32,
    /// Number of reviewer re-review passes.
    pub reviewer_reviews: u32,
    /// Developer context level.
    pub developer_context: u8,
    /// Reviewer context level.
    pub reviewer_context: u8,
    /// Review depth level.
    #[serde(default)]
    pub review_depth: String,
    /// Path to save last prompt.
    #[serde(default)]
    pub prompt_path: Option<String>,
    /// User templates directory for custom template overrides.
    /// When set, templates in this directory take priority over embedded templates.
    #[serde(default)]
    pub templates_dir: Option<String>,
    /// Git user name for commits (optional, falls back to git config).
    #[serde(default)]
    pub git_user_name: Option<String>,
    /// Git user email for commits (optional, falls back to git config).
    #[serde(default)]
    pub git_user_email: Option<String>,
    /// Maximum continuation attempts when developer returns "partial" or "failed".
    ///
    /// Higher values allow more attempts to complete complex tasks within a single plan.
    ///
    /// Semantics: this value counts *continuation attempts* (fresh sessions) beyond the initial
    /// attempt. Total valid attempts per iteration is `1 + max_dev_continuations`.
    ///
    /// Default: 2 continuations (initial attempt + 2 continuations = 3 total attempts per iteration).
    #[serde(default = "default_max_dev_continuations")]
    pub max_dev_continuations: u32,
    /// Maximum XSD retry attempts when agent output fails XML validation.
    ///
    /// Higher values allow more attempts to fix XML formatting issues before
    /// switching to the next agent in the fallback chain.
    ///
    /// Default: 10 retries before falling back to the next agent.
    #[serde(default = "default_max_xsd_retries")]
    pub max_xsd_retries: u32,
    /// Maximum same-agent retry attempts for transient invocation failures (timeout/internal).
    ///
    /// Semantics: this is a *failure budget* for the current agent. With a value of `2`:
    /// 1st failure → retry the same agent; 2nd failure → fall back to the next agent.
    ///
    /// Default: 2 (one retry before falling back).
    #[serde(default = "default_max_same_agent_retries")]
    pub max_same_agent_retries: u32,
}

/// Default maximum continuation attempts per development iteration.
///
/// This allows 2 continuations per iteration (3 total valid attempts including the initial)
/// for fast iteration cycles.
fn default_max_dev_continuations() -> u32 {
    2
}

/// Default maximum XSD retry attempts before agent fallback.
///
/// This allows 10 retries to fix XML formatting issues before switching agents.
fn default_max_xsd_retries() -> u32 {
    10
}

fn default_max_same_agent_retries() -> u32 {
    2
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            verbosity: 2, // Verbose
            behavior: GeneralBehaviorFlags {
                interactive: true,
                auto_detect_stack: true,
                strict_validation: false,
            },
            workflow: GeneralWorkflowFlags {
                checkpoint_enabled: true,
            },
            execution: GeneralExecutionFlags {
                force_universal_prompt: false,
                isolation_mode: true,
            },
            developer_iters: 5,
            reviewer_reviews: 2,
            developer_context: 1,
            reviewer_context: 0,
            review_depth: "standard".to_string(),
            prompt_path: None,
            templates_dir: None,
            git_user_name: None,
            git_user_email: None,
            max_dev_continuations: default_max_dev_continuations(),
            max_xsd_retries: default_max_xsd_retries(),
            max_same_agent_retries: default_max_same_agent_retries(),
        }
    }
}

/// CCS (Claude Code Switch) alias configuration.
///
/// Maps alias names to CCS profile commands.
/// For example: `work = "ccs work"` allows using `ccs/work` as an agent.
pub type CcsAliases = HashMap<String, CcsAliasToml>;

/// CCS defaults applied to all CCS aliases unless overridden per-alias.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct CcsConfig {
    /// Output-format flag for CCS (often Claude-compatible stream JSON).
    pub output_flag: String,
    /// Flag for autonomous mode (skip permission/confirmation prompts).
    /// Ralph is designed for unattended automation, so this is enabled by default.
    /// Set to empty string ("") to disable and require confirmations.
    pub yolo_flag: String,
    /// Flag for verbose output.
    pub verbose_flag: String,
    /// Print flag for non-interactive mode.
    ///
    /// IMPORTANT: CCS treats `-p` / `--prompt` as *its own* headless delegation mode.
    /// When we execute via the `ccs` wrapper (e.g. `ccs codex`), we must use
    /// Claude's long-form `--print` flag to avoid triggering CCS delegation.
    ///
    /// Default: "--print"
    pub print_flag: String,
    /// Streaming flag for JSON output with -p (required for Claude/CCS to stream).
    /// Default: "--include-partial-messages"
    pub streaming_flag: String,
    /// Which JSON parser to use for CCS output.
    pub json_parser: String,
    /// Session continuation flag template for CCS aliases (Claude CLI).
    /// The `{}` placeholder is replaced with the session ID at runtime.
    ///
    /// Default: "--resume {}"
    pub session_flag: String,
    /// Whether CCS can run workflow tools (git commit, etc.).
    pub can_commit: bool,
}

impl Default for CcsConfig {
    fn default() -> Self {
        Self {
            output_flag: "--output-format=stream-json".to_string(),
            // Default to unattended automation (config can override to disable).
            yolo_flag: "--dangerously-skip-permissions".to_string(),
            verbose_flag: "--verbose".to_string(),
            print_flag: "--print".to_string(),
            streaming_flag: "--include-partial-messages".to_string(),
            json_parser: "claude".to_string(),
            session_flag: "--resume {}".to_string(),
            can_commit: true,
        }
    }
}

/// Per-alias CCS configuration (table form).
#[derive(Debug, Clone, Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct CcsAliasConfig {
    /// Base CCS command to run (e.g., "ccs work", "ccs gemini").
    pub cmd: String,
    /// Optional output flag override for this alias. Use "" to disable.
    pub output_flag: Option<String>,
    /// Optional yolo flag override for this alias. Use "" to enable/disable explicitly.
    pub yolo_flag: Option<String>,
    /// Optional verbose flag override for this alias. Use "" to disable.
    pub verbose_flag: Option<String>,
    /// Optional print flag override for this alias (e.g., "-p" for Claude/CCS).
    pub print_flag: Option<String>,
    /// Optional streaming flag override for this alias (e.g., "--include-partial-messages").
    pub streaming_flag: Option<String>,
    /// Optional JSON parser override (e.g., "claude", "generic").
    pub json_parser: Option<String>,
    /// Optional `can_commit` override for this alias.
    pub can_commit: Option<bool>,
    /// Optional model flag appended to the command.
    pub model_flag: Option<String>,
    /// Optional session continuation flag (e.g., "--resume {}" for Claude CLI).
    /// The "{}" placeholder is replaced with the session ID.
    pub session_flag: Option<String>,
}

/// CCS alias entry supports both shorthand string and table form.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum CcsAliasToml {
    Command(String),
    Config(CcsAliasConfig),
}

impl CcsAliasToml {
    pub fn as_config(&self) -> CcsAliasConfig {
        match self {
            Self::Command(cmd) => CcsAliasConfig {
                cmd: cmd.clone(),
                ..CcsAliasConfig::default()
            },
            Self::Config(cfg) => cfg.clone(),
        }
    }
}

/// Agent TOML configuration (compatible with `examples/agents.toml`).
///
/// Fields are used via serde deserialization.
#[derive(Debug, Clone, Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct AgentConfigToml {
    /// Base command to run the agent.
    ///
    /// When overriding a built-in agent, this may be omitted to keep the built-in command.
    pub cmd: Option<String>,
    /// Output-format flag.
    ///
    /// Omitted means "keep built-in default". Empty string explicitly disables output flag.
    pub output_flag: Option<String>,
    /// Flag for autonomous mode.
    ///
    /// Omitted means "keep built-in default". Empty string explicitly disables yolo mode.
    pub yolo_flag: Option<String>,
    /// Flag for verbose output.
    ///
    /// Omitted means "keep built-in default". Empty string explicitly disables verbose flag.
    pub verbose_flag: Option<String>,
    /// Print/non-interactive mode flag (e.g., "-p" for Claude/CCS).
    ///
    /// Omitted means "keep built-in default". Empty string explicitly disables print mode.
    pub print_flag: Option<String>,
    /// Include partial messages flag for streaming with -p (e.g., "--include-partial-messages").
    ///
    /// Omitted means "keep built-in default". Empty string explicitly disables streaming flag.
    pub streaming_flag: Option<String>,
    /// Session continuation flag template (e.g., "-s {}" for OpenCode, "--resume {}" for Claude).
    /// The `{}` placeholder is replaced with the session ID at runtime.
    ///
    /// Omitted means "keep built-in default". Empty string explicitly disables session continuation.
    /// See agent documentation for correct flag format:
    /// - Claude: --resume <session_id> (from `claude --help`)
    /// - OpenCode: -s <session_id> (from `opencode run --help`)
    pub session_flag: Option<String>,
    /// Whether the agent can run git commit.
    ///
    /// Omitted means "keep built-in default". For new agents, this defaults to true when omitted.
    pub can_commit: Option<bool>,
    /// Which JSON parser to use.
    ///
    /// Omitted means "keep built-in default". For new agents, defaults to "generic" when omitted.
    pub json_parser: Option<String>,
    /// Model/provider flag.
    pub model_flag: Option<String>,
    /// Human-readable display name for UI/UX.
    ///
    /// Omitted means "keep built-in default". Empty string explicitly clears the display name.
    pub display_name: Option<String>,
}

/// Unified configuration file structure.
///
/// This is the sole source of truth for Ralph configuration,
/// located at `~/.config/ralph-workflow.toml`.
#[derive(Debug, Clone, Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct UnifiedConfig {
    /// General settings.
    pub general: GeneralConfig,
    /// CCS defaults for aliases.
    pub ccs: CcsConfig,
    /// Agent definitions (used via serde deserialization for future expansion).
    #[serde(default)]
    pub agents: HashMap<String, AgentConfigToml>,
    /// CCS alias mappings.
    #[serde(default)]
    pub ccs_aliases: CcsAliases,
    /// Agent chain configuration.
    ///
    /// When omitted, Ralph uses built-in defaults.
    #[serde(default, rename = "agent_chain")]
    pub agent_chain: Option<FallbackConfig>,
}

impl UnifiedConfig {
    /// Load unified configuration from the default path.
    ///
    /// Returns None if the file doesn't exist.
    ///
    pub fn load_default() -> Option<Self> {
        Self::load_with_env(&super::path_resolver::RealConfigEnvironment)
    }

    /// Load unified configuration using a `ConfigEnvironment`.
    ///
    /// This is the testable version of `load_default`. It reads from the
    /// unified config path as determined by the environment.
    ///
    /// Returns None if no config path is available or the file doesn't exist.
    pub fn load_with_env(env: &dyn super::path_resolver::ConfigEnvironment) -> Option<Self> {
        env.unified_config_path().and_then(|path| {
            if env.file_exists(&path) {
                Self::load_from_path_with_env(&path, env).ok()
            } else {
                None
            }
        })
    }

    /// Load unified configuration from a specific path.
    ///
    /// **Note:** This method uses `std::fs` directly. For testable code,
    /// use `load_from_path_with_env` with a `ConfigEnvironment` instead.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, ConfigLoadError> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Load unified configuration from a specific path using a `ConfigEnvironment`.
    ///
    /// This is the testable version of `load_from_path`.
    pub fn load_from_path_with_env(
        path: &std::path::Path,
        env: &dyn super::path_resolver::ConfigEnvironment,
    ) -> Result<Self, ConfigLoadError> {
        let contents = env.read_file(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Merge local config into self (global), returning merged config.
    ///
    /// Local values override global values with these semantics:
    /// - Scalar values: local replaces global
    /// - Maps (agents, ccs_aliases): local entries merge with global (local wins on collision)
    /// - Arrays (agent_chain): local replaces global entirely (not appended)
    /// - Optional values: local Some(_) replaces global, local None preserves global
    ///
    /// This is a pure function - no I/O, cannot fail.
    pub fn merge_with(&self, local: &UnifiedConfig) -> UnifiedConfig {
        // Merge general config (scalar overrides)
        let general = GeneralConfig {
            verbosity: local.general.verbosity,
            behavior: GeneralBehaviorFlags {
                interactive: local.general.behavior.interactive,
                auto_detect_stack: local.general.behavior.auto_detect_stack,
                strict_validation: local.general.behavior.strict_validation,
            },
            workflow: GeneralWorkflowFlags {
                checkpoint_enabled: local.general.workflow.checkpoint_enabled,
            },
            execution: GeneralExecutionFlags {
                force_universal_prompt: local.general.execution.force_universal_prompt,
                isolation_mode: local.general.execution.isolation_mode,
            },
            developer_iters: local.general.developer_iters,
            reviewer_reviews: local.general.reviewer_reviews,
            developer_context: local.general.developer_context,
            reviewer_context: local.general.reviewer_context,
            review_depth: local.general.review_depth.clone(),
            prompt_path: local
                .general
                .prompt_path
                .clone()
                .or_else(|| self.general.prompt_path.clone()),
            templates_dir: local
                .general
                .templates_dir
                .clone()
                .or_else(|| self.general.templates_dir.clone()),
            git_user_name: local
                .general
                .git_user_name
                .clone()
                .or_else(|| self.general.git_user_name.clone()),
            git_user_email: local
                .general
                .git_user_email
                .clone()
                .or_else(|| self.general.git_user_email.clone()),
            max_dev_continuations: local.general.max_dev_continuations,
            max_xsd_retries: local.general.max_xsd_retries,
            max_same_agent_retries: local.general.max_same_agent_retries,
        };

        // Merge CCS config (scalar overrides)
        let ccs = CcsConfig {
            output_flag: local.ccs.output_flag.clone(),
            yolo_flag: local.ccs.yolo_flag.clone(),
            verbose_flag: local.ccs.verbose_flag.clone(),
            print_flag: local.ccs.print_flag.clone(),
            streaming_flag: local.ccs.streaming_flag.clone(),
            json_parser: local.ccs.json_parser.clone(),
            session_flag: local.ccs.session_flag.clone(),
            can_commit: local.ccs.can_commit,
        };

        // Merge agents map (local entries override global entries)
        let mut agents = self.agents.clone();
        for (key, value) in &local.agents {
            agents.insert(key.clone(), value.clone());
        }

        // Merge CCS aliases map (local entries override global entries)
        let mut ccs_aliases = self.ccs_aliases.clone();
        for (key, value) in &local.ccs_aliases {
            ccs_aliases.insert(key.clone(), value.clone());
        }

        // Agent chain: local replaces global entirely (not merged)
        let agent_chain = if local.agent_chain.is_some() {
            local.agent_chain.clone()
        } else {
            self.agent_chain.clone()
        };

        UnifiedConfig {
            general,
            ccs,
            agents,
            ccs_aliases,
            agent_chain,
        }
    }

    /// Ensure unified config file exists, creating it from template if needed.
    ///
    /// This creates `~/.config/ralph-workflow.toml` with the default template
    /// if it doesn't already exist.
    ///
    pub fn ensure_config_exists() -> io::Result<ConfigInitResult> {
        Self::ensure_config_exists_with_env(&super::path_resolver::RealConfigEnvironment)
    }

    /// Ensure unified config file exists using a `ConfigEnvironment`.
    ///
    /// This is the testable version of `ensure_config_exists`.
    pub fn ensure_config_exists_with_env(
        env: &dyn super::path_resolver::ConfigEnvironment,
    ) -> io::Result<ConfigInitResult> {
        let Some(path) = env.unified_config_path() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Cannot determine config directory (no home directory)",
            ));
        };

        Self::ensure_config_exists_at_with_env(&path, env)
    }

    /// Ensure a config file exists at the specified path.
    ///
    pub fn ensure_config_exists_at(path: &std::path::Path) -> io::Result<ConfigInitResult> {
        Self::ensure_config_exists_at_with_env(path, &super::path_resolver::RealConfigEnvironment)
    }

    /// Ensure a config file exists at the specified path using a `ConfigEnvironment`.
    ///
    /// This is the testable version of `ensure_config_exists_at`.
    pub fn ensure_config_exists_at_with_env(
        path: &std::path::Path,
        env: &dyn super::path_resolver::ConfigEnvironment,
    ) -> io::Result<ConfigInitResult> {
        if env.file_exists(path) {
            return Ok(ConfigInitResult::AlreadyExists);
        }

        // Write the default template (write_file creates parent directories)
        env.write_file(path, DEFAULT_UNIFIED_CONFIG)?;

        Ok(ConfigInitResult::Created)
    }
}

/// Error type for unified config loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigLoadError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
}

#[cfg(test)]
mod tests;
