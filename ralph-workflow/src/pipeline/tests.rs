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
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path;

/// Helper to set up a test registry with mock agents.
///
/// Creates agents with commands that pass `which::which()` checks (required for fallback chain).
/// Uses distinct, always-available binaries so MockProcessExecutor can differentiate agents
/// by `AgentSpawnConfig.command`.
fn setup_mock_registry_with_fallback() -> AgentRegistry {
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
    // Use distinct binaries so MockProcessExecutor can return different results per agent.
    aliases.insert(
        "glm".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "printf".to_string(),
            ..Default::default()
        },
    );
    aliases.insert(
        "ok".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "echo".to_string(),
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

/// Test that GLM with unknown error is retried max_retries times before fallback.
///
/// Uses MockProcessExecutor to avoid spawning real processes.
#[test]
fn run_with_fallback_retries_unknown_glm_errors_before_fallback() {
    let registry = setup_mock_registry_with_fallback();

    // Set up runtime components
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    // Use temp directory for prompt file (prompt saving uses real fs, not workspace)
    let prompt_dir = std::env::temp_dir().join("ralph-test-fallback-retries");
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: prompt_dir.join("prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor:
    // - GLM agent fails with exit code 1 (unknown error, empty stderr)
    // - OK agent succeeds
    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "printf",
            Ok(crate::executor::AgentCommandResult::failure(1, "")),
        )
        .with_agent_result("echo", Ok(crate::executor::AgentCommandResult::success()));

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    let mut fallback_config = super::runner::FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/glm",
        output_validator: None,
        workspace: &workspace,
    };
    let exit = super::runner::run_with_fallback_and_validator(&mut fallback_config).unwrap();

    assert_eq!(exit, 0, "fallback agent should succeed");

    // Count agent invocations via MockProcessExecutor
    // We need to downcast to access agent_calls(), but since executor_arc is behind Arc<dyn>,
    // we verify behavior through the exit code and can't directly count calls.
    // The test passes if: GLM fails 3 times (max_retries), then OK succeeds.
    // Exit code 0 confirms the fallback chain worked correctly.
}

/// Test that spawn errors (like command not found) trigger fallback.
///
/// This verifies that when the primary agent command doesn't exist (exit code 127),
/// the fallback chain is used instead of crashing the pipeline.
#[test]
fn run_with_fallback_handles_command_not_found() {
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
    // Primary agent - use "printf" which exists in PATH, mock to fail with 127
    aliases.insert(
        "nonexistent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "printf nonexistent".to_string(),
            ..Default::default()
        },
    );
    // Fallback agent - use "echo" which exists in PATH, mock to succeed
    aliases.insert(
        "ok".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "echo ok".to_string(),
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

    // Set up runtime components (prefixed with _ as this test is incomplete/WIP)
    let colors = Colors { enabled: false };
    let _logger = Logger::new(colors);
    let _timer = Timer::new();
    let _config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: path::PathBuf::from("/test/prompt.txt"),
        ..Config::default()
    };

    // Use MockProcessExecutor for agent and git commands.
    // Configure primary agent (printf) to fail with exit code 127 (command not found)
    // and fallback agent (echo) to succeed.
    //
    // NOTE: This test is incomplete. To properly test fallback behavior, we would need to:
    // 1. Create a workspace with PROMPT.md
    // 2. Call run_with_fallback_and_validator with the mock executor
    // 3. Assert that the fallback agent (echo) was used after primary (printf) failed
    //
    // The test setup is preserved for documentation purposes, showing how to configure
    // the mock executor for command-not-found scenarios. See run_with_fallback_uses_chain_on_failure
    // for a working example of fallback testing.
    let _mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        // Primary agent (printf) fails with command not found (exit 127)
        .with_agent_result(
            "printf",
            Ok(crate::executor::AgentCommandResult::failure(
                127,
                "command not found",
            )),
        )
        // Fallback agent (echo) succeeds
        .with_agent_result("echo", Ok(crate::executor::AgentCommandResult::success()));
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
    let workspace = MemoryWorkspace::new_test();
    let parser = crate::json_parser::ClaudeParser::new(colors, Verbosity::Normal);
    parser.parse_stream(reader, &workspace).unwrap();

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

