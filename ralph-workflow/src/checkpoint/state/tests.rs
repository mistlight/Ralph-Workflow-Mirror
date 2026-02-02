// Tests for checkpoint state module.
//
// This file contains all test code for checkpoint types and serialization.

// =========================================================================
// Workspace-based tests (for testability without real filesystem)
// =========================================================================

#[test]
fn test_environment_snapshot_filters_sensitive_vars() {
    std::env::set_var("RALPH_SAFE_SETTING", "ok");
    std::env::set_var("RALPH_API_TOKEN", "secret");
    std::env::set_var("EDITOR", "vim");

    let snapshot = EnvironmentSnapshot::capture_current();

    assert!(snapshot.ralph_vars.contains_key("RALPH_SAFE_SETTING"));
    assert!(!snapshot.ralph_vars.contains_key("RALPH_API_TOKEN"));
    assert!(snapshot.other_vars.contains_key("EDITOR"));

    std::env::remove_var("RALPH_SAFE_SETTING");
    std::env::remove_var("RALPH_API_TOKEN");
    std::env::remove_var("EDITOR");
}

#[cfg(feature = "test-utils")]
mod workspace_tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;
    use std::path::Path;

    /// Helper function to create a checkpoint for workspace tests.
    fn make_test_checkpoint_for_workspace(
        phase: PipelinePhase,
        iteration: u32,
    ) -> PipelineCheckpoint {
        let cli_args = CliArgsSnapshot::new(5, 2, None, true, 2, false, None);
        let dev_config =
            AgentConfigSnapshot::new("claude".into(), "cmd".into(), "-o".into(), None, true);
        let rev_config =
            AgentConfigSnapshot::new("codex".into(), "cmd".into(), "-o".into(), None, true);
        let run_id = uuid::Uuid::new_v4().to_string();
        PipelineCheckpoint::from_params(CheckpointParams {
            phase,
            iteration,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            developer_agent: "claude",
            reviewer_agent: "codex",
            cli_args,
            developer_agent_config: dev_config,
            reviewer_agent_config: rev_config,
            rebase_state: RebaseState::default(),
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: iteration,
            actual_reviewer_runs: 0,
            working_dir: "/test/repo".to_string(),
            prompt_md_checksum: None,
            config_path: None,
            config_checksum: None,
        })
    }

    #[test]
    fn test_calculate_file_checksum_with_workspace() {
        let workspace = MemoryWorkspace::new_test().with_file("test.txt", "test content");

        let checksum =
            calculate_file_checksum_with_workspace(&workspace, Path::new("test.txt"));
        assert!(checksum.is_some());

        // Same content should give same checksum
        let workspace2 = MemoryWorkspace::new_test().with_file("other.txt", "test content");
        let checksum2 =
            calculate_file_checksum_with_workspace(&workspace2, Path::new("other.txt"));
        assert_eq!(checksum, checksum2);
    }

    #[test]
    fn test_calculate_file_checksum_with_workspace_different_content() {
        let workspace1 = MemoryWorkspace::new_test().with_file("test.txt", "content A");
        let workspace2 = MemoryWorkspace::new_test().with_file("test.txt", "content B");

        let checksum1 =
            calculate_file_checksum_with_workspace(&workspace1, Path::new("test.txt"));
        let checksum2 =
            calculate_file_checksum_with_workspace(&workspace2, Path::new("test.txt"));

        assert!(checksum1.is_some());
        assert!(checksum2.is_some());
        assert_ne!(checksum1, checksum2);
    }

    #[test]
    fn test_calculate_file_checksum_with_workspace_nonexistent() {
        let workspace = MemoryWorkspace::new_test();

        let checksum =
            calculate_file_checksum_with_workspace(&workspace, Path::new("nonexistent.txt"));
        assert!(checksum.is_none());
    }

    #[test]
    fn test_save_checkpoint_with_workspace() {
        let workspace = MemoryWorkspace::new_test();
        let checkpoint = make_test_checkpoint_for_workspace(PipelinePhase::Development, 2);

        save_checkpoint_with_workspace(&workspace, &checkpoint).unwrap();

        assert!(workspace.exists(Path::new(".agent/checkpoint.json")));
    }

    #[test]
    fn test_checkpoint_exists_with_workspace() {
        let workspace = MemoryWorkspace::new_test();

        assert!(!checkpoint_exists_with_workspace(&workspace));

        let checkpoint = make_test_checkpoint_for_workspace(PipelinePhase::Development, 1);
        save_checkpoint_with_workspace(&workspace, &checkpoint).unwrap();

        assert!(checkpoint_exists_with_workspace(&workspace));
    }

    #[test]
    fn test_load_checkpoint_with_workspace_nonexistent() {
        let workspace = MemoryWorkspace::new_test();

        let result = load_checkpoint_with_workspace(&workspace).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_and_load_checkpoint_with_workspace() {
        let workspace = MemoryWorkspace::new_test();
        let checkpoint = make_test_checkpoint_for_workspace(PipelinePhase::Review, 5);

        save_checkpoint_with_workspace(&workspace, &checkpoint).unwrap();

        let loaded = load_checkpoint_with_workspace(&workspace)
            .unwrap()
            .expect("checkpoint should exist");

        assert_eq!(loaded.phase, PipelinePhase::Review);
        assert_eq!(loaded.iteration, 5);
        assert_eq!(loaded.developer_agent, "claude");
        assert_eq!(loaded.reviewer_agent, "codex");
    }

    #[test]
    fn test_clear_checkpoint_with_workspace() {
        let workspace = MemoryWorkspace::new_test();
        let checkpoint = make_test_checkpoint_for_workspace(PipelinePhase::Development, 1);

        save_checkpoint_with_workspace(&workspace, &checkpoint).unwrap();
        assert!(checkpoint_exists_with_workspace(&workspace));

        clear_checkpoint_with_workspace(&workspace).unwrap();
        assert!(!checkpoint_exists_with_workspace(&workspace));
    }

    #[test]
    fn test_clear_checkpoint_with_workspace_nonexistent() {
        let workspace = MemoryWorkspace::new_test();

        // Should not error when checkpoint doesn't exist
        clear_checkpoint_with_workspace(&workspace).unwrap();
    }

    #[test]
    fn test_load_checkpoint_rejects_v1_format() {
        // Test that loading a v1 checkpoint is rejected (legacy migration removed)
        let json = r#"{
            "version": 1,
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 1,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null
            },
            "developer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            },
            "reviewer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            },
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "/some/other/directory",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null
        }"#;

        let workspace = MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", json);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "v1 checkpoint should be rejected: {:?}",
            result
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("no longer supported"),
            "Error should mention legacy not supported: {}",
            err
        );
    }

    #[test]
    fn test_load_checkpoint_rejects_legacy_phase_variants() {
        let base_json = r#"{
            "version": 3,
            "phase": "%PHASE%",
            "iteration": 1,
            "total_iterations": 1,
            "reviewer_pass": 0,
            "total_reviewer_passes": 0,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 1,
                "reviewer_reviews": 0,
                "commit_msg": "",
                "review_depth": null
            },
            "developer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            },
            "reviewer_agent_config": {
                "name": "test-agent",
                "cmd": "echo",
                "output_flag": "",
                "yolo_flag": null,
                "can_commit": false,
                "model_override": null,
                "provider_override": null,
                "context_level": 1
            },
            "rebase_state": "NotStarted",
            "config_path": null,
            "config_checksum": null,
            "working_dir": "/some/other/directory",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null
        }"#;

        for phase_label in ["Fix", "ReviewAgain"] {
            let json = base_json.replace("%PHASE%", phase_label);
            let workspace =
                MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", &json);

            let result = load_checkpoint_with_workspace(&workspace);
            assert!(
                result.is_err(),
                "Legacy phase '{}' should be rejected",
                phase_label
            );
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("no longer supported"),
                "Error for '{}' should mention 'no longer supported': {}",
                phase_label,
                err
            );
        }
    }

    #[test]
    fn test_pipeline_phase_deserialize_rejects_legacy_variants() {
        let fix_result: Result<PipelinePhase, _> = serde_json::from_str("\"Fix\"");
        assert!(fix_result.is_err(), "Fix phase should be rejected");
        let err = fix_result.unwrap_err().to_string();
        assert!(
            err.contains("no longer supported"),
            "Error should mention 'no longer supported': {}",
            err
        );

        let review_again_result: Result<PipelinePhase, _> =
            serde_json::from_str("\"ReviewAgain\"");
        assert!(
            review_again_result.is_err(),
            "ReviewAgain phase should be rejected"
        );
        let err = review_again_result.unwrap_err().to_string();
        assert!(
            err.contains("no longer supported"),
            "Error should mention 'no longer supported': {}",
            err
        );
    }
}

