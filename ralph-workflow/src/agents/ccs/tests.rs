// Tests for CCS (Claude Code Switch) alias resolution

use super::*;

use std::sync::Mutex;

// NOTE: Some tests in this file need to temporarily modify process-wide env vars
// (PATH/CCS_HOME). Guard them with a mutex to reduce cross-test interference.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    old: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let old = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, old }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(old) = &self.old {
            std::env::set_var(self.key, old);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn make_temp_dir(prefix: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    dir.push(format!("ralph-ccs-tests-{prefix}-{pid}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn install_fake_claude_on_path() -> (std::path::PathBuf, EnvVarGuard) {
    let bin_dir = make_temp_dir("bin");

    #[cfg(windows)]
    let claude_path = bin_dir.join("claude.cmd");
    #[cfg(not(windows))]
    let claude_path = bin_dir.join("claude");

    #[cfg(windows)]
    {
        std::fs::write(&claude_path, "@echo off\recho claude\r\n").expect("write fake claude");
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&claude_path, "#!/bin/sh\necho claude\n").expect("write fake claude");
        let mut perms = std::fs::metadata(&claude_path)
            .expect("stat fake claude")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&claude_path, perms).expect("chmod fake claude");
    }

    // Prepend our bin dir to PATH (avoid clobbering the existing PATH).
    let old_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let guard = EnvVarGuard::set("PATH", &new_path);

    (claude_path, guard)
}

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
fn test_ccs_alias_resolver_empty() {
    let resolver = CcsAliasResolver::empty();
    // Empty resolver has no aliases; only plain "ccs" should resolve to default
    assert!(resolver.try_resolve("ccs").is_some());
    // Any ccs/<alias> should still resolve with default config for direct execution
    assert!(resolver.try_resolve("ccs/unknown").is_some());
}

#[test]
fn test_ccs_alias_resolver_with_aliases_resolves() {
    // Behavioral test: resolver with configured aliases should resolve them
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

    // Resolve ccs/work - should use configured alias
    let config = resolver.try_resolve("ccs/work");
    assert!(config.is_some());
    let work_cmd = config.unwrap().cmd;
    assert!(
        work_cmd.ends_with("claude") || work_cmd == "ccs work",
        "cmd should be 'ccs work' or a path ending with 'claude', got: {work_cmd}"
    );

    // Resolve ccs/personal - should use configured alias
    let config = resolver.try_resolve("ccs/personal");
    assert!(config.is_some());
    let personal_cmd = config.unwrap().cmd;
    assert!(
        personal_cmd.ends_with("claude") || personal_cmd == "ccs personal",
        "cmd should be 'ccs personal' or a path ending with 'claude', got: {personal_cmd}"
    );

    // Resolve plain "ccs" (default)
    let config = resolver.try_resolve("ccs");
    assert!(config.is_some());
    let default_cmd = config.unwrap().cmd;
    assert!(
        default_cmd.ends_with("claude") || default_cmd == "ccs",
        "cmd should be 'ccs' or a path ending with 'claude', got: {default_cmd}"
    );

    // Unknown alias - now resolves with default config for direct CCS execution
    let config = resolver.try_resolve("ccs/unknown");
    assert!(config.is_some());
    let unknown_cmd = config.unwrap().cmd;
    assert!(
        unknown_cmd.ends_with("claude") || unknown_cmd == "ccs unknown",
        "cmd should be 'ccs unknown' or a path ending with 'claude', got: {unknown_cmd}"
    );

    // Not a CCS ref
    let config = resolver.try_resolve("claude");
    assert!(config.is_none());
}

#[test]
fn test_ccs_references_resolve() {
    // Behavioral test: verify CCS references can be distinguished from non-CCS refs
    // by checking if try_resolve returns Some vs None
    let resolver = CcsAliasResolver::empty();

    // CCS references should resolve (including unregistered ones)
    assert!(resolver.try_resolve("ccs").is_some());
    assert!(resolver.try_resolve("ccs/work").is_some());
    assert!(resolver.try_resolve("ccs/unknown").is_some());

    // Non-CCS references should not resolve
    assert!(resolver.try_resolve("claude").is_none());
    assert!(resolver.try_resolve("codex").is_none());
}