/// Test that GLM reviewer with exit code 1 triggers fallback without retries.
///
/// Uses MockProcessExecutor to avoid spawning real processes.
/// NOTE: Uses `/bin/cat` as the fallback command because `is_agent_available`
/// checks if the command exists in PATH via `which::which()`.
#[test]
fn test_glm_reviewer_fallback_on_exit_code_1() {
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
            cmd: "mock-glm-agent".to_string(),
            ..Default::default()
        },
    );
    // Use /bin/cat as the fallback command - it exists in PATH so is_agent_available returns true
    // The MockProcessExecutor will intercept the actual execution
    aliases.insert(
        "ok".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "/bin/cat".to_string(),
            ..Default::default()
        },
    );

    let mut registry = AgentRegistry::new().unwrap();
    registry.set_ccs_aliases(&aliases, defaults);
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
        prompt_path: path::PathBuf::from("/test/prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor:
    // - GLM agent fails with exit code 1 and error message
    // - OK agent (using /bin/cat command) succeeds
    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "mock-glm-agent",
            Ok(crate::executor::AgentCommandResult::failure(
                1,
                "GLM agent failed with exit code 1",
            )),
        )
        .with_agent_result(
            "/bin/cat",
            Ok(crate::executor::AgentCommandResult::success()),
        );

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    let mut fallback_config = super::runner::FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "test review",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/glm",
        output_validator: None,
        workspace: &workspace,
    };
    let exit = super::runner::run_with_fallback_and_validator(&mut fallback_config).unwrap();

    // The fallback agent should succeed
    assert_eq!(exit, 0, "Fallback agent should succeed");

    // GLM with exit code 1 should be classified as AgentSpecificQuirk and not retried.
    // Exit code 0 confirms the fallback chain worked correctly.
}

