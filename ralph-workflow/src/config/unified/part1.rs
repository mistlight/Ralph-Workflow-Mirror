// Unified Configuration Types - Part 1: Imports and General Config
//
// Contains all imports, constants, and the GeneralConfig-related types.

use crate::agents::fallback::FallbackConfig;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
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

/// General configuration workflow automation flags.
///
/// Groups workflow automation features for `GeneralConfig`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GeneralWorkflowFlags {
    /// Enable checkpoint/resume functionality.
    pub checkpoint_enabled: bool,
}

/// General configuration execution behavior flags.
///
/// Groups execution behavior settings for `GeneralConfig`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GeneralExecutionFlags {
    /// Force universal review prompt for all agents.
    pub force_universal_prompt: bool,
    /// Isolation mode (prevent context contamination).
    pub isolation_mode: bool,
}

/// General configuration section.
#[derive(Debug, Clone, Deserialize)]
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
        }
    }
}
