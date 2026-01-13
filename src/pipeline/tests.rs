//! Tests for the agent pipeline runner.
//!
//! Includes contract tests for agent configurations (qwen, vibe, llama-cli)
//! and model flag resolution logic.

use super::*;
use crate::agents::{AgentRegistry, JsonParserType};
use crate::colors::Colors;
use crate::config::Config;
use crate::config::Verbosity;
use crate::output::argv_requests_json;
use crate::timer::Timer;
use crate::utils::split_command;
use crate::utils::Logger;
use std::collections::HashMap;

#[test]
fn resolve_model_with_provider_emits_full_model_flag() {
    // Provider override should preserve a full -m/--model flag rather than returning provider/model.
    assert_eq!(
        resolve_model_with_provider(
            Some("opencode"),
            Some("-m zai/glm-4.7"),
            Some("-m anthropic/claude-sonnet-4")
        )
        .as_deref(),
        Some("-m opencode/glm-4.7")
    );

    // Provider-only override should use the agent's configured model name.
    assert_eq!(
        resolve_model_with_provider(Some("opencode"), None, Some("-m anthropic/claude-sonnet-4"))
            .as_deref(),
        Some("-m opencode/claude-sonnet-4")
    );

    // Model-only overrides normalize bare provider/model to a full flag.
    assert_eq!(
        resolve_model_with_provider(None, Some("opencode/glm-4.7-free"), None).as_deref(),
        Some("-m opencode/glm-4.7-free")
    );

    // Preserve the user's style when provided.
    assert_eq!(
        resolve_model_with_provider(None, Some("--model=opencode/glm-4.7-free"), None).as_deref(),
        Some("--model=opencode/glm-4.7-free")
    );
}

#[test]
fn run_with_prompt_returns_command_result_for_missing_binary() {
    let dir = tempfile::tempdir().unwrap();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config {
        interactive: false,
        prompt_path: dir.path().join("prompt.txt"),
        ..Config::default()
    };

    let mut runtime = PipelineRuntime {
        timer: &mut timer,
        logger: &logger,
        colors: &colors,
        config: &config,
    };

    let result = run_with_prompt(
        PromptCommand {
            label: "test",
            display_name: "test",
            cmd_str: "definitely-not-a-real-binary-ralph",
            prompt: "hello",
            logfile: &dir.path().join("log.txt").display().to_string(),
            parser_type: JsonParserType::Generic,
        },
        &mut runtime,
    )
    .unwrap();

    assert_eq!(result.exit_code, 127);
    assert!(!result.stderr.is_empty());
}

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
    registry.set_ccs_aliases(aliases, defaults);

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
    let qwen = registry.get("qwen").unwrap();

    let cmd = qwen.build_cmd(true, true, true);
    let argv = split_command(&cmd).unwrap();

    let parser_type = qwen.json_parser;
    let uses_json = parser_type != JsonParserType::Generic || argv_requests_json(&argv);
    assert!(uses_json, "Qwen should run in JSON-parsing mode");
    assert_eq!(parser_type, JsonParserType::Claude);

    // Claude stream-json compatibility (used by qwen-code)
    let json =
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello from qwen"}]}}"#;
    let input = std::io::Cursor::new(format!("{}\n", json));
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
    let vibe = registry.get("vibe").unwrap();

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
    let llama = registry.get("llama-cli").unwrap();

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