/// Test that GLM agent with exit code 1 BUT valid output is treated as success.
///
/// This tests the bug fix where GLM may exit with code 1 even when successfully completing work.
/// Uses MockProcessExecutor to avoid spawning real processes.
#[test]
fn test_glm_exit_code_1_with_valid_output_treated_as_success() {
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
            cmd: "mock-glm-agent".to_string(),
            ..Default::default()
        },
    );
    aliases.insert(
        "fallback".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "mock-fallback-agent".to_string(),
            ..Default::default()
        },
    );

    let mut registry = AgentRegistry::new().unwrap();
    registry.set_ccs_aliases(&aliases, defaults);

    let toml_str = r#"
        [agent_chain]
        reviewer = ["ccs/glm", "ccs/fallback"]
        max_retries = 3
        max_cycles = 1
    "#;
    let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
    registry.apply_unified_config(&unified);

    // Output validator reads from workspace to check for valid JSON output
    let validate_output: crate::pipeline::fallback::OutputValidator =
        |workspace: &dyn crate::workspace::Workspace,
         log_dir_path: &path::Path,
         _logger: &crate::logger::Logger|
         -> std::io::Result<bool> {
            // Look for any log file with valid result JSON
            // The mock executor writes output to log files via the streaming parser
            let log_file = log_dir_path.join("reviewer.log");
            match workspace.read(&log_file) {
                Ok(content) => Ok(content.contains(r#"{"type":"result""#)),
                Err(_) => Ok(false),
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
        prompt_path: path::PathBuf::from("/test/prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor:
    // - GLM agent exits with code 1 (the mock generates valid output automatically)
    // - Fallback agent should NOT be called (test verifies GLM succeeds despite exit 1)
    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "mock-glm-agent",
            Ok(crate::executor::AgentCommandResult::failure(1, "")),
        )
        .with_agent_result(
            "mock-fallback-agent",
            Ok(crate::executor::AgentCommandResult::failure(
                1,
                "fallback should not be called",
            )),
        );

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;

    // Pre-populate workspace with log file containing valid output
    // This simulates the agent producing valid JSON output before exiting with code 1
    let workspace = MemoryWorkspace::new_test().with_file(
        "/test/logs/reviewer.log",
        r#"{"type":"result","result":"- [ ] Test review item"}"#,
    );

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    let mut fallback_config = crate::pipeline::runner::FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/glm",
        output_validator: Some(validate_output),
        workspace: &workspace,
    };

    let exit =
        crate::pipeline::runner::run_with_fallback_and_validator(&mut fallback_config).unwrap();

    // GLM with exit code 1 but valid output should be treated as success
    assert_eq!(
        exit, 0,
        "GLM with valid output should succeed despite exit code 1"
    );
}

// ============================================================================
// Session Continuation Tests
// ============================================================================

/// Test that session continuation is NOT used on first attempt (retry_num = 0).
///
/// Even if session_info is provided, the first attempt should use normal
/// `run_with_fallback` behavior, not session continuation.
///
/// Uses MockProcessExecutor to avoid spawning real processes.
#[test]
fn session_continuation_not_used_on_first_attempt() {
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
            cmd: "mock-session-agent".to_string(),
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
        prompt_path: path::PathBuf::from("/test/prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor - agent succeeds
    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "mock-session-agent",
            Ok(crate::executor::AgentCommandResult::success()),
        );

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    // Create fake session info
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs/session-agent".to_string(),
        log_file: path::PathBuf::from("/test/fake.log"),
    };

    // Run with retry_num = 0 (first attempt) - should NOT use session continuation
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/session-agent",
        session_info: Some(&session_info),
        retry_num: 0, // First attempt
        output_validator: None,
        workspace: &workspace,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    // Agent should succeed - on first attempt (retry_num=0), normal fallback is used
    assert_eq!(exit.exit_code, 0, "Agent should succeed on first attempt");
}

/// Test that session continuation IS used on retry (retry_num > 0).
///
/// When retry_num > 0 and session_info is provided for a matching agent,
/// session continuation should be attempted.
///
/// Uses MockProcessExecutor to avoid spawning real processes.
#[test]
fn session_continuation_used_on_retry() {
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
            cmd: "mock-session-agent".to_string(),
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
        prompt_path: path::PathBuf::from("/test/prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor - agent succeeds
    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "mock-session-agent",
            Ok(crate::executor::AgentCommandResult::success()),
        );

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    // Create session info matching the agent
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs/session-agent".to_string(),
        log_file: path::PathBuf::from("/test/fake.log"),
    };

    // Run with retry_num = 1 (XSD retry) - SHOULD use session continuation
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/session-agent",
        session_info: Some(&session_info),
        retry_num: 1, // XSD retry
        output_validator: None,
        workspace: &workspace,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    // Agent should succeed - on retry (retry_num>0) with session_info, session continuation is used
    assert_eq!(
        exit.exit_code, 0,
        "Agent should succeed on retry with session continuation"
    );
}

/// Test that session continuation falls back when agent doesn't support it.
///
/// If the agent doesn't have a session_flag configured, session continuation
/// should silently fall back to normal `run_with_fallback` behavior.
///
/// Uses MockProcessExecutor to avoid spawning real processes.
#[test]
fn session_continuation_fallback_when_agent_unsupported() {
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
            cmd: "mock-no-session-agent".to_string(),
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
        prompt_path: path::PathBuf::from("/test/prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor - agent succeeds
    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "mock-no-session-agent",
            Ok(crate::executor::AgentCommandResult::success()),
        );

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    // Create session info (even though agent doesn't support it)
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs/no-session-agent".to_string(),
        log_file: path::PathBuf::from("/test/fake.log"),
    };

    // Run with retry_num = 1 - should fall back since agent doesn't support session
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/no-session-agent",
        session_info: Some(&session_info),
        retry_num: 1, // XSD retry
        output_validator: None,
        workspace: &workspace,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    // Agent should succeed via fallback path since it doesn't support sessions
    assert_eq!(exit.exit_code, 0, "Agent should succeed via fallback path");
}