#[test]
fn test_ccs_alias_resolver_multiple_aliases_resolve_correctly() {
    // Behavioral test: multiple configured aliases all resolve correctly
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

    // Each configured alias should resolve with its specific command
    let work_config = resolver.try_resolve("ccs/work").unwrap();
    assert!(
        work_config.cmd.contains("work") || work_config.cmd.ends_with("claude"),
        "work alias should resolve with 'work' in command or end with claude"
    );

    let personal_config = resolver.try_resolve("ccs/personal").unwrap();
    assert!(
        personal_config.cmd.contains("personal") || personal_config.cmd.ends_with("claude"),
        "personal alias should resolve with 'personal' in command or end with claude"
    );
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
    // IMPORTANT: CCS uses `-p/--prompt` for headless delegation.
    // When invoking Claude through CCS (e.g. `ccs codex`), we must use Claude's
    // `--print` flag instead of `-p` to avoid triggering CCS delegation.
    assert_eq!(config.print_flag, "--print");
    assert_eq!(config.session_flag, "--resume {}");
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
    // Behavioral test: CCS refs resolve, non-CCS refs don't
    assert!(resolver.try_resolve("ccs/work").is_some());
    assert!(resolver.try_resolve("claude").is_none()); // Not a CCS ref
    assert!(resolver.try_resolve("codex").is_none()); // Not a CCS ref

    // The resolved config should be usable
    let config = resolver.try_resolve("ccs/work").unwrap();
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
    // Verify that GLM commands include the print flag for non-interactive mode
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

    // Verify print_flag is set (from defaults)
    assert_eq!(glm_config.print_flag, "--print");

    // Build the command and verify --print is included
    let cmd = glm_config.build_cmd(true, true, true);
    assert!(
        cmd.contains(" --print"),
        "GLM command must include --print flag"
    );
    // When claude binary is found, command should contain "claude" as the base command
    // The actual command is now "claude --print ..." instead of "ccs glm --print ..."
    // We check if the first word (before any space) ends with "claude"
    let first_word = cmd.split_whitespace().next().unwrap_or("");
    assert!(
        first_word.ends_with("claude") || cmd.contains("ccs glm"),
        "Command should start with a path ending in 'claude' or contain 'ccs glm', got: {cmd}"
    );
}

#[test]
fn test_ccs_glm_flag_ordering() {
    // Verify that flags are in the correct order for CCS GLM
    // The --print flag must come AFTER the command name
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
        "First part should end with 'claude' or be 'ccs', got: {first_part}"
    );

    // --print flag should come after the command name
    let p_index = parts.iter().position(|&s| s == "--print");
    assert!(p_index.is_some(), "--print flag must be present");
    assert!(
        p_index.unwrap() > 0,
        "--print flag must come after command name"
    );
}

#[test]
fn test_ccs_glm_with_empty_print_override() {
    // Test that if user explicitly sets print_flag to empty, it stays empty
    let mut aliases = HashMap::new();
    aliases.insert(
        "glm".to_string(),
        CcsAliasConfig {
            cmd: "ccs glm".to_string(),
            print_flag: Some(String::new()), // Explicit empty override
            ..CcsAliasConfig::default()
        },
    );

    let resolver = CcsAliasResolver::new(aliases, default_ccs());
    let glm_config = resolver.try_resolve("ccs/glm").unwrap();

    // User override should take precedence
    assert_eq!(glm_config.print_flag, "");

    // Command should NOT include --print (user explicitly disabled it)
    let cmd = glm_config.build_cmd(true, true, true);
    assert!(
        !cmd.contains(" --print"),
        "Command should not include --print when explicitly disabled"
    );
}

