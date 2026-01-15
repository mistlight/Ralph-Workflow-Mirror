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
use std::fs;
use std::io;
use std::path::PathBuf;

/// Default unified config template embedded at compile time.
pub const DEFAULT_UNIFIED_CONFIG: &str = include_str!("../../../examples/ralph-workflow.toml");

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
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GeneralBehaviorFlags {
    /// Interactive mode (keep agent in foreground).
    pub interactive: bool,
    /// Auto-detect project stack for review guidelines.
    pub auto_detect_stack: bool,
    /// Strict PROMPT.md validation.
    pub strict_validation: bool,
}

/// General configuration feature flags.
///
/// Groups optional feature toggle settings for `GeneralConfig`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GeneralFeatureFlags {
    /// Enable checkpoint/resume functionality.
    pub checkpoint_enabled: bool,
    /// Force universal review prompt for all agents.
    pub force_universal_prompt: bool,
    /// Isolation mode (prevent context contamination).
    pub isolation_mode: bool,
}

/// General configuration section.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
// Configuration options naturally use many boolean flags. These represent
// independent feature toggles, not a state machine, so bools are appropriate.
pub struct GeneralConfig {
    /// Verbosity level (0-4).
    pub verbosity: u8,
    /// Behavioral flags (interactive, auto-detect, strict validation)
    #[serde(default)]
    pub behavior: GeneralBehaviorFlags,
    /// Feature flags (checkpoint, universal prompt, isolation mode)
    #[serde(default)]
    pub features: GeneralFeatureFlags,
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
    /// Git user name for commits (optional, falls back to git config).
    #[serde(default)]
    pub git_user_name: Option<String>,
    /// Git user email for commits (optional, falls back to git config).
    #[serde(default)]
    pub git_user_email: Option<String>,
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
            features: GeneralFeatureFlags {
                checkpoint_enabled: true,
                force_universal_prompt: false,
                isolation_mode: true,
            },
            developer_iters: 5,
            reviewer_reviews: 2,
            developer_context: 1,
            reviewer_context: 0,
            review_depth: "standard".to_string(),
            prompt_path: None,
            git_user_name: None,
            git_user_email: None,
        }
    }
}

/// CCS (Claude Code Switch) alias configuration.
///
/// Maps alias names to CCS profile commands.
/// For example: `work = "ccs work"` allows using `ccs/work` as an agent.
pub type CcsAliases = HashMap<String, CcsAliasToml>;

/// CCS defaults applied to all CCS aliases unless overridden per-alias.
#[derive(Debug, Clone, Deserialize)]
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
    /// Print flag for non-interactive mode (required by Claude CLI).
    /// Default: "-p"
    pub print_flag: String,
    /// Streaming flag for JSON output with -p (required for Claude/CCS to stream).
    /// Default: "--include-partial-messages"
    pub streaming_flag: String,
    /// Which JSON parser to use for CCS output.
    pub json_parser: String,
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
            print_flag: "-p".to_string(),
            streaming_flag: "--include-partial-messages".to_string(),
            json_parser: "claude".to_string(),
            can_commit: true,
        }
    }
}

/// Per-alias CCS configuration (table form).
#[derive(Debug, Clone, Deserialize, Default)]
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
}