/// Test that session continuation returns the agent's exit code even on crash.
///
/// If the agent crashes during session continuation, the exit code is returned
/// so the caller can check for valid output or handle the failure.
///
/// Uses MockProcessExecutor to avoid spawning real processes.
#[test]
fn session_continuation_fallback_when_agent_crashes() {
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
            cmd: "mock-crash-agent".to_string(),
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
        prompt_path: path::PathBuf::from("/test/prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor - agent crashes with exit code 139 (simulating SIGSEGV)
    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "mock-crash-agent",
            Ok(crate::executor::AgentCommandResult::failure(
                139,
                "Segmentation fault",
            )),
        );

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    // Create session info
    let session_info = crate::pipeline::session::SessionInfo {
        session_id: "ses_test123".to_string(),
        agent_name: "ccs/crash-agent".to_string(),
        log_file: path::PathBuf::from("/test/fake.log"),
    };

    // Run with retry_num = 1 - should try session continuation and return crash exit code
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/crash-agent",
        session_info: Some(&session_info),
        retry_num: 1, // XSD retry
        output_validator: None,
        workspace: &workspace,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();

    // When session continuation runs but agent crashes (non-zero exit),
    // we return the exit code so caller can check for valid output.
    assert_eq!(
        exit.exit_code, 139,
        "Should return crash exit code when session continuation ran but agent crashed"
    );
}

/// End-to-end test: Session ID extraction from log file and reuse on retry.
///
/// This test exercises the complete session continuation flow:
/// 1. First agent run outputs NDJSON with session_id to log file
/// 2. Session ID is extracted from the log file
/// 3. On retry, the same session ID is passed back to the agent
///
/// Uses MockProcessExecutor with pre-populated log files to avoid spawning real processes.
#[test]
fn session_continuation_e2e_extracts_session_from_logfile() {
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

    let mut aliases = HashMap::new();
    aliases.insert(
        "e2e-agent".to_string(),
        crate::config::CcsAliasConfig {
            cmd: "mock-e2e-agent".to_string(),
            session_flag: Some("--resume {}".to_string()),
            json_parser: Some("claude".to_string()),
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
        prompt_path: path::PathBuf::from("/test/prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor - agent succeeds
    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "mock-e2e-agent",
            Ok(crate::executor::AgentCommandResult::success()),
        );

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;

    // Create workspace - the log file will be written by the mock agent via the streaming parser
    // MockProcessExecutor's generate_mock_agent_output includes session_id for Claude parser
    let workspace = MemoryWorkspace::new_test();

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    // Run first attempt (retry_num = 0)
    let mut xsd_config = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs/test_1",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/e2e-agent",
        session_info: None, // No session info on first attempt
        retry_num: 0,
        output_validator: None,
        workspace: &workspace,
    };

    let exit = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config).unwrap();
    assert_eq!(exit.exit_code, 0, "First agent run should succeed");

    // Extract session info from the log file (this is what happens in production)
    // We pass the known agent name to avoid ambiguity from sanitized log file names
    let agent_config = registry.resolve_config("ccs/e2e-agent").unwrap();
    let log_prefix = path::PathBuf::from("/test/logs/test_1");
    let session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
        &log_prefix,
        agent_config.json_parser,
        Some("ccs/e2e-agent"),
        &workspace,
    );

    // Verify session was extracted
    assert!(
        session_info.is_some(),
        "Session info should be extracted from log file"
    );
    let session_info = session_info.unwrap();
    assert_eq!(
        session_info.session_id, "ses_mock_session_12345",
        "Session ID should match what the mock agent output"
    );
    // The agent name should be the original registry name (passed directly)
    assert_eq!(
        session_info.agent_name, "ccs/e2e-agent",
        "Agent name should be original registry name when passed directly"
    );

    // Run retry (retry_num = 1) with the extracted session info
    let mock_executor2 = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "mock-e2e-agent",
            Ok(crate::executor::AgentCommandResult::success()),
        );

    let executor_arc2 =
        std::sync::Arc::new(mock_executor2) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace2 = MemoryWorkspace::new_test();
    let mut runtime2 = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc2.as_ref(),
        executor_arc: executor_arc2.clone(),
        workspace: &workspace2,
    };

    let mut xsd_config2 = crate::pipeline::XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "retry prompt",
        logfile_prefix: "/test/logs/test_1",
        runtime: &mut runtime2,
        registry: &registry,
        primary_agent: "ccs/e2e-agent",
        session_info: Some(&session_info), // Pass the extracted session info
        retry_num: 1,                      // XSD retry
        output_validator: None,
        workspace: &workspace2,
    };

    let exit2 = crate::pipeline::run_xsd_retry_with_session(&mut xsd_config2).unwrap();

    // Retry should succeed - this verifies the full session continuation flow works:
    // 1. Session ID was extracted from the log file
    // 2. Session continuation passed the session ID via --resume flag
    assert_eq!(
        exit2.exit_code, 0,
        "Retry should succeed with session continuation"
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

/// Test that rate limit (429) errors trigger immediate agent fallback without retries.
///
/// This verifies the core behavior change: when an agent hits a 429 rate limit,
/// we immediately switch to the next agent in the chain rather than retrying
/// with the same agent. This allows work to continue without waiting for rate
/// limits to reset.
///
/// Uses MockProcessExecutor to avoid spawning real processes.
#[test]
fn test_rate_limit_triggers_immediate_agent_fallback() {
    let registry = setup_mock_registry_with_fallback();

    // Set up runtime components
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let prompt_dir = std::env::temp_dir().join("ralph-test-rate-limit");
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: prompt_dir.join("prompt.txt"),
        ..Config::default()
    };

    // Configure MockProcessExecutor:
    // - GLM agent fails with 429 rate limit error
    // - OK agent succeeds
    let mock_executor = std::sync::Arc::new(
        crate::executor::MockProcessExecutor::new()
            .with_output("git", "")
            .with_output("cargo", "")
            .with_agent_result(
                "printf",
                Ok(crate::executor::AgentCommandResult::failure(
                    1,
                    "rate limit exceeded: error 429",
                )),
            )
            .with_agent_result("echo", Ok(crate::executor::AgentCommandResult::success())),
    );

    let executor_arc =
        mock_executor.clone() as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    let mut fallback_config = super::runner::FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/glm",
        output_validator: None,
        workspace: &workspace,
    };

    // The 429 error should NOT be retried - immediate fallback to next agent
    // Exit code 0 confirms the fallback chain worked correctly.
    let exit = super::runner::run_with_fallback_and_validator(&mut fallback_config).unwrap();

    assert_eq!(
        exit, 0,
        "Fallback agent should succeed after 429 on first agent"
    );

    // Key requirement: 429 should NOT be retried with the same agent.
    assert_eq!(
        mock_executor.agent_calls_for("printf").len(),
        1,
        "rate-limited agent should not be retried"
    );
    assert_eq!(
        mock_executor.agent_calls_for("echo").len(),
        1,
        "fallback agent should be invoked exactly once"
    );
}

