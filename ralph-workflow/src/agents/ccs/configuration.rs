// CCS configuration handling - config types, aliases, environment variables

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
        (cfg.clone(), format!("ccs-{alias}"))
    } else {
        // Unknown alias - return None so caller can fall back
        return None;
    };

    Some(build_ccs_agent_config(&cmd, defaults, display_name, alias))
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
/// Example: `ccs glm` -> uses Claude parser by default (from `defaults.json_parser`)
///          `ccs gemini` -> uses Claude parser by default
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
/// Log CCS environment variables loading status (debug mode only).
///
/// Only logs whitelisted "safe" environment variable keys to prevent accidental
/// leakage of sensitive credential values. Keys containing patterns like "token",
/// "key", "secret", "password", "auth" are always filtered out regardless of
/// their actual value, to protect against custom credential formats.
///
/// Debug summary of CCS environment variables loaded for a profile.
/// Public in test mode for testing; crate-private in production.
pub struct CcsEnvVarDebugSummary {
    pub whitelisted_keys_present: Vec<String>,
    pub redacted_sensitive_keys: usize,
    pub hidden_non_whitelisted_keys: usize,
}

pub fn ccs_env_var_debug_summary(env_vars: &HashMap<String, String>) -> CcsEnvVarDebugSummary {
    // Whitelist of safe-to-log environment variable keys.
    // These are configuration keys, not credentials, so it's safe to log them.
    const SAFE_KEYS: &[&str] = &[
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
    ];

    let mut whitelisted_keys_present = Vec::new();
    let mut redacted_sensitive_keys = 0usize;
    let mut hidden_non_whitelisted_keys = 0usize;

    for key in env_vars.keys() {
        let key_upper = key.to_uppercase();

        if SAFE_KEYS.contains(&key_upper.as_str()) {
            whitelisted_keys_present.push(key_upper);
            continue;
        }

        // Treat as sensitive if it looks secret-like. Otherwise it's just hidden because it's not on the whitelist.
        let key_normalized = key_upper
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>();

        let looks_sensitive = key_normalized.contains("TOKEN")
            || key_normalized.contains("SECRET")
            || key_normalized.contains("PASSWORD")
            || key_normalized.contains("AUTH")
            || key_normalized.contains("KEY")
            || key_normalized.contains("API")
            || key_normalized == "AUTHORIZATION";

        if looks_sensitive {
            redacted_sensitive_keys += 1;
        } else {
            hidden_non_whitelisted_keys += 1;
        }
    }

    whitelisted_keys_present.sort();
    whitelisted_keys_present.dedup();

    CcsEnvVarDebugSummary {
        whitelisted_keys_present,
        redacted_sensitive_keys,
        hidden_non_whitelisted_keys,
    }
}

fn log_ccs_env_vars_loaded(
    debug_mode: bool,
    alias_name: &str,
    profile_used_for_env: Option<&String>,
    env_vars_loaded: bool,
    env_vars: &HashMap<String, String>,
) {
    if !debug_mode || alias_name.is_empty() {
        return;
    }
    let profile = profile_used_for_env.map_or(alias_name, |s| s.as_str());
    if env_vars_loaded {
        let summary = ccs_env_var_debug_summary(env_vars);

        eprintln!(
            "CCS DEBUG: Loaded {} environment variable(s) for profile '{}'",
            env_vars.len(),
            profile
        );

        for key in &summary.whitelisted_keys_present {
            eprintln!("CCS DEBUG:   - {key}");
        }

        if summary.redacted_sensitive_keys > 0 {
            eprintln!(
                "CCS DEBUG:   - ({} sensitive key(s) redacted)",
                summary.redacted_sensitive_keys
            );
        }

        if summary.hidden_non_whitelisted_keys > 0 {
            eprintln!(
                "CCS DEBUG:   - ({} non-whitelisted key(s) hidden)",
                summary.hidden_non_whitelisted_keys
            );
        }
    } else {
        eprintln!("CCS DEBUG: Failed to load environment variables for profile '{profile}'");
    }
}

fn is_glm_alias(alias_name: &str) -> bool {
    alias_name.eq_ignore_ascii_case("glm")
}

/// Resolve the CCS command, potentially bypassing the ccs wrapper for direct claude binary.
///
/// For CCS aliases, we try to use `claude` directly instead of the `ccs` wrapper
/// because the wrapper does not pass through all flags properly (especially
/// streaming-related flags like --include-partial-messages).
///
/// We only bypass the wrapper when:
/// - The agent name is `ccs/<alias>` (not plain `ccs`)
/// - We successfully loaded at least one env var for that profile
/// - The configured command targets that profile (e.g. `ccs <profile>` or `ccs api <profile>`
pub fn resolve_ccs_command(
    alias_config: &CcsAliasConfig,
    alias_name: &str,
    env_vars_loaded: bool,
    profile_used_for_env: Option<&String>,
    debug_mode: bool,
) -> String {
    let original_cmd = alias_config.cmd.as_str();

    find_claude_binary().map_or_else(
        || {
            // Could not find claude binary, use original command
            // This may result in suboptimal flag passthrough, but is better than breaking
            if original_cmd.starts_with("ccs ") || original_cmd == "ccs" {
                if debug_mode {
                    eprintln!("CCS DEBUG: Claude binary not found in PATH");
                }
                eprintln!("Warning: `claude` binary not found in PATH, using `ccs` wrapper");
                eprintln!(
                    "  This may cause issues with streaming flags like --include-partial-messages"
                );
                eprintln!(
                    "  Consider installing the Claude CLI (see your internal install docs, or the upstream Claude CLI installer)"
                );
            }
            original_cmd.to_string()
        },
        |claude_path| {
            // Only GLM supports bypassing the `ccs` wrapper.
            // Other CCS profiles (gemini, codex, etc.) must run through `ccs` directly so
            // CCS can initialize provider-specific state.
            let can_bypass_wrapper = is_glm_alias(alias_name) && env_vars_loaded;

            // Debug logging
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
                .or_else(|| profile_used_for_env.cloned())
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
        },
    )
}