// =========================================================================
// Test helper functions (real filesystem usage allowed per CLAUDE.md docs)
// =========================================================================

/// Helper function to create a checkpoint for testing.
fn make_test_checkpoint(phase: PipelinePhase, iteration: u32) -> PipelineCheckpoint {
    let cli_args = CliArgsSnapshot::new(5, 2, None, true, 2, false, None);
    let dev_config =
        AgentConfigSnapshot::new("claude".into(), "cmd".into(), "-o".into(), None, true);
    let rev_config =
        AgentConfigSnapshot::new("codex".into(), "cmd".into(), "-o".into(), None, true);
    let run_id = uuid::Uuid::new_v4().to_string();
    PipelineCheckpoint::from_params(CheckpointParams {
        phase,
        iteration,
        total_iterations: 5,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        developer_agent: "claude",
        reviewer_agent: "codex",
        cli_args,
        developer_agent_config: dev_config,
        reviewer_agent_config: rev_config,
        rebase_state: RebaseState::default(),
        git_user_name: None,
        git_user_email: None,
        run_id: &run_id,
        parent_run_id: None,
        resume_count: 0,
        actual_developer_runs: iteration,
        actual_reviewer_runs: 0,
        working_dir: "/test/repo".to_string(),
        prompt_md_checksum: None,
        config_path: None,
        config_checksum: None,
    })
}