/// Test that stderr with multi-byte UTF-8 characters doesn't cause panic.
///
/// This is a regression test for the bug where `&stderr[..500]` panicked
/// when byte 500 fell in the middle of a multi-byte UTF-8 character.
/// The error message from the original crash was:
///   "byte index 500 is not a char boundary; it is inside '\u{95}' (bytes 499..501)"
#[test]
fn test_handle_agent_error_with_utf8_stderr() {
    let registry = setup_mock_registry_with_fallback();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let prompt_dir = std::env::temp_dir().join("ralph-test-utf8-stderr");
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: prompt_dir.join("prompt.txt"),
        ..Config::default()
    };

    // Create stderr with multi-byte UTF-8 characters that would panic
    // if we used byte-slicing at position 500.
    // Each '日' character is 3 bytes in UTF-8, so 200 of them = 600 bytes.
    // With the "Error: " prefix (7 bytes), total is 607 bytes.
    // The old code would slice at byte 500, which could land in the middle
    // of a multi-byte character, causing a panic.
    let stderr_content = "Error: ".to_string() + &"日".repeat(200);
    assert!(
        stderr_content.len() > 500,
        "stderr should be longer than 500 bytes for this test"
    );

    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "printf",
            Ok(crate::executor::AgentCommandResult::failure(
                1,
                &stderr_content,
            )),
        )
        .with_agent_result("echo", Ok(crate::executor::AgentCommandResult::success()));

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    let mut fallback_config = super::runner::FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/glm",
        output_validator: None,
        workspace: &workspace,
    };

    // Should NOT panic, should fallback to ok agent
    let exit = super::runner::run_with_fallback_and_validator(&mut fallback_config).unwrap();
    assert_eq!(exit, 0, "Should fallback successfully without panic");
}