/// Build the final `AgentConfig` from alias config and defaults.
fn build_ccs_config_from_flags(
    alias_config: &CcsAliasConfig,
    defaults: &CcsConfig,
    cmd: String,
    env_vars: HashMap<String, String>,
    display_name: String,
) -> AgentConfig {
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
    // CCS headless behavior: when invoking via the `ccs` wrapper, avoid `-p` because CCS
    // interprets `-p`/`--prompt` as its own headless delegation mode.
    // Use Claude's long-form `--print` flag for non-interactive mode instead.
    // If defaults.print_flag is empty (missing config), fall back to "--print".
    let print_flag = alias_config.print_flag.clone().unwrap_or_else(|| {
        let pf = defaults.print_flag.clone();
        if pf.is_empty() {
            // Hardcoded safety fallback: use --print to avoid CCS delegation interception
            "--print".to_string()
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

    // Session continuation flag: prefer alias-specific override, then unified-config CCS defaults.
    // This is used for XSD retry loops to continue an existing conversation.
    let session_flag = alias_config
        .session_flag
        .clone()
        .unwrap_or_else(|| defaults.session_flag.clone());

    AgentConfig {
        cmd, // Uses `claude` directly if found, otherwise falls back to original command
        output_flag,
        yolo_flag,
        verbose_flag,
        can_commit,
        json_parser: JsonParserType::parse(json_parser),
        model_flag: alias_config.model_flag.clone(),
        print_flag, // Default: --print (safe for `ccs` wrapper); user can override per-alias
        streaming_flag, // Required for JSON streaming when using -p
        session_flag, // Session continuation flag for XSD retries
        env_vars,   // Loaded from CCS settings for the resolved profile, if available
        display_name: Some(display_name),
    }
}

/// CCS aliases to use their configured credentials without requiring manual environment variable
/// configuration, while avoiding hard-coded assumptions about CCS' internal schema.
pub fn build_ccs_agent_config(
    alias_config: &CcsAliasConfig,
    defaults: &CcsConfig,
    display_name: String,
    alias_name: &str,
) -> AgentConfig {
    // Check for CCS_DEBUG env var to enable detailed logging
    let debug_mode = std::env::var("RALPH_CCS_DEBUG").is_ok();

    let mut profile_used_for_env: Option<String> = None;
    let (env_vars, env_vars_loaded) = if alias_name.is_empty() {
        (HashMap::new(), false)
    } else if is_glm_alias(alias_name) {
        let original_cmd = alias_config.cmd.as_str();
        let profile =
            ccs_profile_from_command(original_cmd).unwrap_or_else(|| alias_name.to_string());
        profile_used_for_env = Some(profile.clone());
        match load_ccs_env_vars_with_guess(&profile) {
            Ok((vars, guessed)) => {
                if let Some(guessed) = guessed {
                    eprintln!("Info: CCS profile '{profile}' not found; using '{guessed}'");
                }
                let loaded = !vars.is_empty();
                (vars, loaded)
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
                (HashMap::new(), false)
            }
        }
    } else {
        // Non-GLM CCS aliases must execute `ccs ...` directly.
        // Do not inject GLM/Anthropic-style env vars for other providers.
        (HashMap::new(), false)
    };

    // Debug logging: Show env vars loaded
    log_ccs_env_vars_loaded(
        debug_mode,
        alias_name,
        profile_used_for_env.as_ref(),
        env_vars_loaded,
        &env_vars,
    );

    // Determine the command to use
    let cmd = resolve_ccs_command(
        alias_config,
        alias_name,
        env_vars_loaded,
        profile_used_for_env.as_ref(),
        debug_mode,
    );

    // Build the final AgentConfig
    build_ccs_config_from_flags(alias_config, defaults, cmd, env_vars, display_name)
}

/// CCS alias resolver that can be used by the agent registry.
#[derive(Debug, Clone, Default)]
pub struct CcsAliasResolver {
    aliases: HashMap<String, CcsAliasConfig>,
    defaults: CcsConfig,
}

impl CcsAliasResolver {
    /// Create a new CCS alias resolver with the given aliases.
    pub const fn new(aliases: HashMap<String, CcsAliasConfig>, defaults: CcsConfig) -> Self {
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
            cmd: format!("ccs {alias}"),
            ..CcsAliasConfig::default()
        };
        let display_name = format!("ccs-{alias}");
        Some(build_ccs_agent_config(
            &cmd,
            &self.defaults,
            display_name,
            alias,
        ))
    }
}
