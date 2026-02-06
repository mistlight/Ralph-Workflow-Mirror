//! Resume integration tests.
//!
//! Tests verify that resume functionality works correctly with reducer state machine.
//! Tests cover resume at all pipeline phases (planning, development, review, commit).

use crate::test_timeout::with_default_timeout;
use ralph_workflow::checkpoint::state::{AgentConfigSnapshot, CliArgsSnapshot, RebaseState};
use ralph_workflow::checkpoint::{
    CheckpointBuilder, PipelineCheckpoint, PipelinePhase as CheckpointPhase,
};
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::state::PipelineState;

fn create_minimal_agent_config(name: &str) -> AgentConfigSnapshot {
    AgentConfigSnapshot {
        name: name.to_string(),
        cmd: name.to_string(),
        output_flag: "-o".to_string(),
        yolo_flag: None,
        can_commit: true,
        model_override: None,
        provider_override: None,
        context_level: 1,
    }
}

fn create_minimal_cli_args() -> CliArgsSnapshot {
    CliArgsSnapshot {
        developer_iters: 1,
        reviewer_reviews: 1,
        review_depth: None,
        isolation_mode: true,
        verbosity: 2,
        show_streaming_metrics: false,
        reviewer_json_parser: None,
    }
}

fn create_test_checkpoint(
    phase: CheckpointPhase,
    iteration: u32,
    total: u32,
    reviewer_pass: u32,
) -> PipelineCheckpoint {
    CheckpointBuilder::new()
        .phase(phase, iteration, total)
        .reviewer_pass(reviewer_pass, 2)
        .agents("claude", "claude")
        .cli_args(create_minimal_cli_args())
        .developer_config(create_minimal_agent_config("claude"))
        .reviewer_config(create_minimal_agent_config("claude"))
        .rebase_state(RebaseState::default())
        .git_identity(None, None)
        .build()
        .unwrap()
}

#[test]
fn test_pipeline_state_from_checkpoint_at_planning() {
    with_default_timeout(|| {
        let checkpoint = create_test_checkpoint(CheckpointPhase::Planning, 0, 5, 0);

        let state = PipelineState::from(checkpoint);

        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 0);
    });
}

#[test]
fn test_pipeline_state_from_checkpoint_at_development() {
    with_default_timeout(|| {
        let checkpoint = create_test_checkpoint(CheckpointPhase::Development, 3, 5, 0);

        let state = PipelineState::from(checkpoint);

        assert_eq!(state.phase, PipelinePhase::Development);
        assert_eq!(state.iteration, 3);
    });
}

#[test]
fn test_pipeline_state_from_checkpoint_at_review() {
    with_default_timeout(|| {
        let checkpoint = create_test_checkpoint(CheckpointPhase::Review, 5, 5, 1);

        let state = PipelineState::from(checkpoint);

        assert_eq!(state.phase, PipelinePhase::Review);
        assert_eq!(state.reviewer_pass, 1);
        assert_eq!(state.iteration, 5);
    });
}

#[test]
fn test_pipeline_state_from_checkpoint_at_commit() {
    with_default_timeout(|| {
        let checkpoint = create_test_checkpoint(CheckpointPhase::CommitMessage, 5, 5, 2);

        let state = PipelineState::from(checkpoint);

        assert_eq!(state.phase, PipelinePhase::CommitMessage);
        assert_eq!(state.reviewer_pass, 2);
        assert_eq!(state.iteration, 5);
    });
}

#[test]
fn test_pipeline_state_from_complete_checkpoint() {
    with_default_timeout(|| {
        let checkpoint = create_test_checkpoint(CheckpointPhase::Complete, 5, 5, 2);

        let state = PipelineState::from(checkpoint);

        assert_eq!(state.phase, PipelinePhase::Complete);
        assert_eq!(state.reviewer_pass, 2);
        assert_eq!(state.iteration, 5);
    });
}

