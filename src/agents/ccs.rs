//! CCS (Claude Code Switch) Alias Resolution
//!
//! This module provides support for resolving CCS aliases to agent configurations.
//! CCS is a universal AI profile manager that supports multiple Claude accounts,
//! Gemini, Copilot, OpenRouter, and other providers.
//!
//! # Direct Claude Execution for CCS Aliases
//!
//! **IMPORTANT**: This module bypasses the `ccs` wrapper command and uses `claude` directly.
//!
//! ## Why?
//!
//! The `ccs` wrapper command does not pass through all Claude CLI flags properly,
//! especially streaming-related flags like `--include-partial-messages`. This causes
//! issues with Ralph's JSON streaming output.
//!
//! ## How?
//!
//! Instead of running `ccs glm -p --output-format=stream-json ...`, we run:
//! ```bash
//! ANTHROPIC_BASE_URL="..." \
//! ANTHROPIC_AUTH_TOKEN="..." \
//! ANTHROPIC_MODEL="..." \
//! claude -p --output-format=stream-json ...
//! ```
//!
//! The environment variables are loaded by scanning `~/.ccs/*.json` for a file
//! matching the alias name, then extracting an environment-variable map from the
//! JSON. This avoids hard-coding CCS' internal file naming or JSON schema.
//!
//! ## Fallback
//!
//! If the `claude` binary is not found in PATH (or env vars can't be loaded), the
//! original `ccs` command is used.
//!
//! # Usage
//!
//! Agents can be specified using `ccs/alias` syntax:
//! - `ccs/work` - Uses the "work" profile from CCS config
//! - `ccs/personal` - Uses the "personal" profile
//! - `ccs/gemini` - Uses CCS with Gemini provider
//! - `ccs` - Uses the default CCS profile
//!
//! # Configuration
//!
//! CCS aliases are defined in `~/.config/ralph-workflow.toml`:
//!
//! ```toml
//! [ccs]
//! # Defaults applied to all CCS aliases unless overridden per-alias.
//! # If your CCS version doesn't support these Claude CLI flags, set them to "".
//! output_flag = "--output-format=stream-json"
//! verbose_flag = "--verbose"
//! # YOLO (autonomous) mode: enabled by default (skip permission/confirmation prompts).
//! # Set to "" to disable and require confirmations.
//! yolo_flag = "--dangerously-skip-permissions"
//! json_parser = "claude"
//!
//! [ccs_aliases]
//! work = "ccs work" # shorthand
//! personal = { cmd = "ccs personal" } # explicit table form
//! gemini = { cmd = "ccs gemini", output_flag = "", verbose_flag = "", json_parser = "generic" }
//! ```

use super::config::{find_claude_binary, load_ccs_env_vars, AgentConfig};
use super::parser::JsonParserType;
use crate::config::{CcsAliasConfig, CcsConfig};
use crate::utils::split_command;
use std::collections::HashMap;
use std::path::Path;

/// CCS alias prefix for agent names.
pub const CCS_PREFIX: &str = "ccs/";

/// Parse a CCS agent reference and extract the alias name.
///
/// Returns `Some(alias)` if the agent name matches `ccs/alias` pattern,
/// or `Some("")` if it's just `ccs` (for default profile).
/// Returns `None` if the name doesn't match the CCS pattern.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(parse_ccs_ref("ccs/work"), Some("work"));
/// assert_eq!(parse_ccs_ref("ccs"), Some(""));
/// assert_eq!(parse_ccs_ref("claude"), None);
/// ```
pub fn parse_ccs_ref(agent_name: &str) -> Option<&str> {
    if agent_name == "ccs" {
        Some("")
    } else if let Some(alias) = agent_name.strip_prefix(CCS_PREFIX) {
        Some(alias)
    } else {
        None
    }
}

/// Check if an agent name is a CCS reference.
pub fn is_ccs_ref(agent_name: &str) -> bool {
    parse_ccs_ref(agent_name).is_some()
}

fn looks_like_ccs_executable(cmd0: &str) -> bool {
    Path::new(cmd0)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "ccs" || n == "ccs.exe")
}

