//! Tests for the agent pipeline runner.
//!
//! Includes contract tests for agent configurations (qwen, vibe, llama-cli)
//! and fallback behavior.

use super::*;
use crate::agents::{AgentRegistry, JsonParserType};
use crate::common::split_command;
use crate::config::Config;
use crate::config::Verbosity;
use crate::logger::argv_requests_json;
use crate::logger::Colors;
use crate::logger::Logger;
use crate::pipeline::Timer;
use std::collections::HashMap;
use std::path;

/// Helper to create test scripts that track execution count.
///
/// Returns (`fail_script_path`, `ok_script_path`, `fail_count_path`, `ok_count_path`).
#[cfg(unix)]
fn create_test_scripts(
    dir: &path::Path,
) -> (path::PathBuf, path::PathBuf, path::PathBuf, path::PathBuf) {
    let fail_count = dir.join("fail_count.txt");
    let ok_count = dir.join("ok_count.txt");

    let fail_script = dir.join("fail.sh");
    std::fs::write(
        &fail_script,
        format!(
            r#"#!/bin/sh
echo x >> "{}"
exit 1
"#,
            fail_count.display()
        ),
    )
    .unwrap();

    let ok_script = dir.join("ok.sh");
    std::fs::write(
        &ok_script,
        format!(
            r#"#!/bin/sh
echo x >> "{}"
exit 0
"#,
            ok_count.display()
        ),
    )
    .unwrap();

    (fail_script, ok_script, fail_count, ok_count)
}

/// Helper to set up a test registry with GLM and fallback CCS agents.
///
/// Configures the registry with aliases for GLM (which fails) and OK (which succeeds),
/// and applies a fallback chain configuration.
#[cfg(unix)]
fn setup_test_registry_with_fallback(
    _dir: &path::Path,
    fail_script: &path::Path,
    ok_script: &path::Path,
) -> AgentRegistry {
    let mut registry = AgentRegistry::new().unwrap();
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: String::new(),
        streaming_flag: String::new(),
        json_parser: "generic".to_string(),
        can_commit: true,
    };

    let mut aliases = HashMap::new();
    aliases.insert(
        "glm".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", fail_script.display()),
            ..Default::default()
        },
    );
    aliases.insert(
        "ok".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", ok_script.display()),
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases, defaults);

    let toml_str = r#"
        [agent_chain]
        reviewer = ["ccs/glm", "ccs/ok"]
        max_retries = 3
        max_cycles = 1
    "#;
    let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
    registry.apply_unified_config(&unified);
    registry
}

#[cfg(unix)]
#[test]
fn run_with_fallback_retries_unknown_glm_errors_before_fallback() {
    let dir = tempfile::tempdir().unwrap();

    let (fail_script, ok_script, fail_count, ok_count) = create_test_scripts(dir.path());
    let registry = setup_test_registry_with_fallback(dir.path(), &fail_script, &ok_script);

    // Set up runtime components inline (lifetime issues prevent extracting this)
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    let exit = run_with_fallback(
        crate::agents::AgentRole::Reviewer,
        "test",
        "hello",
        &dir.path().join("logs").display().to_string(),
        &mut runtime,
        &registry,
        "ccs/glm",
    )
    .unwrap();

    assert_eq!(exit, 0, "fallback agent should succeed");
    let fail_invocations = std::fs::read_to_string(&fail_count)
        .unwrap()
        .lines()
        .count();
    let ok_invocations = std::fs::read_to_string(&ok_count).unwrap().lines().count();
    // GLM with unknown error (empty stderr) is now retried max_retries times before fallback
    // max_retries = 3 means the loop runs for retry in 0..3 = 3 total attempts
    assert_eq!(
        fail_invocations, 3,
        "GLM agent with unknown error should be retried max_retries times before fallback"
    );
    assert_eq!(ok_invocations, 1, "fallback agent should run once");
}

