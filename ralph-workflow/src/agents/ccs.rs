//! CCS (Claude Code Switch) Alias Resolution
//!
//! This module provides support for resolving CCS aliases to agent configurations.
//! CCS is a universal AI profile manager that supports multiple Claude accounts,
//! Gemini, `Copilot`, `OpenRouter`, and other providers.
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
//! ANTHROPIC_BASE_URL="..." \\
//! ANTHROPIC_AUTH_TOKEN="..." \\
//! ANTHROPIC_MODEL="..." \\
//! claude -p --output-format=stream-json ...
//! ```
//!
//! The environment variables are loaded from CCS' settings files using the same
//! resolution rules CCS uses (via `~/.ccs/config.json` / `~/.ccs/config.yaml` and
//! common settings filenames like `~/.ccs/{profile}.settings.json`). This avoids
//! running the `ccs` wrapper while still using CCS-managed credentials.
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

use super::config::{
    find_ccs_profile_suggestions, find_claude_binary, load_ccs_env_vars, AgentConfig,
    CcsEnvVarsError,
};
use super::parser::JsonParserType;
use crate::config::{CcsAliasConfig, CcsConfig};
use crate::utils::split_command;
use std::collections::HashMap;
use std::path::Path;

/// CCS alias prefix for agent names.
const CCS_PREFIX: &str = "ccs/";

/// Parse a CCS agent reference and extract the alias name.
///
/// Returns `Some(alias)` if the agent name matches `ccs/alias` pattern,
/// or `Some("")` if it's just `ccs` (for default profile).
/// Returns `None` if the name doesn't match the CCS pattern.
fn parse_ccs_ref(agent_name: &str) -> Option<&str> {
    if agent_name == "ccs" {
        Some("")
    } else if let Some(alias) = agent_name.strip_prefix(CCS_PREFIX) {
        Some(alias)
    } else {
        None
    }
}

/// Check if an agent name is a CCS reference.
///
/// CCS references include:
/// - `ccs` - Default CCS profile
/// - `ccs/<alias>` - Named CCS alias (e.g., `ccs/work`, `ccs/gemini`)
pub fn is_ccs_ref(agent_name: &str) -> bool {
    parse_ccs_ref(agent_name).is_some()
}

fn looks_like_ccs_executable(cmd0: &str) -> bool {
    Path::new(cmd0)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "ccs" || n == "ccs.exe")
}

fn ccs_profile_from_command(original_cmd: &str) -> Option<String> {
    let parts = split_command(original_cmd).ok()?;
    if !parts.first().is_some_and(|p| looks_like_ccs_executable(p)) {
        return None;
    }
    // Common patterns:
    // - `ccs <profile>`
    // - `ccs api <profile>`
    if parts.get(1).is_some_and(|p| p == "api") {
        parts.get(2).cloned()
    } else {
        parts.get(1).cloned()
    }
}

fn choose_best_profile_guess<'a>(input: &str, suggestions: &'a [String]) -> Option<&'a str> {
    if suggestions.is_empty() {
        return None;
    }
    let input_lower = input.to_lowercase();
    if let Some(exact) = suggestions
        .iter()
        .find(|s| s.to_lowercase() == input_lower)
        .map(String::as_str)
    {
        return Some(exact);
    }
    if suggestions.len() == 1 {
        return Some(suggestions[0].as_str());
    }
    if let Some(starts) = suggestions
        .iter()
        .find(|s| s.to_lowercase().starts_with(&input_lower))
        .map(String::as_str)
    {
        return Some(starts);
    }
    Some(suggestions[0].as_str())
}

fn load_ccs_env_vars_with_guess(
    profile: &str,
) -> Result<(HashMap<String, String>, Option<String>), CcsEnvVarsError> {
    match load_ccs_env_vars(profile) {
        Ok(vars) => Ok((vars, None)),
        Err(err @ CcsEnvVarsError::ProfileNotFound { .. }) => {
            let suggestions = find_ccs_profile_suggestions(profile);
            let Some(best) = choose_best_profile_guess(profile, &suggestions) else {
                return Err(err);
            };
            let vars = load_ccs_env_vars(best)?;
            Ok((vars, Some(best.to_string())))
        }
        Err(err) => Err(err),
    }
}

/// Resolve a CCS alias to an `AgentConfig`.
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
fn resolve_ccs_agent(
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
        (cfg.clone(), format!("ccs-{alias}"))
    } else {
        // Unknown alias - return None so caller can fall back
        return None;
    };

    Some(build_ccs_agent_config(&cmd, defaults, display_name, alias))
}