/// Resolve a CCS alias to an AgentConfig.
///
/// Given a CCS alias and a map of aliases to commands, this function
/// generates an `AgentConfig` that can be used to run CCS.
///
/// # Arguments
///
/// * `alias` - The alias name (e.g., "work", "gemini", or "" for default)
/// * `aliases` - Map of alias names to CCS commands
///
/// # Returns
///
/// Returns `Some(AgentConfig)` if the alias is found or if using default,
/// `None` if the alias is not found in the map.
pub fn resolve_ccs_agent(
    alias: &str,
    aliases: &HashMap<String, CcsAliasConfig>,
    defaults: &CcsConfig,
) -> Option<AgentConfig> {
    // Empty alias means use default CCS
    let (cmd, display_name) = if alias.is_empty() {
        (
            CcsAliasConfig {
                cmd: "ccs".to_string(),
                ..CcsAliasConfig::default()
            },
            "ccs".to_string(),
        )
    } else if let Some(cfg) = aliases.get(alias) {
        (cfg.clone(), format!("ccs-{}", alias))
    } else {
        // Unknown alias - return None so caller can fall back
        return None;
    };

    Some(build_ccs_agent_config(&cmd, defaults, display_name, alias))
}

/// Build an AgentConfig for a CCS command.
///
/// CCS wraps Claude Code, so it uses Claude's stream-json format
/// and similar flags.
///
/// # JSON Parser Selection
///
/// CCS (Claude Code Switcher) defaults to the Claude parser (`json_parser = "claude"`)
/// because CCS wraps the `claude` CLI tool and uses Claude's stream-json output format.
///
/// **Why Claude parser by default?** CCS uses Claude Code's CLI interface and output format.
/// The `--output-format=stream-json` flag produces Claude's NDJSON format, which the
/// Claude parser is designed to handle.
///
/// **Parser override:** Users can override the parser via `json_parser` in their config.
/// The alias-specific `json_parser` takes precedence over the CCS default. This allows
/// advanced users to use alternative parsers if needed for specific providers.
///
/// Example: `ccs glm` → uses Claude parser by default (from defaults.json_parser)
///          `ccs gemini` → uses Claude parser by default
///          With override: `json_parser = "generic"` in alias config overrides default
///
/// Display name format: CCS aliases are shown as "ccs-{alias}" (e.g., "ccs-glm", "ccs-gemini")
/// in output/logs to make it clearer which provider is actually being used, while still using
/// the Claude parser under the hood.
///
/// # Environment Variable Loading
///
/// This function automatically loads environment variables for the given profile by scanning
/// `~/.ccs/*.json` for a file matching the alias name. This allows CCS aliases to use their
/// configured credentials without requiring manual environment variable configuration, while
/// avoiding hard-coded assumptions about CCS' exact file naming or JSON schema.
fn build_ccs_agent_config(
    alias_config: &CcsAliasConfig,
    defaults: &CcsConfig,
    display_name: String,
    alias_name: &str,
) -> AgentConfig {
    let env_vars_result = if alias_name.is_empty() {
        Ok(HashMap::new())
    } else {
        load_ccs_env_vars(alias_name).map_err(|err| {
            let suggestions = super::config::find_ccs_profile_suggestions(alias_name);
            eprintln!("Warning: failed to load CCS env vars for profile '{alias_name}': {err}");
            if !suggestions.is_empty() {
                eprintln!("Tip: available/nearby CCS profiles:");
                for s in suggestions {
                    eprintln!("  - {s}");
                }
            }
            err
        })
    };
    let (env_vars, env_vars_loaded) = match env_vars_result {
        Ok(vars) => (vars, true),
        Err(_) => (HashMap::new(), false),
    };

    // Determine the command to use.
    // For CCS aliases, we try to use `claude` directly instead of the `ccs` wrapper
    // because the wrapper does not pass through all flags properly (especially
    // streaming-related flags like --include-partial-messages).
    //
    // We only bypass the wrapper when:
    // - The agent name is `ccs/<alias>` (not plain `ccs`)
    // - We successfully loaded env vars for that profile (even if empty)
    // - The configured command is exactly `ccs <alias>` (plus optional extra args)
    let cmd = if let Some(claude_path) = find_claude_binary() {
        let original_cmd = alias_config.cmd.as_str();
        let can_bypass_wrapper = !alias_name.is_empty() && env_vars_loaded;
        if can_bypass_wrapper {
            if let Ok(parts) = split_command(original_cmd) {
                let is_profile_ccs_cmd = parts
                    .first()
                    .is_some_and(|p| looks_like_ccs_executable(p))
                    && parts.get(1).is_some_and(|p| p == alias_name);

                if is_profile_ccs_cmd {
                    let mut new_parts = Vec::with_capacity(parts.len().saturating_sub(1));
                    new_parts.push(claude_path.to_string_lossy().to_string());
                    new_parts.extend(parts.into_iter().skip(2));
                    eprintln!(
                        "Info: bypassing `ccs` wrapper for `ccs/{alias_name}` to preserve Claude CLI flag passthrough"
                    );
                    shell_words::join(&new_parts)
                } else {
                    original_cmd.to_string()
                }
            } else {
                original_cmd.to_string()
            }
        } else {
            original_cmd.to_string()
        }
    } else {
        // Could not find claude binary, use original command
        // This may result in suboptimal flag passthrough, but is better than breaking
        let original_cmd = alias_config.cmd.as_str();
        if original_cmd.starts_with("ccs ") || original_cmd == "ccs" {
            eprintln!("Warning: `claude` binary not found in PATH, using `ccs` wrapper");
            eprintln!(
                "  This may cause issues with streaming flags like --include-partial-messages"
            );
            eprintln!("  Consider installing the Claude CLI: https://claude.ai/download");
        }
        original_cmd.to_string()
    };

    let output_flag = alias_config
        .output_flag
        .clone()
        .unwrap_or_else(|| defaults.output_flag.clone());
    let yolo_flag = alias_config
        .yolo_flag
        .clone()
        .unwrap_or_else(|| defaults.yolo_flag.clone());
    let verbose_flag = alias_config
        .verbose_flag
        .clone()
        .unwrap_or_else(|| defaults.verbose_flag.clone());
    // CRITICAL: CCS always requires -p flag for non-interactive mode.
    // If defaults.print_flag is empty (missing config), fall back to "-p".
    let print_flag = alias_config.print_flag.clone().unwrap_or_else(|| {
        let pf = defaults.print_flag.clone();
        if pf.is_empty() {
            // Hardcoded safety fallback: CCS commands need -p for non-interactive mode
            "-p".to_string()
        } else {
            pf
        }
    });

    // Parser selection: alias-specific override takes precedence over CCS default.
    // This allows users to customize parser per CCS alias if needed.
    // See function docstring above for detailed explanation.
    let json_parser = alias_config
        .json_parser
        .as_deref()
        .unwrap_or(&defaults.json_parser);
    let can_commit = alias_config.can_commit.unwrap_or(defaults.can_commit);

    // Get streaming flag from alias override or defaults
    let streaming_flag = alias_config
        .streaming_flag
        .clone()
        .unwrap_or_else(|| defaults.streaming_flag.clone());

    AgentConfig {
        cmd, // Uses `claude` directly if found, otherwise falls back to original command
        output_flag,
        yolo_flag,
        verbose_flag,
        can_commit,
        json_parser: JsonParserType::parse(json_parser),
        model_flag: alias_config.model_flag.clone(),
        print_flag, // CCS requires -p for non-interactive mode (from defaults or alias override)
        streaming_flag, // Required for JSON streaming when using -p
        env_vars,   // Loaded from ~/.ccs/*.json for the given alias, if available
        display_name: Some(display_name),
    }
}