#[test]
fn test_timestamp_format() {
    let ts = timestamp();
    assert!(ts.contains('-'));
    assert!(ts.contains(':'));
    assert_eq!(ts.len(), 19);
}

#[test]
fn test_pipeline_phase_display() {
    assert_eq!(format!("{}", PipelinePhase::Rebase), "Rebase");
    assert_eq!(format!("{}", PipelinePhase::Planning), "Planning");
    assert_eq!(format!("{}", PipelinePhase::Development), "Development");
    assert_eq!(format!("{}", PipelinePhase::Review), "Review");
    assert_eq!(
        format!("{}", PipelinePhase::CommitMessage),
        "Commit Message Generation"
    );
    assert_eq!(
        format!("{}", PipelinePhase::FinalValidation),
        "Final Validation"
    );
    assert_eq!(format!("{}", PipelinePhase::Complete), "Complete");
    assert_eq!(format!("{}", PipelinePhase::PreRebase), "Pre-Rebase");
    assert_eq!(
        format!("{}", PipelinePhase::PreRebaseConflict),
        "Pre-Rebase Conflict"
    );
    assert_eq!(format!("{}", PipelinePhase::PostRebase), "Post-Rebase");
    assert_eq!(
        format!("{}", PipelinePhase::PostRebaseConflict),
        "Post-Rebase Conflict"
    );
    assert_eq!(format!("{}", PipelinePhase::Interrupted), "Interrupted");
}