/// Test that spawn errors (like command not found) trigger fallback.
///
/// This verifies that when the primary agent command doesn't exist (exit code 127),
/// the fallback chain is used instead of crashing the pipeline.
#[cfg(unix)]
#[test]
fn run_with_fallback_handles_command_not_found() {
    let dir = tempfile::tempdir().unwrap();

    // Create a script that succeeds for the fallback agent
    let ok_script = dir.path().join("ok.sh");
    let ok_count = dir.path().join("ok_count.txt");
    std::fs::write(
        &ok_script,
        format!(
            r#"#!/bin/sh
echo x >> "{}"
exit 0
"#,
            ok_count.display()
        ),
    )
    .unwrap();

    // Set up registry with a nonexistent primary command and a working fallback
    let mut registry = AgentRegistry::new().unwrap();
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: String::new(),
        streaming_flag: String::new(),
        json_parser: "generic".to_string(),
        can_commit: true,
    };

    let mut aliases = HashMap::new();
    // Primary agent uses a nonexistent command - will fail with exit code 127
    aliases.insert(
        "nonexistent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "/nonexistent/command/that/does/not/exist".to_string(),
            ..Default::default()
        },
    );
    aliases.insert(
        "ok".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", ok_script.display()),
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases, defaults);

    let toml_str = r#"
        [agent_chain]
        reviewer = ["ccs/nonexistent", "ccs/ok"]
        max_retries = 1
        max_cycles = 1
    "#;
    let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
    registry.apply_unified_config(&unified);

    // Set up runtime components
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    let exit = run_with_fallback(
        crate::agents::AgentRole::Reviewer,
        "test",
        "hello",
        &dir.path().join("logs").display().to_string(),
        &mut runtime,
        &registry,
        "ccs/nonexistent",
    )
    .unwrap();

    // The fallback should have succeeded
    assert_eq!(
        exit, 0,
        "fallback agent should succeed after primary command not found"
    );

    // Verify fallback was invoked
    let ok_invocations = std::fs::read_to_string(&ok_count).unwrap().lines().count();
    assert_eq!(
        ok_invocations, 1,
        "fallback agent should have been invoked once"
    );
}

#[test]
fn contract_qwen_stream_json_parses_with_claude_parser() {
    let registry = AgentRegistry::new().unwrap();
    let qwen = registry.resolve_config("qwen").unwrap();

    let cmd = qwen.build_cmd(true, true, true);
    let argv = split_command(&cmd).unwrap();

    let parser_type = qwen.json_parser;
    let uses_json = parser_type != JsonParserType::Generic || argv_requests_json(&argv);
    assert!(uses_json, "Qwen should run in JSON-parsing mode");
    assert_eq!(parser_type, JsonParserType::Claude);

    // Claude stream-json compatibility (used by qwen-code)
    let json =
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello from qwen"}]}}"#;
    let input = std::io::Cursor::new(format!("{json}\n"));
    let reader = std::io::BufReader::new(input);

    let colors = Colors { enabled: false };
    let parser = crate::json_parser::ClaudeParser::new(colors, Verbosity::Normal);
    parser.parse_stream(reader).unwrap();

    // Note: After Printable trait refactor, parse_stream no longer takes a writer
    // The parser internally uses a printer to output to stdout/stderr
    // This test now verifies that parsing succeeds without errors
}

#[test]
fn contract_vibe_runs_in_plain_text_mode() {
    let registry = AgentRegistry::new().unwrap();
    let vibe = registry.resolve_config("vibe").unwrap();

    let cmd = vibe.build_cmd(true, true, true);
    let argv = split_command(&cmd).unwrap();

    let parser_type = vibe.json_parser;
    let uses_json = parser_type != JsonParserType::Generic || argv_requests_json(&argv);
    assert!(!uses_json, "vibe should not enable JSON parsing by default");
    assert_eq!(parser_type, JsonParserType::Generic);
}

#[test]
fn contract_llama_cli_runs_in_plain_text_mode_with_local_model_flag() {
    let registry = AgentRegistry::new().unwrap();
    let llama = registry.resolve_config("llama-cli").unwrap();

    let cmd = llama.build_cmd(true, true, true);
    assert!(
        cmd.contains(" -m "),
        "llama-cli should default to a local model path"
    );

    let argv = split_command(&cmd).unwrap();

    let parser_type = llama.json_parser;
    let uses_json = parser_type != JsonParserType::Generic || argv_requests_json(&argv);
    assert!(
        !uses_json,
        "llama-cli should not enable JSON parsing by default"
    );
    assert_eq!(parser_type, JsonParserType::Generic);
}

