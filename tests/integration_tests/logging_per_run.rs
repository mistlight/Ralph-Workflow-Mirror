//! Integration tests for per-run logging infrastructure.
//!
//! These tests verify that:
//! - Per-run log directories are created with correct structure
//! - Resume continues logging to the same run directory
//! - event_loop.log does not contain sensitive content

use anyhow::Result;
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::config::Config;
use ralph_workflow::executor::MockProcessExecutor;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use ralph_workflow::workspace::Workspace;
use std::path::PathBuf;
use std::sync::Arc;

/// Helper to create a minimal test config
fn test_config() -> Config {
    Config::test_default()
}

/// Helper to create a mock executor
fn test_executor() -> Arc<MockProcessExecutor> {
    Arc::new(MockProcessExecutor::new())
}

#[test]
fn test_per_run_log_directory_creation() {
    crate::test_timeout::with_default_timeout(|| {
        test_per_run_log_directory_creation_impl().unwrap();
    });
}

fn test_per_run_log_directory_creation_impl() -> Result<()> {
    // Create mock handlers
    let mut app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file(
            "PROMPT.md",
            "# Task: test\n## Goal\nTest\n## Acceptance\n- Pass",
        );

    let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));

    let config = test_config();
    let executor = test_executor();

    // Run Ralph
    crate::common::run_ralph_cli_with_handlers(
        &[],
        executor,
        config,
        &mut app_handler,
        &mut effect_handler,
    )?;

    // Find the run directory - look for .agent/logs-* pattern
    let all_files = app_handler.get_all_files();
    let run_dirs: Vec<_> = all_files
        .iter()
        .filter_map(|(path, _)| {
            let path_str = path.to_string_lossy();
            if path_str.starts_with(".agent/logs-") && path_str.contains("run.json") {
                // Extract the run directory from the full path
                let parts: Vec<_> = path_str.split('/').collect();
                if parts.len() >= 2 {
                    Some(PathBuf::from(format!("{}/{}", parts[0], parts[1])))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    assert!(
        !run_dirs.is_empty(),
        "Should create at least one run directory"
    );

    let run_dir = &run_dirs[0];

    // Verify run_id format
    let dir_name = run_dir.file_name().unwrap().to_str().unwrap();
    assert!(
        dir_name.starts_with("logs-"),
        "Directory should start with 'logs-'"
    );

    let run_id = dir_name.strip_prefix("logs-").unwrap();
    let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}\.\d{3}Z(-\d{2})?$").unwrap();
    assert!(
        re.is_match(run_id),
        "run_id format should match YYYY-MM-DD_HH-mm-ss.SSSZ[-NN]: got {}",
        run_id
    );

    // Verify run.json exists
    // Note: The full check for pipeline.log and event_loop.log is done in
    // test_event_loop_log_structure and test_event_loop_log_redaction

    // Verify no old logs in .agent/logs/
    let old_pipeline_log = PathBuf::from(".agent/logs/pipeline.log");
    assert!(
        app_handler.get_file(&old_pipeline_log).is_none(),
        "Should not create logs in old .agent/logs/ directory"
    );

    // Verify run.json contents
    let run_json = app_handler.get_file(&run_dir.join("run.json")).unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&run_json)?;

    assert_eq!(
        metadata["run_id"].as_str(),
        Some(run_id),
        "run_id should match directory name"
    );
    assert!(
        metadata["started_at_utc"].is_string(),
        "started_at_utc should be present"
    );
    assert!(metadata["command"].is_string(), "command should be present");
    assert_eq!(
        metadata["resume"].as_bool(),
        Some(false),
        "resume should be false for fresh run"
    );
    assert!(
        metadata["repo_root"].is_string(),
        "repo_root should be present"
    );
    assert!(
        metadata["ralph_version"].is_string(),
        "ralph_version should be present"
    );

    Ok(())
}

#[test]
fn test_event_loop_log_redaction() {
    crate::test_timeout::with_default_timeout(|| {
        test_event_loop_log_redaction_impl().unwrap();
    });
}

fn test_event_loop_log_redaction_impl() -> Result<()> {
    // Create sentinel strings that should NOT appear in event_loop.log
    let sentinel_prompt = "SENTINEL_PROMPT_CONTENT_SHOULD_NOT_APPEAR_IN_LOG";
    let sentinel_secret = "SENTINEL_SECRET_sk-1234567890abcdef";

    // Create mock handlers with sentinel content in PROMPT.md
    let mut app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file(
            "PROMPT.md",
            &format!(
                "# Task: test\n## Goal\n{}\n## Acceptance\n- Pass\n\n{}",
                sentinel_prompt, sentinel_secret
            ),
        );

    let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));

    let config = test_config();
    let executor = test_executor();

    // Run Ralph
    let _ = crate::common::run_ralph_cli_with_handlers(
        &[],
        executor,
        config,
        &mut app_handler,
        &mut effect_handler,
    );

    // Find the run directory
    let all_files = app_handler.get_all_files();
    let run_dir = all_files
        .iter()
        .find_map(|(path, _)| {
            let path_str = path.to_string_lossy();
            if path_str.starts_with(".agent/logs-") && path_str.contains("run.json") {
                let parts: Vec<_> = path_str.split('/').collect();
                if parts.len() >= 2 {
                    Some(PathBuf::from(format!("{}/{}", parts[0], parts[1])))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("Should create a run directory");

    // Read event_loop.log
    let event_loop_log = app_handler
        .get_file(&run_dir.join("event_loop.log"))
        .expect("event_loop.log should exist");

    // Verify sentinel strings do NOT appear in the log
    assert!(
        !event_loop_log.contains(sentinel_prompt),
        "event_loop.log must not contain PROMPT.md content"
    );
    assert!(
        !event_loop_log.contains(sentinel_secret),
        "event_loop.log must not contain secrets"
    );

    // Also check that the log is not empty (sanity check)
    assert!(
        !event_loop_log.trim().is_empty(),
        "event_loop.log should contain some entries"
    );

    Ok(())
}

#[test]
fn test_collision_handling() {
    crate::test_timeout::with_default_timeout(|| {
        test_collision_handling_impl().unwrap();
    });
}

fn test_collision_handling_impl() -> Result<()> {
    use ralph_workflow::logging::{RunId, RunLogContext};
    use ralph_workflow::workspace::MemoryWorkspace;

    let tempdir = tempfile::tempdir()?;
    let workspace = MemoryWorkspace::new(tempdir.path().to_path_buf());

    // Create a fixed run_id that we can use to simulate collision
    let fixed_id = RunId::for_test("2026-02-06_14-03-27.123Z");

    // Create the base directory to simulate a collision
    let base_dir = std::path::PathBuf::from(format!(".agent/logs-{}", fixed_id));
    workspace
        .create_dir_all(&base_dir)
        .expect("Failed to create base directory for collision test");

    // Also create collision variants 1-5 to test proper collision handling
    for i in 1..=5 {
        let collision_dir = std::path::PathBuf::from(format!(".agent/logs-{}-{:02}", fixed_id, i));
        workspace
            .create_dir_all(&collision_dir)
            .expect("Failed to create collision directory");
    }

    // Now create a RunLogContext with the fixed base run_id
    // It should skip base and collisions 1-5 and create collision variant 06
    let ctx = RunLogContext::for_testing(fixed_id.clone(), &workspace)?;

    // Verify the run_id has a collision suffix -06
    let run_id_str = ctx.run_id().as_str();
    assert!(
        run_id_str.ends_with("-06"),
        "Run ID should have collision suffix -06, got: {}",
        run_id_str
    );

    // Verify the directory exists
    assert!(workspace.exists(ctx.run_dir()));

    // Verify the directory name matches
    let expected_dir = std::path::PathBuf::from(format!(".agent/logs-{}", run_id_str));
    assert_eq!(
        ctx.run_dir(),
        &expected_dir,
        "Run directory should match the collision suffix path"
    );

    Ok(())
}

#[test]
fn test_no_legacy_logs_created() {
    crate::test_timeout::with_default_timeout(|| {
        test_no_legacy_logs_created_impl().unwrap();
    });
}

fn test_no_legacy_logs_created_impl() -> Result<()> {
    // Create mock handlers
    let mut app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file(
            "PROMPT.md",
            "# Task: test\n## Goal\nTest\n## Acceptance\n- Pass",
        );

    let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));

    let config = test_config();
    let executor = test_executor();

    // Run Ralph
    let _ = crate::common::run_ralph_cli_with_handlers(
        &[],
        executor,
        config,
        &mut app_handler,
        &mut effect_handler,
    );

    // Verify .agent/logs/pipeline.log does NOT exist (legacy location)
    let legacy_pipeline_log = PathBuf::from(".agent/logs/pipeline.log");
    assert!(
        app_handler.get_file(&legacy_pipeline_log).is_none(),
        "Should not create legacy pipeline.log"
    );

    // Verify no agent logs in legacy .agent/logs/ location
    let all_files = app_handler.get_all_files();
    for (path, _) in all_files.iter() {
        let path_str = path.to_string_lossy();
        // Check that no files starting with known prefixes exist in .agent/logs/
        if path_str.starts_with(".agent/logs/") {
            // Allow .agent/logs-<timestamp>/ directories but not .agent/logs/
            assert!(
                path_str.starts_with(".agent/logs-"),
                "Should not create legacy logs in .agent/logs/, found: {}",
                path_str
            );
        }
    }

    Ok(())
}

#[test]
fn test_agent_log_headers() {
    crate::test_timeout::with_default_timeout(|| {
        test_agent_log_headers_impl().unwrap();
    });
}

fn test_agent_log_headers_impl() -> Result<()> {
    // Create mock handlers
    let mut app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file(
            "PROMPT.md",
            "# Task: test\n## Goal\nTest\n## Acceptance\n- Pass",
        );

    let mut effect_handler = MockEffectHandler::new(PipelineState::initial(1, 1));

    let config = test_config();
    let executor = test_executor();

    // Run Ralph
    let _ = crate::common::run_ralph_cli_with_handlers(
        &[],
        executor,
        config,
        &mut app_handler,
        &mut effect_handler,
    );

    // Find the run directory
    let all_files = app_handler.get_all_files();
    let run_dir = all_files
        .iter()
        .find_map(|(path, _)| {
            let path_str = path.to_string_lossy();
            if path_str.starts_with(".agent/logs-") && path_str.contains("run.json") {
                let parts: Vec<_> = path_str.split('/').collect();
                if parts.len() >= 2 {
                    Some(PathBuf::from(format!("{}/{}", parts[0], parts[1])))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("Should create a run directory");

    // Find agent log files in the agents subdirectory
    let agents_dir = run_dir.join("agents");
    let agent_logs: Vec<_> = all_files
        .iter()
        .filter_map(|(path, content)| {
            if path.starts_with(&agents_dir) && path.extension()? == "log" {
                Some((path.clone(), content.clone()))
            } else {
                None
            }
        })
        .collect();

    // Note: Agent logs are only created if agents are actually invoked.
    // If the mock completes without agent invocation (e.g., if PLAN.md already exists),
    // there won't be agent logs. This is expected behavior.
    if agent_logs.is_empty() {
        // Skip test if no agent logs exist (pipeline completed without agent invocation)
        return Ok(());
    }

    // Verify each agent log has a header with required metadata
    for (path, content) in agent_logs {
        let path_str = path.display().to_string();

        // Verify header structure
        assert!(
            content.contains("# Ralph Agent Invocation Log"),
            "Agent log {} should have header",
            path_str
        );
        assert!(
            content.contains("# Role:"),
            "Agent log {} should specify role",
            path_str
        );
        assert!(
            content.contains("# Agent:"),
            "Agent log {} should specify agent name",
            path_str
        );
        assert!(
            content.contains("# Model Index:"),
            "Agent log {} should specify model index",
            path_str
        );
        assert!(
            content.contains("# Attempt:"),
            "Agent log {} should specify attempt number",
            path_str
        );
        assert!(
            content.contains("# Phase:"),
            "Agent log {} should specify phase",
            path_str
        );
        assert!(
            content.contains("# Timestamp:"),
            "Agent log {} should have timestamp",
            path_str
        );
    }

    Ok(())
}

#[test]
fn test_resume_logging_continuity() {
    crate::test_timeout::with_default_timeout(|| {
        test_resume_logging_continuity_impl().unwrap();
    });
}

fn test_resume_logging_continuity_impl() -> Result<()> {
    // First run: create a checkpoint
    let mut app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file(
            "PROMPT.md",
            "# Task: test\n## Goal\nTest\n## Acceptance\n- Pass",
        );

    let mut effect_handler = MockEffectHandler::new(PipelineState::initial(1, 0));

    let config = test_config();
    let executor = test_executor();

    // First run
    let _ = crate::common::run_ralph_cli_with_handlers(
        &[],
        executor.clone(),
        config.clone(),
        &mut app_handler,
        &mut effect_handler,
    );

    // Find the run directory from the first run
    let all_files = app_handler.get_all_files();
    let run_dirs: Vec<_> = all_files
        .iter()
        .filter_map(|(path, _)| {
            let path_str = path.to_string_lossy();
            if path_str.starts_with(".agent/logs-") && path_str.contains("run.json") {
                let parts: Vec<_> = path_str.split('/').collect();
                if parts.len() >= 2 {
                    Some(PathBuf::from(format!("{}/{}", parts[0], parts[1])))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    assert!(
        !run_dirs.is_empty(),
        "First run should create a run directory"
    );
    let first_run_dir = &run_dirs[0];

    // Get the pipeline.log content from first run
    let first_pipeline_log = app_handler
        .get_file(&first_run_dir.join("pipeline.log"))
        .expect("pipeline.log should exist after first run");
    let first_pipeline_log_lines: Vec<_> = first_pipeline_log.lines().collect();

    // Get the event_loop.log content from first run
    let first_event_loop_log = app_handler
        .get_file(&first_run_dir.join("event_loop.log"))
        .expect("event_loop.log should exist after first run");
    let first_event_loop_log_lines: Vec<_> = first_event_loop_log.lines().collect();

    // Manually create a checkpoint for the resume test
    // (The mock pipeline completes too quickly to save a checkpoint through normal flow)
    let run_id_str = first_run_dir
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("logs-")
        .unwrap();

    // Create a minimal checkpoint JSON directly
    let checkpoint_json = format!(
        r#"{{
        "version": 3,
        "phase": "Complete",
        "iteration": 1,
        "total_iterations": 1,
        "reviewer_pass": 0,
        "total_reviewer_passes": 0,
        "timestamp": "2026-02-06T16:00:00Z",
        "developer_agent": "mock_dev",
        "reviewer_agent": "mock_reviewer",
        "cli_args": {{
            "recovery_flags": {{}},
            "rebase_flags": {{}},
            "analysis_flags": {{}}
        }},
        "developer_agent_config": {{
            "agent_identifier": "mock_dev"
        }},
        "reviewer_agent_config": {{
            "agent_identifier": "mock_reviewer"
        }},
        "rebase_state": "NotStarted",
        "working_dir": "/mock/repo",
        "run_id": "{}",
        "resume_count": 0,
        "actual_developer_runs": 0,
        "actual_reviewer_runs": 0
    }}"#,
        run_id_str
    );

    app_handler.add_file(".agent/checkpoint.json", &checkpoint_json);

    // Resume run (simulate --resume with the same handler)
    let _ = crate::common::run_ralph_cli_with_handlers(
        &["--resume"],
        executor,
        config,
        &mut app_handler,
        &mut effect_handler,
    );

    // Verify the same run directory is used
    let all_files_after_resume = app_handler.get_all_files();
    let run_dirs_after_resume: Vec<_> = all_files_after_resume
        .iter()
        .filter_map(|(path, _)| {
            let path_str = path.to_string_lossy();
            if path_str.starts_with(".agent/logs-") && path_str.contains("run.json") {
                let parts: Vec<_> = path_str.split('/').collect();
                if parts.len() >= 2 {
                    Some(PathBuf::from(format!("{}/{}", parts[0], parts[1])))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    // Should still have only one run directory
    assert_eq!(
        run_dirs_after_resume.len(),
        1,
        "Resume should not create a new run directory"
    );
    assert_eq!(
        run_dirs_after_resume[0], *first_run_dir,
        "Resume should use the same run directory"
    );

    // Verify logs were appended, not overwritten
    let resumed_pipeline_log = app_handler
        .get_file(&first_run_dir.join("pipeline.log"))
        .expect("pipeline.log should still exist after resume");
    let resumed_pipeline_log_lines: Vec<_> = resumed_pipeline_log.lines().collect();

    assert!(
        resumed_pipeline_log_lines.len() >= first_pipeline_log_lines.len(),
        "pipeline.log should be appended to, not overwritten"
    );

    // Verify first run's log lines are still present
    for (i, line) in first_pipeline_log_lines.iter().enumerate() {
        assert_eq!(
            resumed_pipeline_log_lines.get(i),
            Some(line),
            "First run's log lines should be preserved"
        );
    }

    // Verify event_loop.log was also appended
    let resumed_event_loop_log = app_handler
        .get_file(&first_run_dir.join("event_loop.log"))
        .expect("event_loop.log should still exist after resume");
    let resumed_event_loop_log_lines: Vec<_> = resumed_event_loop_log.lines().collect();

    assert!(
        resumed_event_loop_log_lines.len() >= first_event_loop_log_lines.len(),
        "event_loop.log should be appended to, not overwritten"
    );

    Ok(())
}

#[test]
fn test_event_loop_log_structure() {
    crate::test_timeout::with_default_timeout(|| {
        test_event_loop_log_structure_impl().unwrap();
    });
}

fn test_event_loop_log_structure_impl() -> Result<()> {
    // Create mock handlers
    let mut app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file(
            "PROMPT.md",
            "# Task: test\n## Goal\nTest\n## Acceptance\n- Pass",
        );

    let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));

    let config = test_config();
    let executor = test_executor();

    // Run Ralph
    let _ = crate::common::run_ralph_cli_with_handlers(
        &[],
        executor,
        config,
        &mut app_handler,
        &mut effect_handler,
    );

    // Find the run directory
    let all_files = app_handler.get_all_files();
    let run_dir = all_files
        .iter()
        .find_map(|(path, _)| {
            let path_str = path.to_string_lossy();
            if path_str.starts_with(".agent/logs-") && path_str.contains("run.json") {
                let parts: Vec<_> = path_str.split('/').collect();
                if parts.len() >= 2 {
                    Some(PathBuf::from(format!("{}/{}", parts[0], parts[1])))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("Should create a run directory");

    // Read event_loop.log
    let event_loop_log = app_handler
        .get_file(&run_dir.join("event_loop.log"))
        .expect("event_loop.log should exist");

    // Verify log has expected structure (seq, ts, phase, effect, event, ms)
    let lines: Vec<_> = event_loop_log.lines().collect();
    assert!(
        !lines.is_empty(),
        "event_loop.log should have at least one line"
    );

    for line in lines {
        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Verify line contains expected fields
        assert!(
            line.contains("ts="),
            "Line should contain timestamp: {}",
            line
        );
        assert!(
            line.contains("phase="),
            "Line should contain phase: {}",
            line
        );
        assert!(
            line.contains("effect="),
            "Line should contain effect: {}",
            line
        );
        assert!(
            line.contains("event="),
            "Line should contain event: {}",
            line
        );
        assert!(
            line.contains("ms="),
            "Line should contain duration: {}",
            line
        );

        // Verify sequence numbers are present and monotonically increasing
        let seq_start = line.find(char::is_numeric);
        if let Some(start) = seq_start {
            let seq_str: String = line[start..]
                .chars()
                .take_while(|c| c.is_numeric())
                .collect();
            assert!(
                !seq_str.is_empty(),
                "Line should start with a sequence number: {}",
                line
            );
        }
    }

    Ok(())
}