#[test]
fn test_glm_error_classification() {
    // GLM exit code 1 behavior:
    // - Empty/unknown stderr -> RetryableAgentQuirk (should retry)
    // - Known problematic patterns -> AgentSpecificQuirk or ToolExecutionFailed (both trigger fallback)
    use crate::agents::error::AgentErrorKind;

    // Empty stderr with GLM agent - treat as retryable quirk
    let error = AgentErrorKind::classify_with_agent(1, "", Some("ccs/glm"), None);
    assert_eq!(error, AgentErrorKind::RetryableAgentQuirk);

    // Generic error message with CCS GLM agent - unknown pattern, should retry
    let error = AgentErrorKind::classify_with_agent(1, "some error", Some("ccs/glm"), None);
    assert_eq!(error, AgentErrorKind::RetryableAgentQuirk);

    // GLM mentioned in stderr - known issue, should fallback
    let error = AgentErrorKind::classify_with_agent(1, "glm failed", Some("ccs"), Some("glm"));
    assert_eq!(error, AgentErrorKind::AgentSpecificQuirk);

    // Permission denied - caught by check_tool_failures first, should fallback
    // (ToolExecutionFailed also triggers fallback, just via a different code path)
    let error = AgentErrorKind::classify_with_agent(1, "permission denied", Some("ccs/glm"), None);
    assert_eq!(error, AgentErrorKind::ToolExecutionFailed);

    // CCS/GLM with failed message - known issue, should fallback
    let error = AgentErrorKind::classify_with_agent(1, "ccs glm failed", Some("ccs/glm"), None);
    assert_eq!(error, AgentErrorKind::AgentSpecificQuirk);
}

// Tests for profile fuzzy matching (choose_best_profile_guess)

#[test]
fn test_choose_best_profile_guess_exact_match() {
    let suggestions = vec!["work".to_string(), "personal".to_string()];
    let result = choose_best_profile_guess("work", &suggestions);
    assert_eq!(result, Some("work"));
}

#[test]
fn test_choose_best_profile_guess_case_insensitive() {
    let suggestions = vec!["Work".to_string(), "Personal".to_string()];
    let result = choose_best_profile_guess("work", &suggestions);
    assert_eq!(result, Some("Work"));
}

#[test]
fn test_choose_best_profile_guess_single_suggestion() {
    let suggestions = vec!["only-option".to_string()];
    let result = choose_best_profile_guess("typo", &suggestions);
    assert_eq!(result, Some("only-option"));
}

#[test]
fn test_choose_best_profile_guess_prefix_match() {
    let suggestions = vec!["work-main".to_string(), "personal".to_string()];
    let result = choose_best_profile_guess("work", &suggestions);
    assert_eq!(result, Some("work-main"));
}

#[test]
fn test_choose_best_profile_guess_no_match_returns_first() {
    let suggestions = vec!["first".to_string(), "second".to_string()];
    let result = choose_best_profile_guess("nomatch", &suggestions);
    assert_eq!(result, Some("first"));
}

#[test]
fn test_choose_best_profile_guess_empty_suggestions() {
    let suggestions: Vec<String> = vec![];
    let result = choose_best_profile_guess("work", &suggestions);
    assert_eq!(result, None);
}

#[test]
fn test_non_glm_never_bypasses_ccs_wrapper_even_if_env_vars_loaded() {
    let _lock = ENV_LOCK.lock().unwrap();
    let (_claude_path, _path_guard) = install_fake_claude_on_path();

    let alias_config = CcsAliasConfig {
        cmd: "ccs gemini".to_string(),
        ..CcsAliasConfig::default()
    };

    // Simulate: env vars were loaded, and claude exists on PATH.
    // Desired behavior: only GLM is allowed to bypass; everything else must run `ccs ...`.
    let resolved = resolve_ccs_command(&alias_config, "gemini", true, None, false);
    assert_eq!(resolved, "ccs gemini");
}