/// Load environment variables for a CCS alias.
fn load_ccs_env_vars_for_alias(
    alias_config: &CcsAliasConfig,
    alias_name: &str,
    debug_mode: bool,
) -> (HashMap<String, String>, bool, Option<String>) {
    if alias_name.is_empty() {
        return (HashMap::new(), false, None);
    }

    let original_cmd = alias_config.cmd.as_str();
    let profile =
        ccs_profile_from_command(original_cmd).unwrap_or_else(|| alias_name.to_string());
    let profile_clone = profile.clone();

    let result = match load_ccs_env_vars_with_guess(&profile) {
        Ok((vars, guessed)) => {
            if let Some(guessed) = guessed {
                eprintln!("Info: CCS profile '{profile}' not found; using '{guessed}'");
            }
            let loaded = !vars.is_empty();
            if debug_mode && loaded {
                eprintln!(
                    "CCS DEBUG: Loaded {} environment variable(s) for profile '{profile}'",
                    vars.len()
                );
                for key in vars.keys() {
                    eprintln!("CCS DEBUG:   - {key}");
                }
            } else if debug_mode {
                eprintln!(
                    "CCS DEBUG: Failed to load environment variables for profile '{profile}'"
                );
            }
            (vars, loaded, Some(profile))
        }
        Err(err) => {
            let suggestions = find_ccs_profile_suggestions(&profile);
            eprintln!("Warning: failed to load CCS env vars for profile '{profile}': {err}");
            if !suggestions.is_empty() {
                eprintln!("Tip: available/nearby CCS profiles:");
                for s in suggestions {
                    eprintln!("  - {s}");
                }
            }
            (HashMap::new(), false, Some(profile_clone))
        }
    };

    result
}

/// Determine whether to bypass the CCS wrapper and use claude directly.
fn determine_ccs_command(
    alias_config: &CcsAliasConfig,
    alias_name: &str,
    env_vars_loaded: bool,
    profile_used_for_env: Option<&str>,
    debug_mode: bool,
) -> String {
    let original_cmd = alias_config.cmd.as_str();

    let Some(claude_path) = find_claude_binary() else {
        if original_cmd.starts_with("ccs ") || original_cmd == "ccs" {
            if debug_mode {
                eprintln!("CCS DEBUG: Claude binary not found in PATH");
            }
            eprintln!("Warning: `claude` binary not found in PATH, using `ccs` wrapper");
            eprintln!(
                "  This may cause issues with streaming flags like --include-partial-messages"
            );
            eprintln!("  Consider installing the Claude CLI: https://claude.ai/download");
        }
        return original_cmd.to_string();
    };

    let can_bypass_wrapper = !alias_name.is_empty() && env_vars_loaded;

    if debug_mode {
        eprintln!(
            "CCS DEBUG: Claude binary found at: {}",
            claude_path.display()
        );
        eprintln!("CCS DEBUG: Original command: {original_cmd}");
        eprintln!("CCS DEBUG: Alias name: '{alias_name}'");
        eprintln!("CCS DEBUG: Env vars loaded: {env_vars_loaded}");
        eprintln!("CCS DEBUG: Can bypass wrapper: {can_bypass_wrapper}");
    }

    if !can_bypass_wrapper {
        if debug_mode {
            eprintln!("CCS DEBUG: Not bypassing (conditions not met)");
        }
        return original_cmd.to_string();
    }

    let Ok(parts) = split_command(original_cmd) else {
        if debug_mode {
            eprintln!("CCS DEBUG: Failed to parse command, using original");
        }
        return original_cmd.to_string();
    };

    let profile = ccs_profile_from_command(original_cmd)
        .or_else(|| profile_used_for_env.map(str::to_string))
        .unwrap_or_else(|| alias_name.to_string());
    let is_ccs_cmd = parts.first().is_some_and(|p| looks_like_ccs_executable(p));
    let skip = if parts.get(1).is_some_and(|p| p == &profile) {
        Some(2)
    } else if parts.get(1).is_some_and(|p| p == "api")
        && parts.get(2).is_some_and(|p| p == &profile)
    {
        Some(3)
    } else {
        None
    };
    let is_profile_ccs_cmd = is_ccs_cmd && skip.is_some();

    if debug_mode {
        eprintln!("CCS DEBUG: Command parts: {parts:?}");
        eprintln!("CCS DEBUG: Is profile CCS command: {is_profile_ccs_cmd}");
    }

    if !is_profile_ccs_cmd {
        if debug_mode {
            eprintln!("CCS DEBUG: Not bypassing (command doesn't match pattern)");
        }
        return original_cmd.to_string();
    }

    let skip = skip.unwrap_or(2);
    let mut new_parts = Vec::with_capacity(parts.len().saturating_sub(skip - 1));
    new_parts.push(claude_path.to_string_lossy().to_string());
    new_parts.extend(parts.into_iter().skip(skip));
    let new_cmd = shell_words::join(&new_parts);

    if debug_mode {
        eprintln!("CCS DEBUG: New command parts: {new_parts:?}");
        eprintln!("CCS DEBUG: New command: {new_cmd}");
        eprintln!(
            "CCS DEBUG: bypassing `ccs` wrapper for `ccs/{alias_name}` to preserve Claude CLI flag passthrough"
        );
    }

    new_cmd
}