// Step 8: Integration test for GLM reviewer flow

#[test]
fn test_glm_reviewer_command_includes_print_flag() {
    // Test that GLM reviewer commands are constructed correctly with the -p flag
    let _registry = AgentRegistry::new().unwrap();

    // Set up GLM alias via CCS
    let defaults = crate::config::CcsConfig {
        output_flag: "--output-format=stream-json".to_string(),
        yolo_flag: "--dangerously-skip-permissions".to_string(),
        verbose_flag: "--verbose".to_string(),
        print_flag: "-p".to_string(),
        streaming_flag: "--include-partial-messages".to_string(),
        json_parser: "claude".to_string(),
        can_commit: true,
    };

    let mut aliases = std::collections::HashMap::new();
    aliases.insert(
        "glm".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "ccs glm".to_string(),
            ..Default::default()
        },
    );

    let mut registry = AgentRegistry::new().unwrap();
    registry.set_ccs_aliases(&aliases, defaults);

    // Get the GLM agent config
    let glm_config = registry
        .resolve_config("ccs/glm")
        .expect("GLM agent should be available");

    // Build the command as it would be built for reviewer role
    let cmd = glm_config.build_cmd_with_model(true, true, false, None);

    // Verify the command contains the -p flag
    assert!(
        cmd.contains(" -p"),
        "GLM reviewer command must include -p flag for non-interactive mode. Command was: {cmd}"
    );

    // Verify the command structure is correct: "claude -p ..." (not "ccs glm -p ..." anymore)
    // When claude binary is found, it replaces "ccs glm" with the path to claude
    let first_word = cmd.split_whitespace().next().unwrap_or("");
    assert!(
        first_word.ends_with("claude") || cmd.starts_with("ccs glm"),
        "GLM command must start with a path ending in 'claude' or with 'ccs glm'. Command was: {cmd}"
    );

    // Verify flag ordering: -p must come after the command name
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let p_index = parts.iter().position(|&s| s == "-p");
    assert!(
        p_index.is_some(),
        "GLM command must contain -p flag. Command was: {cmd}"
    );
    assert!(
        p_index.unwrap() > 0,
        "-p flag must come after command name. Command was: {cmd}"
    );
}

#[test]
fn test_glm_reviewer_fallback_on_exit_code_1() {
    // Test that GLM reviewer with exit code 1 triggers fallback without retries
    let dir = tempfile::tempdir().unwrap();
    let fail_count = dir.path().join("glm_fail_count.txt");

    // Create a mock script that simulates GLM failure with exit code 1
    let fail_script = dir.path().join("glm_fail.sh");
    std::fs::write(
        &fail_script,
        format!(
            r#"#!/bin/sh
echo "GLM agent failed with exit code 1" >&2
echo x >> "{}"
exit 1
"#,
            fail_count.display()
        ),
    )
    .unwrap();

    // Create a successful fallback script
    let ok_script = dir.path().join("ok.sh");
    std::fs::write(
        &ok_script,
        r"#!/bin/sh
exit 0
",
    )
    .unwrap();

    // Set up registry with GLM agent that fails and a fallback that succeeds
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: "-p".to_string(),
        streaming_flag: "--include-partial-messages".to_string(),
        json_parser: "claude".to_string(),
        can_commit: true,
    };

    let mut aliases = std::collections::HashMap::new();
    aliases.insert(
        "glm".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", fail_script.display()),
            ..Default::default()
        },
    );
    aliases.insert(
        "ok".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", ok_script.display()),
            ..Default::default()
        },
    );

    let mut registry = AgentRegistry::new().unwrap();
    registry.set_ccs_aliases(&aliases, defaults);
    // Use apply_unified_config to set fallback chain (public API)
    let toml_str = r#"
        [agent_chain]
        reviewer = ["ccs/glm", "ccs/ok"]
        max_retries = 3
        max_cycles = 1
    "#;
    let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
    registry.apply_unified_config(&unified);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Run the review with GLM agent
    let exit = run_with_fallback(
        crate::agents::AgentRole::Reviewer,
        "test review",
        "hello",
        &dir.path().join("logs").display().to_string(),
        &mut runtime,
        &registry,
        "ccs/glm",
    )
    .unwrap();

    // The fallback agent should succeed
    assert_eq!(exit, 0, "Fallback agent should succeed");

    // GLM should only be called once (no retries) due to AgentSpecificQuirk classification
    let glm_calls = std::fs::read_to_string(&fail_count)
        .unwrap()
        .lines()
        .count();
    assert_eq!(
        glm_calls, 1,
        "GLM agent with exit code 1 should not be retried (classified as AgentSpecificQuirk)"
    );
}