#[test]
fn test_glm_can_bypass_ccs_wrapper_when_env_vars_loaded() {
    let _lock = ENV_LOCK.lock().unwrap();
    let (claude_path, _path_guard) = install_fake_claude_on_path();

    let alias_config = CcsAliasConfig {
        cmd: "ccs glm".to_string(),
        ..CcsAliasConfig::default()
    };

    let resolved = resolve_ccs_command(&alias_config, "glm", true, None, false);
    assert_eq!(resolved, claude_path.to_string_lossy().to_string());
}

#[test]
fn test_build_ccs_agent_config_skips_env_var_loading_for_non_glm() {
    let _lock = ENV_LOCK.lock().unwrap();
    let (_claude_path, _path_guard) = install_fake_claude_on_path();

    // Create a fake CCS_HOME with a working gemini profile env var file.
    let home = make_temp_dir("home");
    let ccs_dir = home.join(".ccs");
    std::fs::create_dir_all(&ccs_dir).expect("create .ccs dir");
    std::fs::write(
        ccs_dir.join("config.json"),
        r#"{"profiles":{"gemini":"gemini.settings.json"}}"#,
    )
    .expect("write config.json");
    std::fs::write(
        ccs_dir.join("gemini.settings.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://example","ANTHROPIC_AUTH_TOKEN":"tok"}}"#,
    )
    .expect("write settings");

    let _ccs_home_guard = EnvVarGuard::set("CCS_HOME", home.to_string_lossy().as_ref());

    let config = build_ccs_agent_config(
        &CcsAliasConfig {
            cmd: "ccs gemini".to_string(),
            ..CcsAliasConfig::default()
        },
        &default_ccs(),
        "ccs-gemini".to_string(),
        "gemini",
    );

    // Non-GLM aliases must not use GLM-style CCS env var injection.
    assert_eq!(config.cmd, "ccs gemini");
    assert!(config.env_vars.is_empty());
}

#[test]
fn test_build_ccs_agent_config_loads_env_vars_for_glm() {
    let _lock = ENV_LOCK.lock().unwrap();
    let (claude_path, _path_guard) = install_fake_claude_on_path();

    // Create a fake CCS_HOME with a working glm profile env var file.
    let home = make_temp_dir("home2");
    let ccs_dir = home.join(".ccs");
    std::fs::create_dir_all(&ccs_dir).expect("create .ccs dir");
    std::fs::write(
        ccs_dir.join("config.json"),
        r#"{"profiles":{"glm":"glm.settings.json"}}"#,
    )
    .expect("write config.json");
    std::fs::write(
        ccs_dir.join("glm.settings.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.example","ANTHROPIC_AUTH_TOKEN":"tok","ANTHROPIC_MODEL":"glm-4.7","CUSTOM_ENV":"value"}}"#,
    )
    .expect("write settings");

    let _ccs_home_guard = EnvVarGuard::set("CCS_HOME", home.to_string_lossy().as_ref());

    let config = build_ccs_agent_config(
        &CcsAliasConfig {
            cmd: "ccs glm".to_string(),
            ..CcsAliasConfig::default()
        },
        &default_ccs(),
        "ccs-glm".to_string(),
        "glm",
    );

    assert_eq!(config.cmd, claude_path.to_string_lossy().to_string());
    assert!(config.env_vars.contains_key("ANTHROPIC_MODEL"));

    // Sanity-check debug summary classification logic.
    let summary = super::ccs_env_var_debug_summary(&config.env_vars);
    assert!(
        summary
            .whitelisted_keys_present
            .iter()
            .any(|k| k == "ANTHROPIC_MODEL"),
        "Expected ANTHROPIC_MODEL to be whitelisted"
    );
    assert_eq!(summary.hidden_non_whitelisted_keys, 1);
    assert_eq!(summary.redacted_sensitive_keys, 1);
}