/// CCS alias entry supports both shorthand string and table form.
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize, Default)]
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
#[derive(Debug, Clone, Deserialize, Default)]
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
    pub fn load_default() -> Option<Self> {
        unified_config_path().and_then(|path| {
            if path.exists() {
                Self::load_from_path(&path).ok()
            } else {
                None
            }
        })
    }

    /// Load unified configuration from a specific path.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, ConfigLoadError> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Ensure unified config file exists, creating it from template if needed.
    ///
    /// This creates `~/.config/ralph-workflow.toml` with the default template
    /// if it doesn't already exist.
    pub fn ensure_config_exists() -> io::Result<ConfigInitResult> {
        let Some(path) = unified_config_path() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Cannot determine config directory (no home directory)",
            ));
        };

        Self::ensure_config_exists_at(&path)
    }

    /// Ensure a config file exists at the specified path.
    pub fn ensure_config_exists_at(path: &std::path::Path) -> io::Result<ConfigInitResult> {
        if path.exists() {
            return Ok(ConfigInitResult::AlreadyExists);
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write the default template
        fs::write(path, DEFAULT_UNIFIED_CONFIG)?;

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
mod tests {
    use super::*;
    use crate::config::types::Verbosity;

    fn get_ccs_alias_cmd(config: &UnifiedConfig, alias: &str) -> Option<String> {
        config.ccs_aliases.get(alias).map(|v| v.as_config().cmd)
    }

    #[test]
    fn test_general_config_defaults() {
        let config = GeneralConfig::default();
        assert_eq!(config.verbosity, 2);
        assert!(config.behavior.interactive);
        assert!(config.features.isolation_mode);
        assert!(config.behavior.auto_detect_stack);
        assert!(config.features.checkpoint_enabled);
        assert_eq!(config.developer_iters, 5);
        assert_eq!(config.reviewer_reviews, 2);
    }

    #[test]
    fn test_unified_config_defaults() {
        let config = UnifiedConfig::default();
        assert!(config.agents.is_empty());
        assert!(config.ccs_aliases.is_empty());
        assert!(config.agent_chain.is_none());
    }

    #[test]
    fn test_parse_unified_config() {
        let toml_str = r#"
[general]
verbosity = 3
interactive = false
developer_iters = 10

[agents.claude]
cmd = "claude -p"
output_flag = "--output-format=stream-json"
can_commit = true
json_parser = "claude"

[ccs_aliases]
work = "ccs work"
personal = "ccs personal"
gemini = "ccs gemini"

[agent_chain]
developer = ["ccs/work", "claude"]
reviewer = ["claude"]
"#;
        let config: UnifiedConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.verbosity, 3);
        assert!(!config.general.behavior.interactive);
        assert_eq!(config.general.developer_iters, 10);
        assert!(config.agents.contains_key("claude"));
        assert_eq!(
            config.ccs_aliases.get("work").unwrap().as_config().cmd,
            "ccs work"
        );
        assert_eq!(
            config.ccs_aliases.get("personal").unwrap().as_config().cmd,
            "ccs personal"
        );
        assert!(config.ccs_aliases.contains_key("work"));
        assert!(!config.ccs_aliases.contains_key("nonexistent"));
        let chain = config.agent_chain.expect("agent_chain should parse");
        assert_eq!(
            chain.developer,
            vec!["ccs/work".to_string(), "claude".to_string()]
        );
        assert_eq!(chain.reviewer, vec!["claude".to_string()]);
    }

    #[test]
    fn test_ccs_alias_lookup() {
        let mut config = UnifiedConfig::default();
        config.ccs_aliases.insert(
            "work".to_string(),
            CcsAliasToml::Command("ccs work".to_string()),
        );
        config.ccs_aliases.insert(
            "gemini".to_string(),
            CcsAliasToml::Command("ccs gemini".to_string()),
        );

        assert_eq!(
            get_ccs_alias_cmd(&config, "work"),
            Some("ccs work".to_string())
        );
        assert_eq!(
            get_ccs_alias_cmd(&config, "gemini"),
            Some("ccs gemini".to_string())
        );
        assert_eq!(get_ccs_alias_cmd(&config, "nonexistent"), None);
    }

    #[test]
    fn test_verbosity_conversion() {
        let mut config = UnifiedConfig::default();
        config.general.verbosity = 0;
        assert_eq!(Verbosity::from(config.general.verbosity), Verbosity::Quiet);
        config.general.verbosity = 4;
        assert_eq!(Verbosity::from(config.general.verbosity), Verbosity::Debug);
    }

    #[test]
    fn test_unified_config_path() {
        // Just verify it returns something (path depends on system)
        let path = unified_config_path();
        if let Some(p) = path {
            assert!(p.to_string_lossy().contains("ralph-workflow.toml"));
        }
    }
}