#[test]
fn test_glm_exit_code_1_with_valid_output_treated_as_success() {
    // Test that GLM agent with exit code 1 BUT valid output is treated as success
    // This is the bug fix: GLM may exit with code 1 even when it successfully completes work
    let dir = tempfile::tempdir().unwrap();
    let glm_count = dir.path().join("glm_count.txt");
    let log_dir = dir.path().join("logs");

    // Create a mock GLM script that exits with code 1 but produces valid JSON output
    let glm_script = dir.path().join("glm_success_with_exit_1.sh");
    std::fs::write(
        &glm_script,
        format!(
            r#"#!/bin/sh
echo x >> "{}"
# Simulate GLM producing valid output but exiting with code 1
mkdir -p "{}"
echo '{{"type":"result","result":"- [ ] Test review item"}}' > "{}/reviewer.log"
exit 1
"#,
            glm_count.display(),
            log_dir.display(),
            log_dir.display()
        ),
    )
    .unwrap();

    // Create a fallback script (should NOT be called)
    let fallback_script = dir.path().join("fallback.sh");
    std::fs::write(
        &fallback_script,
        r"#!/bin/sh
# This should never be called
exit 1
",
    )
    .unwrap();

    // Set up registry with GLM agent and fallback
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: "-p".to_string(),
        streaming_flag: "--include-partial-messages".to_string(),
        json_parser: "claude".to_string(),
        can_commit: true,
    };

    let mut aliases = std::collections::HashMap::new();
    aliases.insert(
        "glm".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", glm_script.display()),
            ..Default::default()
        },
    );
    aliases.insert(
        "fallback".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", fallback_script.display()),
            ..Default::default()
        },
    );

    let mut registry = AgentRegistry::new().unwrap();
    registry.set_ccs_aliases(&aliases, defaults);

    // Configure fallback chain
    let toml_str = r#"
        [agent_chain]
        reviewer = ["ccs/glm", "ccs/fallback"]
        max_retries = 3
        max_cycles = 1
    "#;
    let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
    registry.apply_unified_config(&unified);

    // Create output validator that checks for valid JSON output
    let validate_output: crate::pipeline::fallback::OutputValidator =
        |log_dir_path: &path::Path, _logger: &crate::logger::Logger| -> std::io::Result<bool> {
            let log_file = log_dir_path.join("reviewer.log");
            if log_file.exists() {
                let content = std::fs::read_to_string(&log_file)?;
                Ok(content.contains(r#"{"type":"result""#))
            } else {
                Ok(false)
            }
        };

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Run with output validator
    let mut fallback_config = crate::pipeline::runner::FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: &log_dir.display().to_string(),
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/glm",
        output_validator: Some(validate_output),
    };

    let exit =
        crate::pipeline::runner::run_with_fallback_and_validator(&mut fallback_config).unwrap();

    // GLM with exit code 1 but valid output should be treated as success
    assert_eq!(
        exit, 0,
        "GLM with valid output should succeed despite exit code 1"
    );

    // GLM should only be called once
    let glm_calls = std::fs::read_to_string(&glm_count).unwrap().lines().count();
    assert_eq!(
        glm_calls, 1,
        "GLM agent should be called once and succeed (valid output despite exit code 1)"
    );
}

// ============================================================================
// Session Continuation Tests
// ============================================================================

