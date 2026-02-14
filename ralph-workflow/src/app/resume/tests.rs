// Tests for resume functionality.
// This module contains unit tests for checkpoint validation and resume logic.

use crate::checkpoint::execution_history::{ExecutionHistory, ExecutionStep, StepOutcome};
use crate::checkpoint::state::{AgentConfigSnapshot, CheckpointParams, CliArgsSnapshot};
use crate::checkpoint::RebaseState;
use crate::logger::Colors;
use crate::workspace::MemoryWorkspace;

use std::sync::Arc;

#[test]
fn test_user_friendly_summary_uses_ascii_outcome_markers() {
    // Arrange: a checkpoint with execution history containing each outcome type.
    let mut history = ExecutionHistory::new();
    history.add_step_bounded(
        ExecutionStep::new(
            "Development",
            1,
            "dev_run",
            StepOutcome::success(None, vec![]),
        ),
        100,
    );
    history.add_step_bounded(
        ExecutionStep::new(
            "Review",
            1,
            "review",
            StepOutcome::failure("boom".to_string(), true),
        ),
        100,
    );
    history.add_step_bounded(
        ExecutionStep::new(
            "Development",
            1,
            "fix",
            StepOutcome::partial("did some".to_string(), "left some".to_string()),
        ),
        100,
    );
    history.add_step_bounded(
        ExecutionStep::new(
            "Commit",
            1,
            "commit",
            StepOutcome::skipped("already done".to_string()),
        ),
        100,
    );

    let mut checkpoint = crate::checkpoint::PipelineCheckpoint::from_params(CheckpointParams {
        phase: PipelinePhase::Development,
        iteration: 1,
        total_iterations: 1,
        reviewer_pass: 0,
        total_reviewer_passes: 0,
        developer_agent: "dev",
        reviewer_agent: "rev",
        cli_args: CliArgsSnapshot::new(1, 1, None, true, 2, false, None),
        developer_agent_config: AgentConfigSnapshot::new(
            "dev".to_string(),
            "dev-cmd".to_string(),
            "--output".to_string(),
            None,
            false,
        ),
        reviewer_agent_config: AgentConfigSnapshot::new(
            "rev".to_string(),
            "rev-cmd".to_string(),
            "--output".to_string(),
            None,
            false,
        ),
        rebase_state: RebaseState::NotStarted,
        git_user_name: None,
        git_user_email: None,
        run_id: "run-test",
        parent_run_id: None,
        resume_count: 0,
        actual_developer_runs: 0,
        actual_reviewer_runs: 0,
        working_dir: "/test/repo".to_string(),
        prompt_md_checksum: None,
        config_path: None,
        config_checksum: None,
    });
    checkpoint.execution_history = Some(history);

    let workspace = Arc::new(MemoryWorkspace::new_test());
    let logger = Logger::new(Colors::with_enabled(false))
        .with_workspace_log(workspace.clone(), ".agent/tmp/resume-summary.log");

    // Act
    display_user_friendly_checkpoint_summary(&checkpoint, &logger);

    // Assert: output uses ASCII markers (no Unicode glyphs).
    let log = workspace
        .read(Path::new(".agent/tmp/resume-summary.log"))
        .expect("expected log file to exist");

    // Contains the expected ASCII labels.
    assert!(log.contains("  OK"));
    assert!(log.contains("dev_run (Development)"));
    assert!(log.contains("  FAIL review (Review)"));
    assert!(log.contains("  PART fix (Development)"));
    assert!(log.contains("  SKIP commit (Commit)"));

    // Does not contain the previous Unicode glyphs.
    assert!(!log.contains('✓'));
    assert!(!log.contains('✗'));
    assert!(!log.contains('◐'));
    assert!(!log.contains('○'));
}