/// CCS alias resolver that can be used by the agent registry.
#[derive(Debug, Clone, Default)]
pub struct CcsAliasResolver {
    aliases: HashMap<String, CcsAliasConfig>,
    defaults: CcsConfig,
}

impl CcsAliasResolver {
    /// Create a new CCS alias resolver with the given aliases.
    pub fn new(aliases: HashMap<String, CcsAliasConfig>, defaults: CcsConfig) -> Self {
        Self { aliases, defaults }
    }

    /// Create an empty resolver (no aliases configured).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Check if the resolver has any aliases configured.
    #[cfg(test)]
    pub fn has_aliases(&self) -> bool {
        !self.aliases.is_empty()
    }

    /// Get the number of configured aliases.
    #[cfg(test)]
    pub fn alias_count(&self) -> usize {
        self.aliases.len()
    }

    /// List all configured alias names.
    #[cfg(test)]
    pub fn list_aliases(&self) -> Vec<&str> {
        self.aliases.keys().map(|s| s.as_str()).collect()
    }

    /// Try to resolve an agent name as a CCS reference.
    ///
    /// Returns `Some(AgentConfig)` if the name is a valid CCS reference.
    /// For known aliases (or default `ccs`), uses the configured command.
    /// For unknown aliases (e.g., `ccs/random`), generates a default CCS config
    /// to allow direct CCS execution without configuration.
    /// Returns `None` if the name is not a CCS reference (doesn't start with "ccs").
    pub fn try_resolve(&self, agent_name: &str) -> Option<AgentConfig> {
        let alias = parse_ccs_ref(agent_name)?;
        // Try to resolve from configured aliases
        if let Some(config) = resolve_ccs_agent(alias, &self.aliases, &self.defaults) {
            return Some(config);
        }
        // For unknown CCS aliases, generate a default config for direct execution
        // This allows commands like `ccs random` to work without pre-configuration
        let cmd = CcsAliasConfig {
            cmd: format!("ccs {}", alias),
            ..CcsAliasConfig::default()
        };
        let display_name = format!("ccs-{}", alias);
        Some(build_ccs_agent_config(
            &cmd,
            &self.defaults,
            display_name,
            alias,
        ))
    }