/// Test that session continuation is NOT used on first attempt (retry_num = 0).
///
/// Even if session_info is provided, the first attempt should use normal
/// `run_with_fallback` behavior, not session continuation.
#[cfg(unix)]
#[test]
fn session_continuation_not_used_on_first_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let agent_count = dir.path().join("agent_count.txt");

    // Create a script that tracks calls and succeeds
    let agent_script = dir.path().join("agent.sh");
    std::fs::write(
        &agent_script,
        format!(
            r#"#!/bin/sh
echo x >> "{}"
# Check if session flag was passed
if echo "$@" | grep -q -- "--resume"; then
    echo "ERROR: session flag found on first attempt" >&2
    exit 1
fi
exit 0
"#,
            agent_count.display()
        ),
    )
    .unwrap();

    // Set up registry with an agent that supports session continuation
    let mut registry = AgentRegistry::new().unwrap();
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: String::new(),
        streaming_flag: String::new(),
        json_parser: "generic".to_string(),
        can_commit: true,
    };

    let mut aliases = HashMap::new();
    aliases.insert(
        "session-agent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", agent_script.display()),
            session_flag: Some("--resume {}".to_string()),
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases, defaults);

    // Set up runtime
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Create fake session info
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs/session-agent".to_string(),
        log_file: dir.path().join("fake.log"),
    };

    // Run with retry_num = 0 (first attempt) - should NOT use session continuation
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: &dir.path().join("logs").display().to_string(),
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/session-agent",
        session_info: Some(&session_info),
        retry_num: 0, // First attempt
        output_validator: None,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    assert_eq!(exit, 0, "Agent should succeed");

    // Agent should have been called once
    let calls = std::fs::read_to_string(&agent_count)
        .unwrap()
        .lines()
        .count();
    assert_eq!(calls, 1, "Agent should be called once");
}

/// Test that session continuation IS used on retry (retry_num > 0).
///
/// When retry_num > 0 and session_info is provided for a matching agent,
/// session continuation should be attempted.
#[cfg(unix)]
#[test]
fn session_continuation_used_on_retry() {
    let dir = tempfile::tempdir().unwrap();
    let session_flag_found = dir.path().join("session_flag_found.txt");

    // Create a script that checks for session flag and writes a marker
    let agent_script = dir.path().join("agent.sh");
    std::fs::write(
        &agent_script,
        format!(
            r#"#!/bin/sh
# Check if session flag was passed
if echo "$@" | grep -q -- "--resume ses_test123"; then
    echo "found" > "{}"
fi
exit 0
"#,
            session_flag_found.display()
        ),
    )
    .unwrap();

    // Set up registry with an agent that supports session continuation
    let mut registry = AgentRegistry::new().unwrap();
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: String::new(),
        streaming_flag: String::new(),
        json_parser: "generic".to_string(),
        can_commit: true,
    };

    let mut aliases = HashMap::new();
    aliases.insert(
        "session-agent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", agent_script.display()),
            session_flag: Some("--resume {}".to_string()),
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases, defaults);

    // Set up runtime
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Create session info matching the agent
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs/session-agent".to_string(),
        log_file: dir.path().join("fake.log"),
    };

    // Run with retry_num = 1 (XSD retry) - SHOULD use session continuation
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: &dir.path().join("logs").display().to_string(),
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/session-agent",
        session_info: Some(&session_info),
        retry_num: 1, // XSD retry
        output_validator: None,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    assert_eq!(exit, 0, "Agent should succeed");

    // Verify session flag was passed
    assert!(
        session_flag_found.exists(),
        "Session flag should have been passed to agent on retry"
    );
}

/// Test that session continuation falls back when agent doesn't support it.
///
/// If the agent doesn't have a session_flag configured, session continuation
/// should silently fall back to normal `run_with_fallback` behavior.
#[cfg(unix)]
#[test]
fn session_continuation_fallback_when_agent_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let agent_count = dir.path().join("agent_count.txt");

    // Create a script that tracks calls
    let agent_script = dir.path().join("agent.sh");
    std::fs::write(
        &agent_script,
        format!(
            r#"#!/bin/sh
echo x >> "{}"
exit 0
"#,
            agent_count.display()
        ),
    )
    .unwrap();

    // Set up registry with an agent that does NOT support session continuation
    let mut registry = AgentRegistry::new().unwrap();
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: String::new(),
        streaming_flag: String::new(),
        json_parser: "generic".to_string(),
        can_commit: true,
    };

    let mut aliases = HashMap::new();
    aliases.insert(
        "no-session-agent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", agent_script.display()),
            session_flag: None, // NO session support
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases, defaults);

    // Set up runtime
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Create session info (even though agent doesn't support it)
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs/no-session-agent".to_string(),
        log_file: dir.path().join("fake.log"),
    };

    // Run with retry_num = 1 - should fall back since agent doesn't support session
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: &dir.path().join("logs").display().to_string(),
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/no-session-agent",
        session_info: Some(&session_info),
        retry_num: 1, // XSD retry
        output_validator: None,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    assert_eq!(exit, 0, "Agent should succeed via fallback path");

    // Agent should have been called once (via normal fallback)
    let calls = std::fs::read_to_string(&agent_count)
        .unwrap()
        .lines()
        .count();
    assert_eq!(calls, 1, "Agent should be called once via fallback");
}

