use super::*;

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
fn test_cloud_checkpoint_state_deserializes_legacy_cloud_config_field() {
    let legacy_json = serde_json::json!({
        "cloud_config": {
            "enabled": true,
            "api_url": "https://example.invalid",
            "run_id": "run-legacy-1",
            "heartbeat_interval_secs": 45,
            "graceful_degradation": false,
            "git_remote": {
                "auth_method": { "SshKey": { "key_path": null } },
                "push_branch": "feature/legacy",
                "create_pr": true,
                "pr_title_template": null,
                "pr_body_template": null,
                "pr_base_branch": "main",
                "force_push": false,
                "remote_name": "origin"
            }
        },
        "pending_push_commit": "abc123",
        "git_auth_configured": true,
        "pr_created": true,
        "pr_url": "https://example.invalid/pr/1",
        "pr_number": 1,
        "push_count": 2,
        "push_retry_count": 1,
        "last_push_error": null,
        "unpushed_commits": ["abc123"],
        "last_pushed_commit": "def456"
    });

    let deserialized: CloudCheckpointState = serde_json::from_value(legacy_json).unwrap();

    assert!(deserialized.cloud.enabled);
    assert_eq!(
        deserialized.cloud.api_url.as_deref(),
        Some("https://example.invalid")
    );
    assert_eq!(deserialized.pending_push_commit.as_deref(), Some("abc123"));
    assert!(deserialized.git_auth_configured);
    assert!(deserialized.pr_created);
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