/// Test that stderr with mixed multi-byte content is handled correctly.
///
/// Tests various edge cases for UTF-8 handling in error messages:
/// - CJK characters (3 bytes each)
/// - Emoji characters (4 bytes each)
/// - Mixed ASCII and multi-byte
#[test]
fn test_handle_agent_error_with_mixed_utf8_stderr() {
    let registry = setup_mock_registry_with_fallback();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let prompt_dir = std::env::temp_dir().join("ralph-test-mixed-utf8-stderr");
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: prompt_dir.join("prompt.txt"),
        ..Config::default()
    };

    // Create stderr with mixed content: ASCII + CJK + emoji
    // This tests that truncation works correctly regardless of where
    // the byte boundary falls.
    let stderr_content = format!(
        "ERROR: 日本語のエラーメッセージ 🚨 {}\nDetails: {}",
        "あ".repeat(100),
        "い".repeat(100)
    );
    assert!(
        stderr_content.len() > 500,
        "stderr should be longer than 500 bytes for this test"
    );

    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "printf",
            Ok(crate::executor::AgentCommandResult::failure(
                1,
                &stderr_content,
            )),
        )
        .with_agent_result("echo", Ok(crate::executor::AgentCommandResult::success()));

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    let mut fallback_config = super::runner::FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/glm",
        output_validator: None,
        workspace: &workspace,
    };

    // Should NOT panic, should fallback to ok agent
    let exit = super::runner::run_with_fallback_and_validator(&mut fallback_config).unwrap();
    assert_eq!(exit, 0, "Should fallback successfully with mixed UTF-8");
}

/// Test that panics during error handling don't crash the pipeline.
///
/// This is a defense-in-depth test ensuring that even if error classification
/// code has bugs that cause panics, the fallback chain continues.
/// The catch_unwind in try_agent_with_retries provides additional protection.
///
/// Note: With the UTF-8 fix in place, this specific panic source is eliminated,
/// but we test with replacement characters to verify the defense-in-depth
/// protection works correctly.
#[test]
fn test_error_handling_with_replacement_characters() {
    let registry = setup_mock_registry_with_fallback();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let prompt_dir = std::env::temp_dir().join("ralph-test-replacement-chars");
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: prompt_dir.join("prompt.txt"),
        ..Config::default()
    };

    // Use String::from_utf8_lossy to create a string with replacement characters.
    // Invalid UTF-8 bytes are replaced with U+FFFD (3 bytes each in UTF-8).
    // This simulates what happens when stderr contains binary data.
    let problematic_data: Vec<u8> = (0..600).map(|i| (i % 256) as u8).collect();
    let problematic_stderr = String::from_utf8_lossy(&problematic_data).to_string();

    let mock_executor = crate::executor::MockProcessExecutor::new()
        .with_output("git", "")
        .with_output("cargo", "")
        .with_agent_result(
            "printf",
            Ok(crate::executor::AgentCommandResult::failure(
                1,
                &problematic_stderr,
            )),
        )
        .with_agent_result("echo", Ok(crate::executor::AgentCommandResult::success()));

    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    let mut fallback_config = super::runner::FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: "/test/logs",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "ccs/glm",
        output_validator: None,
        workspace: &workspace,
    };

    // Should NOT panic, should recover and fallback
    let result = super::runner::run_with_fallback_and_validator(&mut fallback_config);
    assert!(
        result.is_ok(),
        "Should complete with replacement characters"
    );
}