#[test]
fn test_resume_continues_from_correct_iteration() {
    with_default_timeout(|| {
        let checkpoint = create_test_checkpoint(CheckpointPhase::Development, 2, 5, 0);

        let state = PipelineState::from(checkpoint);

        assert_eq!(state.iteration, 2);
        assert_eq!(state.total_iterations, 5);
    });
}

#[test]
fn test_resume_continues_from_correct_reviewer_pass() {
    with_default_timeout(|| {
        let checkpoint = create_test_checkpoint(CheckpointPhase::Review, 5, 5, 1);

        let state = PipelineState::from(checkpoint);

        assert_eq!(state.reviewer_pass, 1);
        assert_eq!(state.total_reviewer_passes, 2);
    });
}

#[test]
fn test_agent_chain_initialized_across_resume() {
    with_default_timeout(|| {
        let checkpoint = create_test_checkpoint(CheckpointPhase::Development, 2, 5, 0);

        let state = PipelineState::from(checkpoint);

        assert_eq!(state.agent_chain.current_agent_index, 0);
        assert_eq!(state.agent_chain.current_model_index, 0);
        assert_eq!(state.agent_chain.retry_cycle, 0);
    });
}

// ============================================================================
// Metrics Preservation Tests
// ============================================================================

#[test]
fn test_metrics_preserved_in_checkpoint_serialization() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::event::{DevelopmentEvent, PipelineEvent};
        use ralph_workflow::reducer::state_reduction::reduce;

        // Build state with non-zero metrics
        let mut state = PipelineState::initial(5, 2);
        state = reduce(state, PipelineEvent::development_iteration_started(0));
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration: 0 }),
        );

        // Serialize
        let json = serde_json::to_string(&state).unwrap();

        // Deserialize
        let restored: PipelineState = serde_json::from_str(&json).unwrap();

        // Verify metrics preserved
        assert_eq!(restored.metrics.dev_iterations_started, 1);
        assert_eq!(restored.metrics.dev_attempts_total, 1);
        assert_eq!(restored.metrics.analysis_attempts_total, 1);
        assert_eq!(restored.metrics.analysis_attempts_in_current_iteration, 1);
    });
}

#[test]
fn test_metrics_default_on_old_checkpoint_without_metrics() {
    with_default_timeout(|| {
        // Create a state, serialize it, remove the metrics field, and deserialize
        let state = PipelineState::initial(5, 2);
        let mut json: serde_json::Value = serde_json::to_value(&state).unwrap();

        // Remove the metrics field to simulate an old checkpoint
        json.as_object_mut().unwrap().remove("metrics");

        // Should deserialize with default metrics
        let restored: PipelineState = serde_json::from_value(json).unwrap();

        // Metrics should be defaulted (all zeros)
        assert_eq!(restored.metrics.dev_iterations_started, 0);
        assert_eq!(restored.metrics.dev_attempts_total, 0);
        assert_eq!(restored.metrics.max_dev_iterations, 0);
        assert_eq!(restored.metrics.max_review_passes, 0);
    });
}

#[test]
fn test_metrics_config_fields_preserved() {
    with_default_timeout(|| {
        let state = PipelineState::initial(10, 3);

        assert_eq!(state.metrics.max_dev_iterations, 10);
        assert_eq!(state.metrics.max_review_passes, 3);

        // Serialize and restore
        let json = serde_json::to_string(&state).unwrap();
        let restored: PipelineState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.metrics.max_dev_iterations, 10);
        assert_eq!(restored.metrics.max_review_passes, 3);
    });
}

// ============================================================================
// Step 16: Checkpoint resume test for metrics consistency
// ============================================================================