/// Resolve flags for CCS alias config with defaults fallback.
fn resolve_ccs_flags<'a>(
    alias_config: &'a CcsAliasConfig,
    defaults: &'a CcsConfig,
) -> (String, String, String, String, bool, &'a str, String) {
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
    let print_flag = alias_config.print_flag.clone().unwrap_or_else(|| {
        let pf = defaults.print_flag.clone();
        if pf.is_empty() {
            "-p".to_string()
        } else {
            pf
        }
    });
    let json_parser = alias_config
        .json_parser
        .as_deref()
        .unwrap_or(&defaults.json_parser);
    let can_commit = alias_config.can_commit.unwrap_or(defaults.can_commit);
    let streaming_flag = alias_config
        .streaming_flag
        .clone()
        .unwrap_or_else(|| defaults.streaming_flag.clone());

    (
        output_flag,
        yolo_flag,
        verbose_flag,
        print_flag,
        can_commit,
        json_parser,
        streaming_flag,
    )
}

/// Build an `AgentConfig` for a CCS command.
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
/// Example: `ccs glm` → uses Claude parser by default (from `defaults.json_parser`)
///          `ccs gemini` → uses Claude parser by default
///          With override: `json_parser = "generic"` in alias config overrides default
///
/// Display name format: CCS aliases are shown as "ccs-{alias}" (e.g., "ccs-glm", "ccs-gemini")
/// in output/logs to make it clearer which provider is actually being used, while still using
/// the Claude parser under the hood.
///
/// # Environment Variable Loading
///
/// This function automatically loads environment variables for the resolved CCS profile using
/// CCS config mappings (`~/.ccs/config.json` / `~/.ccs/config.yaml`) and common settings file
/// naming (`~/.ccs/{profile}.settings.json` / `~/.ccs/{profile}.setting.json`). This allows
/// CCS aliases to use their configured credentials without requiring manual environment variable
/// configuration, while avoiding hard-coded assumptions about CCS' internal schema.
fn build_ccs_agent_config(
    alias_config: &CcsAliasConfig,
    defaults: &CcsConfig,
    display_name: String,
    alias_name: &str,
) -> AgentConfig {
    let debug_mode = std::env::var("RALPH_CCS_DEBUG").is_ok();

    let (env_vars, env_vars_loaded, profile_used_for_env) =
        load_ccs_env_vars_for_alias(alias_config, alias_name, debug_mode);

    let cmd = determine_ccs_command(
        alias_config,
        alias_name,
        env_vars_loaded,
        profile_used_for_env.as_deref(),
        debug_mode,
    );

    let (output_flag, yolo_flag, verbose_flag, print_flag, can_commit, json_parser, streaming_flag) =
        resolve_ccs_flags(alias_config, defaults);

    AgentConfig {
        cmd,
        output_flag,
        yolo_flag,
        verbose_flag,
        can_commit,
        json_parser: JsonParserType::parse(json_parser),
        model_flag: alias_config.model_flag.clone(),
        print_flag,
        streaming_flag,
        env_vars,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ccs_ref() {
        assert!(is_ccs_ref("ccs"));
        assert!(is_ccs_ref("ccs/work"));
        assert!(is_ccs_ref("ccs/gemini"));
        assert!(!is_ccs_ref("claude"));
        assert!(!is_ccs_ref("codex"));
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
    fn test_resolve_ccs_agent_default() {
        let aliases = HashMap::new();
        let config = resolve_ccs_agent("", &aliases, &CcsConfig::default());
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

        let config = resolve_ccs_agent("work", &aliases, &CcsConfig::default());
        assert!(config.is_some());
        let config = config.unwrap();
        // When claude binary is found, it replaces "ccs work" with the path to claude
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs work",
            "cmd should be 'ccs work' or a path ending with 'claude', got: {}",
            config.cmd
        );

        let config = resolve_ccs_agent("gemini", &aliases, &CcsConfig::default());
        assert!(config.is_some());
        let config = config.unwrap();
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs gemini",
            "cmd should be 'ccs gemini' or a path ending with 'claude', got: {}",
            config.cmd
        );

        // Unknown alias returns None
        let config = resolve_ccs_agent("unknown", &aliases, &CcsConfig::default());
        assert!(config.is_none());
    }

    #[test]
    fn test_build_ccs_agent_config() {
        let config = build_ccs_agent_config(
            &CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
            &CcsConfig::default(),
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
        let resolver = CcsAliasResolver::new(aliases, CcsConfig::default());
        assert_eq!(resolver.aliases.len(), 1);
    }

    #[test]
    fn test_ccs_alias_resolver_empty() {
        let resolver = CcsAliasResolver::empty();
        assert_eq!(resolver.aliases.len(), 0);
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
        let resolver = CcsAliasResolver::new(aliases, CcsConfig::default());

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
}