#[test]
fn test_checkpoint_from_params() {
    let cli_args = CliArgsSnapshot::new(5, 2, None, true, 2, false, None);
    let dev_config =
        AgentConfigSnapshot::new("claude".into(), "cmd".into(), "-o".into(), None, true);
    let rev_config =
        AgentConfigSnapshot::new("codex".into(), "cmd".into(), "-o".into(), None, true);
    let run_id = uuid::Uuid::new_v4().to_string();
    let checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
        phase: PipelinePhase::Development,
        iteration: 2,
        total_iterations: 5,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        developer_agent: "claude",
        reviewer_agent: "codex",
        cli_args,
        developer_agent_config: dev_config,
        reviewer_agent_config: rev_config,
        rebase_state: RebaseState::default(),
        git_user_name: None,
        git_user_email: None,
        run_id: &run_id,
        parent_run_id: None,
        resume_count: 0,
        actual_developer_runs: 2,
        actual_reviewer_runs: 0,
        working_dir: "/test/repo".to_string(),
        prompt_md_checksum: None,
        config_path: None,
        config_checksum: None,
    });

    assert_eq!(checkpoint.phase, PipelinePhase::Development);
    assert_eq!(checkpoint.iteration, 2);
    assert_eq!(checkpoint.total_iterations, 5);
    assert_eq!(checkpoint.reviewer_pass, 0);
    assert_eq!(checkpoint.total_reviewer_passes, 2);
    assert_eq!(checkpoint.developer_agent, "claude");
    assert_eq!(checkpoint.reviewer_agent, "codex");
    assert_eq!(checkpoint.version, 3);
    assert!(!checkpoint.timestamp.is_empty());
    assert_eq!(checkpoint.run_id, run_id);
    assert_eq!(checkpoint.resume_count, 0);
    assert_eq!(checkpoint.actual_developer_runs, 2);
    assert!(checkpoint.parent_run_id.is_none());
}

#[test]
fn test_checkpoint_description() {
    let checkpoint = make_test_checkpoint(PipelinePhase::Development, 3);
    assert_eq!(checkpoint.description(), "Development iteration 3/5");

    let run_id = uuid::Uuid::new_v4().to_string();
    let checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
        phase: PipelinePhase::Review,
        iteration: 5,
        total_iterations: 5,
        reviewer_pass: 0,
        total_reviewer_passes: 3,
        developer_agent: "claude",
        reviewer_agent: "codex",
        cli_args: CliArgsSnapshot::new(5, 3, None, true, 2, false, None),
        developer_agent_config: AgentConfigSnapshot::new(
            "claude".into(),
            "cmd".into(),
            "-o".into(),
            None,
            true,
        ),
        reviewer_agent_config: AgentConfigSnapshot::new(
            "codex".into(),
            "cmd".into(),
            "-o".into(),
            None,
            true,
        ),
        rebase_state: RebaseState::default(),
        git_user_name: None,
        git_user_email: None,
        run_id: &run_id,
        parent_run_id: None,
        resume_count: 0,
        actual_developer_runs: 5,
        actual_reviewer_runs: 0,
        working_dir: "/test/repo".to_string(),
        prompt_md_checksum: None,
        config_path: None,
        config_checksum: None,
    });
    assert_eq!(checkpoint.description(), "Initial review");

    let checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
        phase: PipelinePhase::Review,
        iteration: 5,
        total_iterations: 5,
        reviewer_pass: 2,
        total_reviewer_passes: 3,
        developer_agent: "claude",
        reviewer_agent: "codex",
        cli_args: CliArgsSnapshot::new(5, 3, None, true, 2, false, None),
        developer_agent_config: AgentConfigSnapshot::new(
            "claude".into(),
            "cmd".into(),
            "-o".into(),
            None,
            true,
        ),
        reviewer_agent_config: AgentConfigSnapshot::new(
            "codex".into(),
            "cmd".into(),
            "-o".into(),
            None,
            true,
        ),
        rebase_state: RebaseState::default(),
        git_user_name: None,
        git_user_email: None,
        run_id: &run_id,
        parent_run_id: None,
        resume_count: 0,
        actual_developer_runs: 5,
        actual_reviewer_runs: 2,
        working_dir: "/test/repo".to_string(),
        prompt_md_checksum: None,
        config_path: None,
        config_checksum: None,
    });
    assert_eq!(checkpoint.description(), "Verification review 2/3");
}