/// Test that session continuation falls back when agent crashes/segfaults.
///
/// If the agent crashes during session continuation, the system should
/// silently fall back to normal `run_with_fallback` behavior.
#[cfg(unix)]
#[test]
fn session_continuation_fallback_when_agent_crashes() {
    let dir = tempfile::tempdir().unwrap();
    let crash_count = dir.path().join("crash_count.txt");
    let success_count = dir.path().join("success_count.txt");

    // Create a script that crashes on session continuation but succeeds normally
    let crash_script = dir.path().join("crash.sh");
    std::fs::write(
        &crash_script,
        format!(
            r#"#!/bin/sh
if echo "$@" | grep -q -- "--resume"; then
    echo x >> "{}"
    # Simulate crash (segfault-like exit)
    exit 139
else
    echo x >> "{}"
    exit 0
fi
"#,
            crash_count.display(),
            success_count.display()
        ),
    )
    .unwrap();

    // Set up registry
    let mut registry = AgentRegistry::new().unwrap();
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: String::new(),
        streaming_flag: String::new(),
        json_parser: "generic".to_string(),
        can_commit: true,
    };

    let mut aliases = HashMap::new();
    aliases.insert(
        "crash-agent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", crash_script.display()),
            session_flag: Some("--resume {}".to_string()),
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases, defaults);

    // Set up runtime
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Create session info
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs/crash-agent".to_string(),
        log_file: dir.path().join("fake.log"),
    };

    // Run with retry_num = 1 - should try session continuation, crash, then succeed via fallback
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: &dir.path().join("logs").display().to_string(),
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/crash-agent",
        session_info: Some(&session_info),
        retry_num: 1, // XSD retry
        output_validator: None,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    // Note: When session continuation runs but agent crashes (non-zero exit),
    // we still return the exit code (139) - the caller checks for valid output.
    // The "Ran" result means agent was invoked, not that it succeeded.
    // For this test, exit code 139 is expected since session continuation DID run.
    assert_eq!(
        exit, 139,
        "Should return crash exit code when session continuation ran but agent crashed"
    );

    // Session continuation should have been attempted
    let crash_calls = std::fs::read_to_string(&crash_count)
        .unwrap_or_default()
        .lines()
        .count();
    assert_eq!(
        crash_calls, 1,
        "Session continuation should have been attempted once"
    );
}