    /// Check if a given agent name would resolve to a CCS alias.
    /// Now returns true for any CCS reference since we support direct execution.
    #[cfg(test)]
    pub fn can_resolve(&self, agent_name: &str) -> bool {
        parse_ccs_ref(agent_name).is_some()
    }

    /// Add a new alias.
    #[cfg(test)]
    pub fn add_alias(&mut self, name: String, cmd: String) {
        self.aliases.insert(
            name,
            CcsAliasConfig {
                cmd,
                ..CcsAliasConfig::default()
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ccs() -> CcsConfig {
        CcsConfig::default()
    }

    #[test]
    fn test_parse_ccs_ref() {
        // Valid CCS references
        assert_eq!(parse_ccs_ref("ccs"), Some(""));
        assert_eq!(parse_ccs_ref("ccs/work"), Some("work"));
        assert_eq!(parse_ccs_ref("ccs/personal"), Some("personal"));
        assert_eq!(parse_ccs_ref("ccs/gemini"), Some("gemini"));
        assert_eq!(
            parse_ccs_ref("ccs/my-custom-alias"),
            Some("my-custom-alias")
        );

        // Not CCS references
        assert_eq!(parse_ccs_ref("claude"), None);
        assert_eq!(parse_ccs_ref("codex"), None);
        assert_eq!(parse_ccs_ref("ccs_work"), None);
        assert_eq!(parse_ccs_ref("cccs/work"), None);
        assert_eq!(parse_ccs_ref(""), None);
    }

    #[test]
    fn test_is_ccs_ref() {
        assert!(is_ccs_ref("ccs"));
        assert!(is_ccs_ref("ccs/work"));
        assert!(is_ccs_ref("ccs/gemini"));
        assert!(!is_ccs_ref("claude"));
        assert!(!is_ccs_ref("codex"));
    }

    #[test]
    fn test_resolve_ccs_agent_default() {
        let aliases = HashMap::new();
        let config = resolve_ccs_agent("", &aliases, &default_ccs());
        assert!(config.is_some());
        let config = config.unwrap();
        // When claude binary is found, it replaces "ccs" with the path to claude
        // The command should end with "claude"
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs",
            "cmd should be 'ccs' or a path ending with 'claude', got: {}",
            config.cmd
        );
        assert!(config.can_commit);
        assert_eq!(config.json_parser, JsonParserType::Claude);
    }

    #[test]
    fn test_resolve_ccs_agent_with_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "gemini".to_string(),
            CcsAliasConfig {
                cmd: "ccs gemini".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        let config = resolve_ccs_agent("work", &aliases, &default_ccs());
        assert!(config.is_some());
        let config = config.unwrap();
        // When claude binary is found, it replaces "ccs work" with the path to claude
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs work",
            "cmd should be 'ccs work' or a path ending with 'claude', got: {}",
            config.cmd
        );

        let config = resolve_ccs_agent("gemini", &aliases, &default_ccs());
        assert!(config.is_some());
        let config = config.unwrap();
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs gemini",
            "cmd should be 'ccs gemini' or a path ending with 'claude', got: {}",
            config.cmd
        );

        // Unknown alias returns None
        let config = resolve_ccs_agent("unknown", &aliases, &default_ccs());
        assert!(config.is_none());
    }

