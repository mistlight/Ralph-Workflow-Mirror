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

#[cfg(unix)]
#[test]
fn run_with_fallback_does_not_retry_problematic_glm_reviewer() {
    let dir = tempfile::tempdir().unwrap();
    let fail_count = dir.path().join("fail_count.txt");
    let ok_count = dir.path().join("ok_count.txt");

    let fail_script = dir.path().join("fail.sh");
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

    let ok_script = dir.path().join("ok.sh");
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

    registry.set_fallback(crate::agents::fallback::FallbackConfig {
        reviewer: vec!["ccs/glm".to_string(), "ccs/ok".to_string()],
        max_retries: 3,
        max_cycles: 1,
        ..Default::default()
    });

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        interactive: false,
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
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
    assert_eq!(
        std::fs::read_to_string(&fail_count)
            .unwrap()
            .lines()
            .count(),
        1,
        "problematic agent should not be retried"
    );
    assert_eq!(
        std::fs::read_to_string(&ok_count).unwrap().lines().count(),
        1,
        "fallback agent should run once"
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

    let mut out = Vec::new();
    let colors = Colors { enabled: false };
    let parser = crate::json_parser::ClaudeParser::new(colors, Verbosity::Normal);
    parser.parse_stream(reader, &mut out).unwrap();

    let rendered = String::from_utf8(out).unwrap();
    assert!(rendered.contains("Hello from qwen"));
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
    if let Some(p_index) = parts.iter().position(|&s| s == "-p") {
        assert!(
            p_index > 0,
            "-p flag must come after command name. Command was: {cmd}"
        );
    } else {
        panic!("GLM command must contain -p flag. Command was: {cmd}");
    }
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
    registry.set_fallback(crate::agents::fallback::FallbackConfig {
        reviewer: vec!["ccs/glm".to_string(), "ccs/ok".to_string()],
        max_retries: 3, // Should not retry GLM
        max_cycles: 1,
        ..Default::default()
    });

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        interactive: false,
        verbosity: Verbosity::Quiet,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
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