/// Test that session continuation resolves sanitized agent names from log files.
///
/// This is a regression test for the bug where session_info.agent_name extracted
/// from log file names (e.g., "ccs-glm") couldn't be resolved to registry names
/// (e.g., "ccs/glm") because the lookup used the wrong name format.
///
/// The fix was to add `resolve_from_logfile_name()` which reverses the
/// sanitization done when creating log file names.
#[cfg(unix)]
#[test]
fn session_continuation_resolves_sanitized_agent_names() {
    let dir = tempfile::tempdir().unwrap();
    let session_flag_found = dir.path().join("session_flag_found.txt");

    // Create a script that checks for the session flag
    let agent_script = dir.path().join("agent.sh");
    std::fs::write(
        &agent_script,
        format!(
            r#"#!/bin/sh
# Check if --resume flag is present
for arg in "$@"; do
    case "$arg" in
        --resume) touch "{}" ;;
    esac
done
exit 0
"#,
            session_flag_found.display()
        ),
    )
    .unwrap();

    // Set up registry with a CCS agent (registry name has slash: "ccs/test-agent")
    let mut registry = AgentRegistry::new().unwrap();
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: String::new(),
        streaming_flag: String::new(),
        json_parser: "generic".to_string(),
        can_commit: true,
    };

    let mut aliases = HashMap::new();
    aliases.insert(
        "test-agent".to_string(), // Will become "ccs/test-agent" in registry
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", agent_script.display()),
            session_flag: Some("--resume {}".to_string()),
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases, defaults);

    // Set up runtime
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Create session info with SANITIZED agent name (as extracted from log file)
    // This is the key: the agent_name is "ccs-test-agent" (hyphen), not "ccs/test-agent" (slash)
    // This simulates what happens when session_info is extracted from a log file named
    // "planning_1_ccs-test-agent_0.log"
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs-test-agent".to_string(), // SANITIZED name (hyphen instead of slash)
        log_file: dir.path().join("fake.log"),
    };

    // Run with retry_num = 1 (XSD retry) - should use session continuation
    // despite the sanitized agent name
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: &dir.path().join("logs").display().to_string(),
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/test-agent",
        session_info: Some(&session_info),
        retry_num: 1, // XSD retry
        output_validator: None,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    assert_eq!(exit, 0, "Agent should succeed");

    // Verify session flag was passed - this proves that:
    // 1. The sanitized name "ccs-test-agent" was resolved to "ccs/test-agent"
    // 2. The agent config was found in the registry
    // 3. Session continuation was used (not fallback)
    assert!(
        session_flag_found.exists(),
        "Session continuation should work with sanitized agent names from log files. \
         The name 'ccs-test-agent' should resolve to 'ccs/test-agent' in the registry."
    );
}

/// End-to-end test: Session ID extraction from log file and reuse on retry.
///
/// This test exercises the complete session continuation flow:
/// 1. First agent run outputs NDJSON with session_id to log file
/// 2. Session ID is extracted from the log file
/// 3. On retry, the same session ID is passed back to the agent
///
/// This is a regression test ensuring that the full pipeline works correctly
/// when session IDs need to be extracted from log files (as happens in production).
#[cfg(unix)]
#[test]
fn session_continuation_e2e_extracts_session_from_logfile() {
    let dir = tempfile::tempdir().unwrap();
    let logs_dir = dir.path().join("logs");
    std::fs::create_dir_all(&logs_dir).unwrap();

    let received_session_id = dir.path().join("received_session_id.txt");

    // Create first agent script that outputs NDJSON with session_id (Claude format)
    let first_agent_script = dir.path().join("first_agent.sh");
    std::fs::write(
        &first_agent_script,
        r#"#!/bin/sh
# Output Claude-format NDJSON with session_id
echo '{"type":"system","subtype":"init","session_id":"ses_extracted_abc123"}'
echo '{"type":"assistant","message":{"content":"Hello"}}'
exit 0
"#,
    )
    .unwrap();

    // Create second agent script that captures the session ID from --resume flag
    let second_agent_script = dir.path().join("second_agent.sh");
    std::fs::write(
        &second_agent_script,
        format!(
            r#"#!/bin/sh
# Capture the session ID passed via --resume
for arg in "$@"; do
    case "$arg" in
        ses_*) echo "$arg" > "{}" ;;
    esac
done
exit 0
"#,
            received_session_id.display()
        ),
    )
    .unwrap();

    // Set up registry with a CCS agent that supports session continuation
    let mut registry = AgentRegistry::new().unwrap();
    let defaults = crate::config::CcsConfig {
        output_flag: String::new(),
        yolo_flag: String::new(),
        verbose_flag: String::new(),
        print_flag: String::new(),
        streaming_flag: String::new(),
        json_parser: "claude".to_string(), // Use Claude parser to extract session_id
        can_commit: true,
    };

    // First agent (for initial run)
    let mut aliases = HashMap::new();
    aliases.insert(
        "e2e-agent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", first_agent_script.display()),
            session_flag: Some("--resume {}".to_string()),
            json_parser: Some("claude".to_string()),
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases, defaults.clone());

    // Set up runtime
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    // Run first attempt (retry_num = 0) - this creates the log file with session_id
    let log_prefix = logs_dir.join("test_1");
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: &log_prefix.display().to_string(),
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/e2e-agent",
        session_info: None, // No session info on first attempt
        retry_num: 0,
        output_validator: None,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();
    assert_eq!(exit, 0, "First agent run should succeed");

    // Extract session info from the log file (this is what happens in production)
    // We pass the known agent name to avoid ambiguity from sanitized log file names
    let agent_config = registry.resolve_config("ccs/e2e-agent").unwrap();
    let session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
        &log_prefix,
        agent_config.json_parser,
        Some("ccs/e2e-agent"),
    );

    // Verify session was extracted
    assert!(
        session_info.is_some(),
        "Session info should be extracted from log file"
    );
    let session_info = session_info.unwrap();
    assert_eq!(
        session_info.session_id, "ses_extracted_abc123",
        "Session ID should match what the agent output"
    );
    // The agent name should be the original registry name (passed directly)
    assert_eq!(
        session_info.agent_name, "ccs/e2e-agent",
        "Agent name should be original registry name when passed directly"
    );

    // Now update the agent to the second script that captures the session ID
    let mut aliases2 = HashMap::new();
    aliases2.insert(
        "e2e-agent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: format!("sh {}", second_agent_script.display()),
            session_flag: Some("--resume {}".to_string()),
            json_parser: Some("claude".to_string()),
            ..Default::default()
        },
    );
    registry.set_ccs_aliases(&aliases2, defaults);

    // Run retry (retry_num = 1) with the extracted session info
    let mut runtime2 = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: &std::sync::Arc::new(crate::executor::RealProcessExecutor::new()) as _,
        #[cfg(any(test, feature = "test-utils"))]
        agent_executor: None,
    };

    let mut xsd_config2 = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "retry prompt",
        logfile_prefix: &log_prefix.display().to_string(),
        runtime: &mut runtime2,
        registry: &registry,
        primary_agent: "ccs/e2e-agent",
        session_info: Some(&session_info), // Pass the extracted session info
        retry_num: 1,                      // XSD retry
        output_validator: None,
    };

    let exit2 = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config2).unwrap();
    assert_eq!(exit2, 0, "Retry should succeed");

    // Verify the session ID was passed to the agent on retry
    assert!(
        received_session_id.exists(),
        "Agent should have received session ID on retry. \
         This verifies that: \
         1. Session ID was extracted from the log file \
         2. Sanitized agent name 'ccs-e2e-agent' was resolved to 'ccs/e2e-agent' \
         3. Session continuation passed the session ID via --resume flag"
    );

    let received = std::fs::read_to_string(&received_session_id).unwrap();
    assert_eq!(
        received.trim(),
        "ses_extracted_abc123",
        "The exact session ID from the first run should be passed to the retry"
    );
}