    #[test]
    fn test_build_ccs_agent_config() {
        let config = build_ccs_agent_config(
            &CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
            &default_ccs(),
            "ccs-work".to_string(),
            "work",
        );
        // When claude binary is found, it replaces "ccs work" with the path to claude
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs work",
            "cmd should be 'ccs work' or a path ending with 'claude', got: {}",
            config.cmd
        );
        assert_eq!(config.output_flag, "--output-format=stream-json");
        assert_eq!(config.yolo_flag, "--dangerously-skip-permissions");
        assert_eq!(config.verbose_flag, "--verbose");
        assert!(config.can_commit);
        assert_eq!(config.json_parser, JsonParserType::Claude);
        assert!(config.model_flag.is_none());
        assert_eq!(config.display_name, Some("ccs-work".to_string()));
    }

    #[test]
    fn test_ccs_alias_resolver_new() {
        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        let resolver = CcsAliasResolver::new(aliases, default_ccs());
        assert!(resolver.has_aliases());
        assert_eq!(resolver.alias_count(), 1);
    }

    #[test]
    fn test_ccs_alias_resolver_empty() {
        let resolver = CcsAliasResolver::empty();
        assert!(!resolver.has_aliases());
        assert_eq!(resolver.alias_count(), 0);
    }

    #[test]
    fn test_ccs_alias_resolver_try_resolve() {
        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "personal".to_string(),
            CcsAliasConfig {
                cmd: "ccs personal".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        let resolver = CcsAliasResolver::new(aliases, default_ccs());

        // Resolve ccs/work
        let config = resolver.try_resolve("ccs/work");
        assert!(config.is_some());
        let work_cmd = config.unwrap().cmd.clone();
        assert!(
            work_cmd.ends_with("claude") || work_cmd == "ccs work",
            "cmd should be 'ccs work' or a path ending with 'claude', got: {}",
            work_cmd
        );

        // Resolve ccs/personal
        let config = resolver.try_resolve("ccs/personal");
        assert!(config.is_some());
        let personal_cmd = config.unwrap().cmd.clone();
        assert!(
            personal_cmd.ends_with("claude") || personal_cmd == "ccs personal",
            "cmd should be 'ccs personal' or a path ending with 'claude', got: {}",
            personal_cmd
        );

        // Resolve plain "ccs" (default)
        let config = resolver.try_resolve("ccs");
        assert!(config.is_some());
        let default_cmd = config.unwrap().cmd.clone();
        assert!(
            default_cmd.ends_with("claude") || default_cmd == "ccs",
            "cmd should be 'ccs' or a path ending with 'claude', got: {}",
            default_cmd
        );

        // Unknown alias - now resolves with default config for direct CCS execution
        let config = resolver.try_resolve("ccs/unknown");
        assert!(config.is_some());
        let unknown_cmd = config.unwrap().cmd.clone();
        assert!(
            unknown_cmd.ends_with("claude") || unknown_cmd == "ccs unknown",
            "cmd should be 'ccs unknown' or a path ending with 'claude', got: {}",
            unknown_cmd
        );

        // Not a CCS ref
        let config = resolver.try_resolve("claude");
        assert!(config.is_none());
    }

    #[test]
    fn test_ccs_alias_resolver_can_resolve() {
        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        let resolver = CcsAliasResolver::new(aliases, default_ccs());

        // All CCS references now resolve (including unregistered ones)
        assert!(resolver.can_resolve("ccs"));
        assert!(resolver.can_resolve("ccs/work"));
        assert!(resolver.can_resolve("ccs/unknown")); // Now true - direct CCS execution
        assert!(!resolver.can_resolve("claude"));
    }

    #[test]
    fn test_ccs_alias_resolver_list_aliases() {
        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "personal".to_string(),
            CcsAliasConfig {
                cmd: "ccs personal".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        let resolver = CcsAliasResolver::new(aliases, default_ccs());

        let list = resolver.list_aliases();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"work"));
        assert!(list.contains(&"personal"));
    }

    #[test]
    fn test_ccs_alias_resolver_add_alias() {
        let mut resolver = CcsAliasResolver::empty();
        assert!(!resolver.has_aliases());

        resolver.add_alias("work".to_string(), "ccs work".to_string());
        assert!(resolver.has_aliases());
        assert!(resolver.can_resolve("ccs/work"));
    }

    // Additional tests for various CCS command patterns per Step 2 of plan

    #[test]
    fn test_ccs_command_variants() {
        // Tests for different CCS command patterns as used in the wild:
        // - ccs (default profile)
        // - ccs <profile> (named profile)
        // - ccs gemini / ccs codex / ccs glm (built-in providers)
        // - ccs api <custom> (custom API profiles)

        let mut aliases = HashMap::new();
        // Named profiles
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "personal".to_string(),
            CcsAliasConfig {
                cmd: "ccs personal".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        // Built-in provider profiles
        aliases.insert(
            "gemini".to_string(),
            CcsAliasConfig {
                cmd: "ccs gemini".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "codex".to_string(),
            CcsAliasConfig {
                cmd: "ccs codex".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "glm".to_string(),
            CcsAliasConfig {
                cmd: "ccs glm".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        // Custom API profiles
        aliases.insert(
            "openrouter".to_string(),
            CcsAliasConfig {
                cmd: "ccs api openrouter".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "custom-api".to_string(),
            CcsAliasConfig {
                cmd: "ccs api custom-profile".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        let resolver = CcsAliasResolver::new(aliases, default_ccs());

        // Test named profiles - when claude binary is found, it replaces "ccs ..." with claude path
        let config = resolver.try_resolve("ccs/work").unwrap();
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs work",
            "cmd should be 'ccs work' or a path ending with 'claude', got: {}",
            config.cmd
        );

        // Test built-in providers
        let config = resolver.try_resolve("ccs/gemini").unwrap();
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs gemini",
            "cmd should be 'ccs gemini' or a path ending with 'claude', got: {}",
            config.cmd
        );

        let config = resolver.try_resolve("ccs/codex").unwrap();
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs codex",
            "cmd should be 'ccs codex' or a path ending with 'claude', got: {}",
            config.cmd
        );

        let config = resolver.try_resolve("ccs/glm").unwrap();
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs glm",
            "cmd should be 'ccs glm' or a path ending with 'claude', got: {}",
            config.cmd
        );

        // Test custom API profiles
        let config = resolver.try_resolve("ccs/openrouter").unwrap();
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs api openrouter",
            "cmd should be 'ccs api openrouter' or a path ending with 'claude', got: {}",
            config.cmd
        );

        let config = resolver.try_resolve("ccs/custom-api").unwrap();
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs api custom-profile",
            "cmd should be 'ccs api custom-profile' or a path ending with 'claude', got: {}",
            config.cmd
        );
    }

    #[test]
    fn test_ccs_config_has_correct_flags() {
        // Verify that CCS agent configs default to Claude-compatible flags
        // (users can override these via the unified config).
        let config = build_ccs_agent_config(
            &CcsAliasConfig {
                cmd: "ccs gemini".to_string(),
                ..CcsAliasConfig::default()
            },
            &default_ccs(),
            "ccs-gemini".to_string(),
            "gemini",
        );

        // CCS wraps Claude Code, so it uses Claude's stream-json format
        assert_eq!(config.output_flag, "--output-format=stream-json");
        assert_eq!(config.yolo_flag, "--dangerously-skip-permissions");
        assert_eq!(config.verbose_flag, "--verbose");
        // CCS requires -p for non-interactive mode
        assert_eq!(config.print_flag, "-p");
        assert!(config.can_commit);

        // CCS always outputs stream-json format, so always use Claude parser
        assert_eq!(config.json_parser, JsonParserType::Claude);
        assert_eq!(config.display_name, Some("ccs-gemini".to_string()));
    }

    #[test]
    fn test_parse_ccs_ref_edge_cases() {
        // Test edge cases in CCS reference parsing
        assert_eq!(parse_ccs_ref("ccs/"), Some("")); // Empty after prefix
        assert_eq!(parse_ccs_ref("ccs/a"), Some("a")); // Single char
        assert_eq!(
            parse_ccs_ref("ccs/with-dashes-and_underscores"),
            Some("with-dashes-and_underscores")
        );
        assert_eq!(parse_ccs_ref("ccs/with.dots"), Some("with.dots"));
        assert_eq!(parse_ccs_ref("ccs/MixedCase"), Some("MixedCase"));
        assert_eq!(parse_ccs_ref("ccs/123numeric"), Some("123numeric"));

        // These should NOT be CCS refs
        assert_eq!(parse_ccs_ref("CCS"), None); // Case sensitive
        assert_eq!(parse_ccs_ref("CCS/work"), None);
        assert_eq!(parse_ccs_ref(" ccs"), None); // Leading space
        assert_eq!(parse_ccs_ref("ccs "), None); // Trailing space (invalid ref, not just "ccs")
    }

    #[test]
    fn test_ccs_in_agent_chain_context() {
        // Simulate how CCS aliases would be used in agent chain context
        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "personal".to_string(),
            CcsAliasConfig {
                cmd: "ccs personal".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        let resolver = CcsAliasResolver::new(aliases, default_ccs());

        // Simulate agent chain: ["ccs/work", "claude", "codex"]
        let chain = ["ccs/work", "claude", "codex"];

        // First in chain should resolve
        assert!(resolver.can_resolve(chain[0]));
        assert!(!resolver.can_resolve(chain[1])); // claude is not a CCS ref
        assert!(!resolver.can_resolve(chain[2])); // codex is not a CCS ref

        // The resolved config should be usable
        let config = resolver.try_resolve(chain[0]).unwrap();
        assert!(config.can_commit);
        assert!(!config.cmd.is_empty());
    }

    #[test]
    fn test_ccs_display_names() {
        // Test that CCS aliases get proper display names like "ccs-glm", "ccs-gemini"
        let mut aliases = HashMap::new();
        aliases.insert(
            "glm".to_string(),
            CcsAliasConfig {
                cmd: "ccs glm".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "gemini".to_string(),
            CcsAliasConfig {
                cmd: "ccs gemini".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        let resolver = CcsAliasResolver::new(aliases, default_ccs());

        // Test display names for various aliases
        let glm_config = resolver.try_resolve("ccs/glm").unwrap();
        assert_eq!(glm_config.display_name, Some("ccs-glm".to_string()));

        let gemini_config = resolver.try_resolve("ccs/gemini").unwrap();
        assert_eq!(gemini_config.display_name, Some("ccs-gemini".to_string()));

        let work_config = resolver.try_resolve("ccs/work").unwrap();
        assert_eq!(work_config.display_name, Some("ccs-work".to_string()));

        // Default CCS (no alias) should just be "ccs"
        let default_config = resolver.try_resolve("ccs").unwrap();
        assert_eq!(default_config.display_name, Some("ccs".to_string()));
    }

    // Step 7: Test coverage for GLM command building

    #[test]
    fn test_ccs_glm_command_has_print_flag() {
        // Verify that GLM commands include the -p flag for non-interactive mode
        let mut aliases = HashMap::new();
        aliases.insert(
            "glm".to_string(),
            CcsAliasConfig {
                cmd: "ccs glm".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        let resolver = CcsAliasResolver::new(aliases, default_ccs());
        let glm_config = resolver.try_resolve("ccs/glm").unwrap();

        // Verify print_flag is set to "-p" (from defaults)
        assert_eq!(glm_config.print_flag, "-p");

        // Build the command and verify -p is included
        let cmd = glm_config.build_cmd(true, true, true);
        assert!(cmd.contains(" -p"), "GLM command must include -p flag");
        // When claude binary is found, command should contain "claude" as the base command
        // The actual command is now "claude -p ..." instead of "ccs glm -p ..."
        // We check if the first word (before any space) ends with "claude"
        let first_word = cmd.split_whitespace().next().unwrap_or("");
        assert!(
            first_word.ends_with("claude") || cmd.contains("ccs glm"),
            "Command should start with a path ending in 'claude' or contain 'ccs glm', got: {}",
            cmd
        );
    }

    #[test]
    fn test_ccs_glm_flag_ordering() {
        // Verify that flags are in the correct order for CCS GLM
        // The -p flag must come AFTER the command name (e.g., "claude -p" not "-p claude")
        let mut aliases = HashMap::new();
        aliases.insert(
            "glm".to_string(),
            CcsAliasConfig {
                cmd: "ccs glm".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        let resolver = CcsAliasResolver::new(aliases, default_ccs());
        let glm_config = resolver.try_resolve("ccs/glm").unwrap();

        let cmd = glm_config.build_cmd(true, true, true);

        // Split command into parts and verify ordering
        let parts: Vec<&str> = cmd.split_whitespace().collect();

        // First part should be the claude command (path ending in "claude")
        // When using ccs directly, it would be "ccs" then "glm"
        // When using claude directly, it's just the path to claude
        let first_part = parts[0];
        assert!(
            first_part.ends_with("claude") || first_part == "ccs",
            "First part should end with 'claude' or be 'ccs', got: {}",
            first_part
        );

        // -p flag should come after the command name
        let p_index = parts.iter().position(|&s| s == "-p");
        assert!(p_index.is_some(), "-p flag must be present");
        assert!(p_index.unwrap() > 0, "-p flag must come after command name");
    }

    #[test]
    fn test_ccs_glm_with_empty_print_override() {
        // Test that if user explicitly sets print_flag to empty, it stays empty
        let mut aliases = HashMap::new();
        aliases.insert(
            "glm".to_string(),
            CcsAliasConfig {
                cmd: "ccs glm".to_string(),
                print_flag: Some("".to_string()), // Explicit empty override
                ..CcsAliasConfig::default()
            },
        );

        let resolver = CcsAliasResolver::new(aliases, default_ccs());
        let glm_config = resolver.try_resolve("ccs/glm").unwrap();

        // User override should take precedence
        assert_eq!(glm_config.print_flag, "");

        // Command should NOT include -p (user explicitly disabled it)
        let cmd = glm_config.build_cmd(true, true, true);
        assert!(
            !cmd.contains(" -p"),
            "Command should not include -p when explicitly disabled"
        );
    }

    #[test]
    fn test_glm_error_classification() {
        // Verify that GLM exit code 1 is classified as AgentSpecificQuirk
        use crate::agents::error::AgentErrorKind;

        let error = AgentErrorKind::classify_with_agent(1, "", Some("ccs/glm"), None);
        assert_eq!(error, AgentErrorKind::AgentSpecificQuirk);

        let error = AgentErrorKind::classify_with_agent(1, "some error", Some("glm"), None);
        assert_eq!(error, AgentErrorKind::AgentSpecificQuirk);

        let error = AgentErrorKind::classify_with_agent(1, "glm failed", Some("ccs"), Some("glm"));
        assert_eq!(error, AgentErrorKind::AgentSpecificQuirk);
    }
}