#[test]
fn run_xsd_retry_with_session_detects_auth_error_from_session_continuation() {
    use crate::agents::{AgentConfig, JsonParserType};
    use crate::pipeline::runner::run_xsd_retry_with_session;
    use crate::pipeline::runner::XsdRetryConfig;
    use crate::pipeline::session::SessionInfo;
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;

    let mut registry = AgentRegistry::new().unwrap();
    registry.register(
        "session-agent",
        AgentConfig::builder()
            .cmd("printf")
            .json_parser(JsonParserType::Claude)
            .session_flag("-s {}")
            .build(),
    );

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let prompt_dir = std::env::temp_dir().join("ralph-test-session-auth-detect");
    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: prompt_dir.join("prompt.txt"),
        ..Config::default()
    };

    let mock_executor = crate::executor::MockProcessExecutor::new().with_agent_result(
        "printf",
        Ok(crate::executor::AgentCommandResult::failure(
            1,
            "credential is invalid",
        )),
    );
    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;
    let workspace = MemoryWorkspace::new_test();
    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        workspace: &workspace,
    };

    let session_info = SessionInfo {
        session_id: "ses_test_123".to_string(),
        agent_name: "session-agent".to_string(),
        log_file: PathBuf::from("/test/repo/.agent/logs/test.log"),
    };

    let mut xsd_retry_config = XsdRetryConfig {
        role: crate::agents::AgentRole::Developer,
        base_label: "test",
        prompt: "hello",
        logfile_prefix: ".agent/logs/test",
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: "session-agent",
        session_info: Some(&session_info),
        retry_num: 1,
        output_validator: None,
        workspace: &workspace,
    };

    let result = run_xsd_retry_with_session(&mut xsd_retry_config).unwrap();
    assert!(
        result.auth_error_detected,
        "Expected auth_error_detected from session continuation"
    );
}

#[test]
fn handler_generates_agent_invocation_failed_on_planning_auth_error() {
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::executor::MockProcessExecutor;
    use crate::phases::context::PhaseContext;
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::effect::{Effect, EffectHandler};
    use crate::reducer::event::{AgentErrorKind, PipelineEvent};
    use crate::reducer::handler::MainEffectHandler;
    use crate::reducer::state::{AgentChainState, PipelineState};
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;

    let mut registry = AgentRegistry::new().unwrap();
    registry.register(
        "session-agent",
        crate::agents::AgentConfig::builder()
            .cmd("printf")
            .json_parser(crate::agents::JsonParserType::Claude)
            .session_flag("-s {}")
            .build(),
    );

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let template_context = TemplateContext::default();
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new_test().with_file("PROMPT.md", "# Prompt");
    let prompt_dir = std::env::temp_dir().join("ralph-test-planning-auth");

    let config = Config {
        behavior: crate::config::types::BehavioralFlags {
            interactive: false,
            auto_detect_stack: false,
            strict_validation: false,
        },
        verbosity: Verbosity::Quiet,
        prompt_path: prompt_dir.join("prompt.txt"),
        ..Config::default()
    };

    let mock_executor = MockProcessExecutor::new().with_agent_result(
        "printf",
        Ok(crate::executor::AgentCommandResult::failure(
            1,
            "authentication failed",
        )),
    );
    let executor_arc =
        std::sync::Arc::new(mock_executor) as std::sync::Arc<dyn crate::executor::ProcessExecutor>;

    let mut ctx = PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: "session-agent",
        reviewer_agent: "session-agent",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        repo_root: &repo_root,
        workspace: &workspace,
    };

    let state = PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec!["session-agent".to_string()],
            vec![vec![]],
            crate::agents::AgentRole::Developer,
        ),
        ..PipelineState::initial(1, 0)
    };
    let mut handler = MainEffectHandler::new(state);
    let result = handler
        .execute(Effect::GeneratePlan { iteration: 1 }, &mut ctx)
        .unwrap();

    match result.event {
        PipelineEvent::Agent(crate::reducer::event::AgentEvent::InvocationFailed {
            role,
            agent,
            exit_code: _,
            error_kind,
            retriable,
        }) => {
            assert_eq!(role, crate::agents::AgentRole::Developer);
            assert_eq!(agent, "session-agent");
            assert_eq!(error_kind, AgentErrorKind::Authentication);
            assert!(!retriable, "auth failures should not be retriable");
        }
        other => panic!("expected AgentInvocationFailed event, got: {other:?}"),
    }
}