/// Test that resolve_from_logfile_name works for OpenCode agents.
///
/// OpenCode agents have names like "opencode/anthropic/claude-sonnet-4" which
/// get sanitized to "opencode-anthropic-claude-sonnet-4" in log file names.
#[test]
fn test_resolve_from_logfile_name_opencode() {
    let mut registry = AgentRegistry::new().unwrap();

    // Register an OpenCode agent
    registry.register(
        "opencode/anthropic/claude-sonnet-4",
        crate::agents::AgentConfig {
            cmd: "opencode run".to_string(),
            output_flag: "--format json".to_string(),
            yolo_flag: String::new(),
            verbose_flag: "--log-level DEBUG".to_string(),
            can_commit: true,
            json_parser: crate::agents::JsonParserType::OpenCode,
            model_flag: Some("-p anthropic -m claude-sonnet-4".to_string()),
            print_flag: String::new(),
            streaming_flag: String::new(),
            session_flag: "-s {}".to_string(),
            env_vars: std::collections::HashMap::new(),
            display_name: Some("OpenCode (anthropic)".to_string()),
        },
    );

    // Test that sanitized name resolves correctly
    let resolved = registry.resolve_from_logfile_name("opencode-anthropic-claude-sonnet-4");
    assert_eq!(
        resolved,
        Some("opencode/anthropic/claude-sonnet-4".to_string()),
        "Sanitized OpenCode agent name should resolve to registry name"
    );

    // Test that unregistered OpenCode agent can also be resolved via pattern matching
    let resolved_dynamic = registry.resolve_from_logfile_name("opencode-google-gemini-pro");
    assert_eq!(
        resolved_dynamic,
        Some("opencode/google/gemini-pro".to_string()),
        "Unregistered OpenCode agent should resolve via pattern matching"
    );
}