#[test]
fn test_checkpoint_serialization() {
    let run_id = uuid::Uuid::new_v4().to_string();
    let checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
        phase: PipelinePhase::Review,
        iteration: 3,
        total_iterations: 5,
        reviewer_pass: 1,
        total_reviewer_passes: 2,
        developer_agent: "aider",
        reviewer_agent: "opencode",
        cli_args: CliArgsSnapshot::new(5, 2, Some("standard".into()), false, 2, false, None),
        developer_agent_config: AgentConfigSnapshot::new(
            "aider".into(),
            "aider".into(),
            "-o".into(),
            Some("--yes".into()),
            true,
        ),
        reviewer_agent_config: AgentConfigSnapshot::new(
            "opencode".into(),
            "opencode".into(),
            "-o".into(),
            None,
            false,
        ),
        rebase_state: RebaseState::PreRebaseCompleted {
            commit_oid: "abc123".into(),
        },
        git_user_name: None,
        git_user_email: None,
        run_id: &run_id,
        parent_run_id: None,
        resume_count: 0,
        actual_developer_runs: 3,
        actual_reviewer_runs: 1,
        working_dir: "/test/repo".to_string(),
        prompt_md_checksum: None,
        config_path: None,
        config_checksum: None,
    });

    let json = serde_json::to_string(&checkpoint).unwrap();
    assert!(json.contains("Review"));
    assert!(json.contains("aider"));
    assert!(json.contains("opencode"));
    assert!(json.contains("\"version\":"));

    let deserialized: PipelineCheckpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.phase, checkpoint.phase);
    assert_eq!(deserialized.iteration, checkpoint.iteration);
    assert_eq!(deserialized.cli_args.developer_iters, 5);
    assert!(matches!(
        deserialized.rebase_state,
        RebaseState::PreRebaseCompleted { .. }
    ));
    assert_eq!(deserialized.run_id, run_id);
    assert_eq!(deserialized.actual_developer_runs, 3);
    assert_eq!(deserialized.actual_reviewer_runs, 1);
}

#[test]
fn test_cli_args_snapshot() {
    let snapshot = CliArgsSnapshot::new(
        10,
        3,
        Some("comprehensive".into()),
        true,
        3,
        true,
        Some("claude".to_string()),
    );

    assert_eq!(snapshot.developer_iters, 10);
    assert_eq!(snapshot.reviewer_reviews, 3);
    assert_eq!(snapshot.review_depth, Some("comprehensive".to_string()));
    assert!(snapshot.isolation_mode);
    assert_eq!(snapshot.verbosity, 3);
    assert!(snapshot.show_streaming_metrics);
    assert_eq!(snapshot.reviewer_json_parser, Some("claude".to_string()));
}

#[test]
fn test_agent_config_snapshot() {
    let config = AgentConfigSnapshot::new(
        "test-agent".into(),
        "/usr/bin/test".into(),
        "--output".into(),
        Some("--yolo".into()),
        false,
    );

    assert_eq!(config.name, "test-agent");
    assert_eq!(config.cmd, "/usr/bin/test");
    assert_eq!(config.output_flag, "--output");
    assert_eq!(config.yolo_flag, Some("--yolo".to_string()));
    assert!(!config.can_commit);
}

#[test]
fn test_rebase_state() {
    let state = RebaseState::PreRebaseInProgress {
        upstream_branch: "main".into(),
    };
    assert!(matches!(state, RebaseState::PreRebaseInProgress { .. }));

    let state = RebaseState::Failed {
        error: "conflict".into(),
    };
    assert!(matches!(state, RebaseState::Failed { .. }));
}