/// Test that metrics survive checkpoint serialization and resume with correct values.
///
/// CRITICAL: All metrics must be preserved across checkpoint/resume to ensure
/// the final summary is accurate even after interruption.
#[test]
fn test_metrics_survive_checkpoint_resume() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::RunMetrics;
        
        // Given: Create PipelineState with metrics partially populated
        let mut state = PipelineState::initial(5, 3);
        
        // Manually populate some metrics to simulate mid-run state
        state.metrics = RunMetrics {
            dev_iterations_started: 2,
            dev_iterations_completed: 1,
            dev_attempts_total: 3,
            dev_continuation_attempt: 1,
            analysis_attempts_total: 5,
            analysis_attempts_in_current_iteration: 2,
            review_passes_started: 1,
            review_passes_completed: 0,
            review_runs_total: 2,
            fix_runs_total: 1,
            fix_continuations_total: 0,
            fix_continuation_attempt: 0,
            current_review_pass: 1,
            xsd_retry_attempts_total: 3,
            xsd_retry_planning: 1,
            xsd_retry_development: 1,
            xsd_retry_review: 1,
            xsd_retry_fix: 0,
            xsd_retry_commit: 0,
            same_agent_retry_attempts_total: 1,
            agent_fallbacks_total: 2,
            model_fallbacks_total: 1,
            retry_cycles_started_total: 0,
            commits_created_total: 1,
            max_dev_iterations: state.metrics.max_dev_iterations,
            max_review_passes: state.metrics.max_review_passes,
            max_xsd_retry_count: state.metrics.max_xsd_retry_count,
            max_dev_continuation_count: state.metrics.max_dev_continuation_count,
            max_fix_continuation_count: state.metrics.max_fix_continuation_count,
            max_same_agent_retry_count: state.metrics.max_same_agent_retry_count,
        };
        
        // When: Serialize to JSON (simulating checkpoint write)
        let json = serde_json::to_string(&state).expect("Failed to serialize state");
        
        // When: Deserialize from JSON (simulating checkpoint resume)
        let restored: PipelineState = serde_json::from_str(&json).expect("Failed to deserialize state");
        
        // Then: All metrics should match original values (no drift, no reset to 0)
        assert_eq!(restored.metrics.dev_iterations_started, 2);
        assert_eq!(restored.metrics.dev_iterations_completed, 1);
        assert_eq!(restored.metrics.dev_attempts_total, 3);
        assert_eq!(restored.metrics.dev_continuation_attempt, 1);
        assert_eq!(restored.metrics.analysis_attempts_total, 5);
        assert_eq!(restored.metrics.analysis_attempts_in_current_iteration, 2);
        assert_eq!(restored.metrics.review_passes_started, 1);
        assert_eq!(restored.metrics.review_passes_completed, 0);
        assert_eq!(restored.metrics.review_runs_total, 2);
        assert_eq!(restored.metrics.fix_runs_total, 1);
        assert_eq!(restored.metrics.fix_continuations_total, 0);
        assert_eq!(restored.metrics.fix_continuation_attempt, 0);
        assert_eq!(restored.metrics.current_review_pass, 1);
        assert_eq!(restored.metrics.xsd_retry_attempts_total, 3);
        assert_eq!(restored.metrics.xsd_retry_planning, 1);
        assert_eq!(restored.metrics.xsd_retry_development, 1);
        assert_eq!(restored.metrics.xsd_retry_review, 1);
        assert_eq!(restored.metrics.xsd_retry_fix, 0);
        assert_eq!(restored.metrics.xsd_retry_commit, 0);
        assert_eq!(restored.metrics.same_agent_retry_attempts_total, 1);
        assert_eq!(restored.metrics.agent_fallbacks_total, 2);
        assert_eq!(restored.metrics.model_fallbacks_total, 1);
        assert_eq!(restored.metrics.retry_cycles_started_total, 0);
        assert_eq!(restored.metrics.commits_created_total, 1);
        
        // Verify config-derived display fields also survived
        assert_eq!(restored.metrics.max_dev_iterations, state.metrics.max_dev_iterations);
        assert_eq!(restored.metrics.max_review_passes, state.metrics.max_review_passes);
        assert_eq!(restored.metrics.max_xsd_retry_count, state.metrics.max_xsd_retry_count);
        assert_eq!(restored.metrics.max_dev_continuation_count, state.metrics.max_dev_continuation_count);
        assert_eq!(restored.metrics.max_fix_continuation_count, state.metrics.max_fix_continuation_count);
        assert_eq!(restored.metrics.max_same_agent_retry_count, state.metrics.max_same_agent_retry_count);
    });
}
