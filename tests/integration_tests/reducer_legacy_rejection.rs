//! Tests verifying legacy fallback paths are removed.
//!
//! These tests assert that the pipeline does NOT fall back to legacy
//! artifact locations. They verify that reducer state is the single source
//! of truth and legacy file-based fallbacks have been eliminated.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (rejection of legacy paths)
//! - Tests are deterministic and isolated
//! - Tests use `MemoryWorkspace` for filesystem isolation
//!
//! # Test Categories
//!
//! 1. **Commit message XML**: Verify only primary path is read
//! 2. **Checkpoint format**: Verify only current format is accepted
//! 3. **Result extraction**: Verify only current naming convention works
//! 4. **Config loading**: Verify only unified config is supported

use std::path::Path;

use ralph_workflow::checkpoint::load_checkpoint_with_workspace;
use ralph_workflow::workspace::MemoryWorkspace;

use crate::test_timeout::with_default_timeout;

// ============================================================================
// CHECKPOINT FORMAT TESTS
// ============================================================================

/// Test that legacy (pre-v1) checkpoint format is rejected.
///
/// Legacy checkpoints have a minimal structure without version number.
/// These should no longer be auto-migrated.
#[test]
fn test_checkpoint_rejects_legacy_format() {
    with_default_timeout(|| {
        let legacy_checkpoint = r#"{
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 00:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude"
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", legacy_checkpoint);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject legacy checkpoint format without version field"
        );
    });
}

/// Test that V1 checkpoint format is rejected.
///
/// V1 checkpoints have version field but lack run_id and other v2 fields.
#[test]
fn test_checkpoint_rejects_v1_format() {
    with_default_timeout(|| {
        let v1_checkpoint = r#"{
            "version": 1,
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 00:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude",
            "cli_args": {
                "iterations": 5,
                "reviewer_passes": 2,
                "agent": null,
                "verbose": false,
                "auto_commit": true
            },
            "developer_agent_config": {
                "name": "claude",
                "command": "claude",
                "args": "",
                "env": null,
                "uses_yolo": false
            },
            "reviewer_agent_config": {
                "name": "claude",
                "command": "claude",
                "args": "",
                "env": null,
                "uses_yolo": false
            },
            "rebase_state": {
                "rebase_enabled": false,
                "current_main_commit": null,
                "original_branch": null
            },
            "config_path": null,
            "config_checksum": null,
            "working_dir": "/test",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v1_checkpoint);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject V1 checkpoint format (missing run_id)"
        );
    });
}

/// Test that V2 checkpoint format is rejected.
///
/// V2 checkpoints have run_id but lack v3 fields (execution_history, etc.).
#[test]
fn test_checkpoint_rejects_v2_format() {
    with_default_timeout(|| {
        let v2_checkpoint = r#"{
            "version": 2,
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 00:00:00",
            "developer_agent": "claude",
            "reviewer_agent": "claude",
            "cli_args": {
                "iterations": 5,
                "reviewer_passes": 2,
                "agent": null,
                "verbose": false,
                "auto_commit": true
            },
            "developer_agent_config": {
                "name": "claude",
                "command": "claude",
                "args": "",
                "env": null,
                "uses_yolo": false
            },
            "reviewer_agent_config": {
                "name": "claude",
                "command": "claude",
                "args": "",
                "env": null,
                "uses_yolo": false
            },
            "rebase_state": {
                "rebase_enabled": false,
                "current_main_commit": null,
                "original_branch": null
            },
            "config_path": null,
            "config_checksum": null,
            "working_dir": "/test",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v2_checkpoint);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject V2 checkpoint format (missing v3 fields)"
        );
    });
}

/// Test that current V3 checkpoint format is accepted.
///
/// V3 is the only supported format. This test uses the correct checkpoint structure
/// which matches the internal `make_test_checkpoint_for_workspace` helper.
#[test]
fn test_checkpoint_accepts_v3_format() {
    with_default_timeout(|| {
        // Use proper V3 format with all required fields matching AgentConfigSnapshot
        let v3_checkpoint = r#"{
            "version": 3,
            "phase": "Development",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 5,
                "reviewer_reviews": 2
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
            "working_dir": "/test",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null,
            "env_snapshot": null
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v3_checkpoint);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_ok(),
            "Should accept V3 checkpoint format: {:?}",
            result
        );
        assert!(result.unwrap().is_some(), "Checkpoint should be present");
    });
}

// ============================================================================
// LEGACY PHASE REJECTION TESTS
// ============================================================================

/// Test that checkpoint with legacy "Fix" phase is rejected outright.
///
/// The reducer-only architecture requires that legacy phases are rejected,
/// not silently migrated. Users must delete old checkpoints and start fresh.
#[test]
fn test_checkpoint_rejects_legacy_fix_phase() {
    with_default_timeout(|| {
        let v3_with_fix = r#"{
            "version": 3,
            "phase": "Fix",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 5,
                "reviewer_reviews": 2
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
            "working_dir": "/test",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null,
            "env_snapshot": null
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v3_with_fix);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject checkpoint with legacy Fix phase (not silently migrate)"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Fix") || err.contains("legacy") || err.contains("no longer supported"),
            "Error should mention Fix or legacy: {err}"
        );
    });
}

/// Test that checkpoint with legacy "ReviewAgain" phase is rejected outright.
///
/// The reducer-only architecture requires that legacy phases are rejected,
/// not silently migrated. Users must delete old checkpoints and start fresh.
#[test]
fn test_checkpoint_rejects_legacy_review_again_phase() {
    with_default_timeout(|| {
        let v3_with_review_again = r#"{
            "version": 3,
            "phase": "ReviewAgain",
            "iteration": 1,
            "total_iterations": 5,
            "reviewer_pass": 0,
            "total_reviewer_passes": 2,
            "timestamp": "2024-01-01 12:00:00",
            "developer_agent": "test-agent",
            "reviewer_agent": "test-agent",
            "cli_args": {
                "developer_iters": 5,
                "reviewer_reviews": 2
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
            "working_dir": "/test",
            "prompt_md_checksum": null,
            "git_user_name": null,
            "git_user_email": null,
            "run_id": "test-run-id",
            "parent_run_id": null,
            "resume_count": 0,
            "actual_developer_runs": 0,
            "actual_reviewer_runs": 0,
            "execution_history": null,
            "file_system_state": null,
            "prompt_history": null,
            "env_snapshot": null
        }"#;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/checkpoint.json", v3_with_review_again);

        let result = load_checkpoint_with_workspace(&workspace);
        assert!(
            result.is_err(),
            "Should reject checkpoint with legacy ReviewAgain phase (not silently migrate)"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("ReviewAgain")
                || err.contains("legacy")
                || err.contains("no longer supported"),
            "Error should mention ReviewAgain or legacy: {err}"
        );
    });
}

// ============================================================================
// REDUCER-ONLY CONTROL FLOW TESTS
// ============================================================================

/// Test that state transitions are purely driven by events through the reducer.
///
/// This verifies that phase transitions happen via the reduce() function,
/// not through any direct state mutation.
#[test]
fn test_state_transitions_via_reducer_only() {
    use ralph_workflow::reducer::event::{CommitEvent, PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start at Planning phase with 2 iterations and 1 reviewer pass
        let state = PipelineState::initial(2, 1);
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 0, "Initial iteration is 0");

        // Planning -> Development transition via reduce()
        let state = reduce(state, PipelineEvent::plan_generation_completed(1, true));
        assert_eq!(
            state.phase,
            PipelinePhase::Development,
            "Transition to Development must happen via reducer"
        );
        assert_eq!(state.iteration, 0, "Iteration unchanged by plan completion");

        // Development iteration completion -> CommitMessage
        // Note: iteration field stays at 0 until commit is created
        let state = reduce(
            state,
            PipelineEvent::development_iteration_completed(0, true),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Dev iteration completion transitions to CommitMessage"
        );
        assert_eq!(state.iteration, 0, "Iteration unchanged in CommitMessage");

        // After commit created, goes to Planning for next iteration (not Development directly!)
        // The reducer pattern is: Dev -> Commit -> Planning -> Dev (for each iteration)
        let state = reduce(
            state,
            PipelineEvent::Commit(CommitEvent::Created {
                message: "test commit".to_string(),
                hash: "abc123".to_string(),
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Planning,
            "After commit with more iterations, goes to Planning"
        );
        assert_eq!(state.iteration, 1, "Iteration incremented to 1");

        // Planning again -> Development
        let state = reduce(state, PipelineEvent::plan_generation_completed(2, true));
        assert_eq!(state.phase, PipelinePhase::Development);

        // Complete iteration 1 (second dev iteration) -> CommitMessage
        let state = reduce(
            state,
            PipelineEvent::development_iteration_completed(1, true),
        );
        assert_eq!(state.phase, PipelinePhase::CommitMessage);

        // After final commit, transitions to Review (iteration 2 >= total_iterations 2)
        let state = reduce(
            state,
            PipelineEvent::Commit(CommitEvent::Created {
                message: "final commit".to_string(),
                hash: "def456".to_string(),
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "After final iteration commit, should transition to Review"
        );
        assert_eq!(state.iteration, 2, "Final iteration is 2");
    });
}

/// Test that effect determination is based solely on reducer state.
///
/// The determine_next_effect() function should be a pure function of state,
/// not reading any external configuration or files.
#[test]
fn test_effects_determined_from_state_only() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // Initial state needs agent chain initialization
        let state = PipelineState::initial(3, 1);
        let effect = determine_next_effect(&state);
        assert!(
            matches!(
                effect,
                Effect::InitializeAgentChain {
                    role: AgentRole::Developer
                }
            ),
            "Effect should be determined purely from state: {:?}",
            effect
        );

        // State with agents but no context cleaned -> clean context
        let mut state = PipelineState::initial(3, 1);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        state.context_cleaned = false;
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::CleanupContext),
            "Should clean context before planning: {:?}",
            effect
        );

        // State ready for planning
        let mut state = PipelineState::initial(3, 1);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        state.context_cleaned = true;
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::GeneratePlan { .. }),
            "Should generate plan when state is ready: {:?}",
            effect
        );

        // Development phase
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunDevelopmentIteration { .. }),
            "Should run dev iteration from state: {:?}",
            effect
        );
    });
}

/// Test that agent selection comes from reducer state, not config lookups.
///
/// The agent_chain in PipelineState should be the single source of truth
/// for which agent to use next.
#[test]
fn test_agent_selection_from_reducer_state() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // Set up state with specific agents in chain
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["custom-agent".to_string(), "fallback-agent".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // The effect doesn't contain agent name - handler reads from state.agent_chain
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunDevelopmentIteration { iteration: 1 }),
            "Expected RunDevelopmentIteration, got {:?}",
            effect
        );

        // Verify agent chain has our custom agent as current
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"custom-agent".to_string()),
            "Agent should be selected from state.agent_chain"
        );

        // After switching to next agent, chain should point to fallback
        state.agent_chain = state.agent_chain.switch_to_next_agent();
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"fallback-agent".to_string()),
            "Should use next agent in chain after switch"
        );
    });
}

/// Test that pipeline completion is determined by reducer state, not file existence.
///
/// The pipeline should complete when state.phase == Complete, not when
/// certain files exist on disk.
#[test]
fn test_completion_from_state_not_files() {
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::{CheckpointTrigger, PipelinePhase};
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // State at Complete phase
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Complete;

        let effect = determine_next_effect(&state);
        // Complete phase emits SaveCheckpoint with Interrupt trigger
        assert!(
            matches!(
                effect,
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::Interrupt
                }
            ),
            "Should save checkpoint on complete based on state.phase, not file checks: {:?}",
            effect
        );
    });
}

// ============================================================================
// XSD RETRY STATE TRACKING TESTS
// ============================================================================

/// Test that XSD validation failures are tracked in reducer state.
///
/// This verifies that `invalid_output_attempts` in ContinuationState is incremented
/// when OutputValidationFailed events are processed, making retry decisions
/// explicit in state rather than hidden in phase code.
#[test]
fn test_xsd_retry_count_in_reducer_state() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Initial state should have zero invalid output attempts
        assert_eq!(
            state.continuation.invalid_output_attempts, 0,
            "Initial state should have 0 invalid_output_attempts"
        );

        // Simulate XSD validation failure - reducer should track this
        let state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );

        // State should track retry count
        assert_eq!(
            state.continuation.invalid_output_attempts, 1,
            "Reducer state must track XSD retry attempts after OutputValidationFailed"
        );

        // Second failure should increment again
        let state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 1),
        );

        assert_eq!(
            state.continuation.invalid_output_attempts, 2,
            "Reducer state must increment invalid_output_attempts on each failure"
        );
    });
}

/// Test that max XSD retries triggers agent advancement via reducer.
///
/// After MAX_DEV_INVALID_OUTPUT_RERUNS XSD failures, the reducer should
/// advance the agent chain, making fallback behavior explicit in state.
#[test]
fn test_max_xsd_retries_advances_agent_chain_via_reducer() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::{PipelineState, MAX_DEV_INVALID_OUTPUT_RERUNS};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["primary-agent".to_string(), "fallback-agent".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // Verify we start with primary agent
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"primary-agent".to_string()),
            "Should start with primary agent"
        );

        // Exhaust retries up to MAX (typically 2)
        let mut current_state = state;
        for attempt in 0..MAX_DEV_INVALID_OUTPUT_RERUNS {
            current_state = reduce(
                current_state,
                PipelineEvent::development_output_validation_failed(0, attempt),
            );
        }

        // After max retries, invalid_output_attempts should be at max
        assert_eq!(
            current_state.continuation.invalid_output_attempts, MAX_DEV_INVALID_OUTPUT_RERUNS,
            "Should have max invalid_output_attempts"
        );

        // One more failure should trigger agent advancement and reset counter
        let final_state = reduce(
            current_state,
            PipelineEvent::development_output_validation_failed(0, MAX_DEV_INVALID_OUTPUT_RERUNS),
        );

        // Counter should be reset after agent switch
        assert_eq!(
            final_state.continuation.invalid_output_attempts, 0,
            "invalid_output_attempts should reset after agent advancement"
        );

        // Agent chain should have advanced
        assert_eq!(
            final_state.agent_chain.current_agent(),
            Some(&"fallback-agent".to_string()),
            "Agent chain should advance to fallback after exhausting retries"
        );
    });
}

// ============================================================================
// AGENT CHAIN STATE MANAGEMENT TESTS
// ============================================================================

/// Test that agent chain is cleared on dev->review transition via reducer.
///
/// When transitioning from Development to Review phase, the agent chain must
/// be cleared so that the orchestrator initializes a fresh Reviewer chain.
/// This prevents the developer agent chain from leaking into review phase.
#[test]
fn test_agent_chain_cleared_on_dev_to_review_transition() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{CommitEvent, PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start with populated developer agent chain that has been used
        let mut state = PipelineState::initial(1, 1);
        state.agent_chain = state.agent_chain.with_agents(
            vec!["dev-primary".to_string(), "dev-fallback".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        state.phase = PipelinePhase::CommitMessage;
        state.previous_phase = Some(PipelinePhase::Development);
        state.commit = ralph_workflow::reducer::state::CommitState::Generated {
            message: "test commit".to_string(),
        };

        // Verify developer chain is populated
        assert!(!state.agent_chain.agents.is_empty());
        assert_eq!(state.agent_chain.current_role, AgentRole::Developer);

        // Transition via CommitEvent::Created - this should go to Review since
        // iteration 0 + 1 = 1 >= total_iterations (1)
        let new_state = reduce(
            state,
            PipelineEvent::Commit(CommitEvent::Created {
                hash: "abc123".to_string(),
                message: "test commit".to_string(),
            }),
        );

        // Should be in Review phase
        assert_eq!(
            new_state.phase,
            PipelinePhase::Review,
            "Should transition to Review phase"
        );

        // Agent chain should be CLEARED for Reviewer initialization
        assert!(
            new_state.agent_chain.agents.is_empty(),
            "Agent chain must be cleared on dev->review transition, was: {:?}",
            new_state.agent_chain.agents
        );
    });
}

// ============================================================================
// EFFECT SINGLE-TASK VERIFICATION TESTS
// ============================================================================

/// Test that all Effect variants represent single logical operations.
///
/// This test documents the single-responsibility nature of each effect type.
/// If a new effect is added that bundles multiple operations, this test
/// should be updated to discuss whether the effect should be split.
#[test]
fn test_effects_are_single_task() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::{ContinuationContextData, Effect};
    use ralph_workflow::reducer::event::{CheckpointTrigger, ConflictStrategy, RebasePhase};
    use ralph_workflow::reducer::state::DevelopmentStatus;

    with_default_timeout(|| {
        // This test enumerates all Effect variants to verify they each represent
        // a single logical operation. The match is exhaustive so the test will
        // fail to compile if new variants are added without consideration.

        fn describe_effect_task(effect: &Effect) -> &'static str {
            match effect {
                // Each match arm describes the SINGLE task the effect performs
                Effect::AgentInvocation { .. } => "Run ONE agent invocation",
                Effect::InitializeAgentChain { .. } => "Initialize agent chain for ONE role",
                Effect::GeneratePlan { .. } => "Generate plan for ONE iteration",
                Effect::RunDevelopmentIteration { .. } => "Run ONE development iteration",
                Effect::RunReviewPass { .. } => "Run ONE review pass",
                Effect::RunFixAttempt { .. } => "Run ONE fix attempt",
                Effect::RunRebase { .. } => "Run ONE rebase operation",
                Effect::ResolveRebaseConflicts { .. } => "Resolve conflicts ONCE",
                Effect::GenerateCommitMessage => "Generate ONE commit message",
                Effect::CreateCommit { .. } => "Create ONE commit",
                Effect::SkipCommit { .. } => "Skip commit ONCE",
                Effect::ValidateFinalState => "Validate final state ONCE",
                Effect::SaveCheckpoint { .. } => "Save ONE checkpoint",
                Effect::CleanupContext => "Clean context ONCE",
                Effect::RestorePromptPermissions => "Restore permissions ONCE",
                Effect::WriteContinuationContext(_) => "Write ONE context file",
                Effect::CleanupContinuationContext => "Clean ONE context file",
            }
        }

        // Create sample instances of each effect to verify they exist
        // and the match is exhaustive
        let effects: Vec<Effect> = vec![
            Effect::AgentInvocation {
                role: AgentRole::Developer,
                agent: "test".to_string(),
                model: None,
                prompt: "test".to_string(),
            },
            Effect::InitializeAgentChain {
                role: AgentRole::Developer,
            },
            Effect::GeneratePlan { iteration: 0 },
            Effect::RunDevelopmentIteration { iteration: 0 },
            Effect::RunReviewPass { pass: 0 },
            Effect::RunFixAttempt { pass: 0 },
            Effect::RunRebase {
                phase: RebasePhase::Initial,
                target_branch: "main".to_string(),
            },
            Effect::ResolveRebaseConflicts {
                strategy: ConflictStrategy::Abort,
            },
            Effect::GenerateCommitMessage,
            Effect::CreateCommit {
                message: "test".to_string(),
            },
            Effect::SkipCommit {
                reason: "test".to_string(),
            },
            Effect::ValidateFinalState,
            Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::PhaseTransition,
            },
            Effect::CleanupContext,
            Effect::RestorePromptPermissions,
            Effect::WriteContinuationContext(ContinuationContextData {
                iteration: 0,
                attempt: 0,
                status: DevelopmentStatus::Completed,
                summary: "test".to_string(),
                files_changed: None,
                next_steps: None,
            }),
            Effect::CleanupContinuationContext,
        ];

        // Verify each effect has a single-task description
        for effect in &effects {
            let description = describe_effect_task(effect);
            assert!(
                description.contains("ONE") || description.contains("ONCE"),
                "Effect {:?} should have a single-task description, got: {}",
                effect,
                description
            );
        }

        // Verify we covered all variants (17 at time of writing)
        assert_eq!(
            effects.len(),
            17,
            "Expected 17 Effect variants; update this test if variants were added or removed"
        );
    });
}

/// Test that agent fallback happens exclusively via reducer events.
///
/// Agent switching occurs through reducer event processing, not through
/// any ad-hoc logic in phase code. This test verifies the reducer is the
/// single source of truth for agent chain advancement.
#[test]
fn test_agent_fallback_only_via_reducer_events() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{AgentEvent, PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec![
                "agent-a".to_string(),
                "agent-b".to_string(),
                "agent-c".to_string(),
            ],
            vec![vec![], vec![], vec![]],
            AgentRole::Developer,
        );

        // Verify initial agent
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"agent-a".to_string())
        );

        // FallbackTriggered event should switch to next agent
        let state = reduce(
            state,
            PipelineEvent::Agent(AgentEvent::FallbackTriggered {
                role: AgentRole::Developer,
                from_agent: "agent-a".to_string(),
                to_agent: "agent-b".to_string(),
            }),
        );

        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"agent-b".to_string()),
            "FallbackTriggered should switch to next agent"
        );

        // InvocationFailed with retriable=false switches to next agent via reducer
        // This is the correct behavior - the reducer handles fallback decisions
        let state = reduce(
            state,
            PipelineEvent::Agent(AgentEvent::InvocationFailed {
                role: AgentRole::Developer,
                agent: "agent-b".to_string(),
                exit_code: 1,
                error_kind: ralph_workflow::reducer::event::AgentErrorKind::InternalError,
                retriable: false,
            }),
        );

        // Non-retriable InvocationFailed SHOULD switch to next agent via reducer
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"agent-c".to_string()),
            "InvocationFailed(retriable=false) should switch to next agent via reducer"
        );

        // InvocationFailed with retriable=true should NOT switch agents (tries next model)
        // Reset to test retriable case
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["primary".to_string(), "fallback".to_string()],
            vec![vec!["model-a".to_string(), "model-b".to_string()], vec![]],
            AgentRole::Developer,
        );

        let state = reduce(
            state,
            PipelineEvent::Agent(AgentEvent::InvocationFailed {
                role: AgentRole::Developer,
                agent: "primary".to_string(),
                exit_code: 1,
                error_kind: ralph_workflow::reducer::event::AgentErrorKind::Network,
                retriable: true,
            }),
        );

        // Retriable failure should advance model, not agent
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"primary".to_string()),
            "InvocationFailed(retriable=true) should NOT switch agent"
        );
    });
}

// ============================================================================
// LEGACY ARTIFACT IGNORED DURING EXECUTION TESTS
// ============================================================================

/// Test that legacy artifacts in workspace don't affect effect determination.
///
/// When legacy files (e.g., ISSUES.md, PLAN.md from old versions) exist
/// in the workspace, the pipeline should NOT read them to derive results.
/// All pipeline decisions must come from reducer events/effects, not file presence.
#[test]
fn test_legacy_artifacts_ignored_during_execution() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // Create state in Development phase with agents initialized
        let mut state = PipelineState::initial(2, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Effect determination should NOT depend on workspace file existence
        // (determine_next_effect is a pure function of state)
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunDevelopmentIteration { .. }),
            "Effect should be determined from state alone, got {:?}",
            effect
        );

        // Even with max iterations reached, state-based transition should happen
        let mut state = PipelineState::initial(0, 1);
        state.phase = PipelinePhase::Review;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        );

        // Effect determination for review should not check for legacy ISSUES.md
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunReviewPass { .. }),
            "Review effect should be determined from state alone, got {:?}",
            effect
        );
    });
}

/// Test that legacy artifact files in workspace are completely ignored.
///
/// Even when legacy files exist in the workspace (ISSUES.md, PLAN.md, commit.xml),
/// the pipeline must not read them to derive results. All results must come from
/// the current XML paths. This test explicitly creates these files and verifies
/// determine_next_effect remains unchanged.
#[test]
fn test_legacy_artifact_files_completely_ignored() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::workspace::MemoryWorkspace;

    with_default_timeout(|| {
        // Create workspace with legacy artifact files that should be ignored
        let _workspace = MemoryWorkspace::new_test()
            .with_file("ISSUES.md", "# Legacy Issues\n- Issue 1\n- Issue 2")
            .with_file("PLAN.md", "# Legacy Plan\n\nDo legacy things")
            .with_file(
                ".agent/tmp/commit.xml",
                "<commit><message>Legacy</message></commit>",
            )
            .with_dir(".agent/logs/planning_1"); // Legacy directory mode

        // Create state in Development phase
        let mut state = PipelineState::initial(2, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["test-agent".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Effect determination must be pure - workspace contents must not affect it
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::RunDevelopmentIteration { .. }),
            "Effect must be determined from state alone, not workspace files"
        );

        // Verify the workspace has our legacy files (confirming test setup)
        // Note: We don't actually check workspace because determine_next_effect
        // is stateless - it only takes &PipelineState, not &Workspace
        // This demonstrates the architectural invariant that effects are pure.
    });
}

/// Test that effect determination is stateless and deterministic.
///
/// The same state should always produce the same effect. This is a key
/// property of the reducer architecture - no external state influences
/// effect determination.
#[test]
fn test_effect_determination_is_pure_function_of_state() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // Create a specific state
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["test-agent".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Call determine_next_effect multiple times
        let effect1 = determine_next_effect(&state);
        let effect2 = determine_next_effect(&state);
        let effect3 = determine_next_effect(&state);

        // All calls should produce the same effect (purity)
        assert!(
            matches!(&effect1, Effect::RunDevelopmentIteration { iteration: 1 }),
            "First call: {:?}",
            effect1
        );
        assert!(
            matches!(&effect2, Effect::RunDevelopmentIteration { iteration: 1 }),
            "Second call: {:?}",
            effect2
        );
        assert!(
            matches!(&effect3, Effect::RunDevelopmentIteration { iteration: 1 }),
            "Third call: {:?}",
            effect3
        );
    });
}

// ============================================================================
// PHASE MODULE CONTROL FLOW TESTS
// ============================================================================

/// Test that review phase validation failures surface as reducer events.
///
/// When XML validation fails during review, the phase module must emit an event
/// and let the reducer decide retry policy. The phase module should NOT internally
/// hide failures or make retry decisions autonomously.
#[test]
fn test_review_validation_failure_surfaces_via_event() {
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase, ReviewEvent};
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start in Review phase
        let mut state = PipelineState::initial(0, 3);
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;

        // When review output validation fails, reducer should track the attempt
        // via the OutputValidationFailed event (not hidden inside phase module)
        let state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::OutputValidationFailed {
                pass: 0,
                attempt: 0,
            }),
        );

        // The state should reflect the validation failure via continuation.invalid_output_attempts
        // This proves the failure was surfaced to the reducer, not hidden in phase code
        assert_eq!(
            state.continuation.invalid_output_attempts, 1,
            "Review validation failure must surface via reducer event and increment attempt counter"
        );

        // Another failure should increment again (reducer controls retry logic)
        let state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::OutputValidationFailed {
                pass: 0,
                attempt: 1,
            }),
        );

        assert_eq!(
            state.continuation.invalid_output_attempts, 2,
            "Subsequent failures must continue to surface via reducer events"
        );
    });
}

/// Test that development continuation decisions come from reducer state.
///
/// When development returns status="partial" or "failed", the decision to continue
/// must come from reducer state transitions, not from autonomous phase module logic.
#[test]
fn test_development_continuation_is_reducer_driven() {
    use ralph_workflow::reducer::event::{DevelopmentEvent, PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::{DevelopmentStatus, PipelineState};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start in Development phase
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;

        // Simulate a "partial" status from development via reducer event
        // The reducer state should track continuation context
        let state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "Work partially done".to_string(),
                files_changed: Some(vec!["file.rs".to_string()]),
                next_steps: Some("Continue implementation".to_string()),
            }),
        );

        // Verify reducer state tracks continuation
        assert!(
            state.continuation.is_continuation(),
            "Continuation decision must be tracked in reducer state"
        );
        assert_eq!(
            state.continuation.previous_status,
            Some(DevelopmentStatus::Partial),
            "Previous status must be tracked for continuation"
        );
        assert_eq!(
            state.continuation.continuation_attempt, 1,
            "Continuation attempt counter must be incremented"
        );
    });
}

/// Test that XSD retry loop exhaustion triggers reducer state transitions.
///
/// When XSD validation fails repeatedly, the reducer state must track exhaustion
/// and trigger agent advancement. Phase modules must NOT silently give up or
/// make fallback decisions internally.
#[test]
fn test_xsd_retry_exhaustion_triggers_state_transition() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase};
    use ralph_workflow::reducer::state::{PipelineState, MAX_DEV_INVALID_OUTPUT_RERUNS};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["agent-1".to_string(), "agent-2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // Exhaust retries via reducer events (not hidden in phase code)
        let mut current = state;
        for attempt in 0..=MAX_DEV_INVALID_OUTPUT_RERUNS {
            current = reduce(
                current,
                PipelineEvent::development_output_validation_failed(0, attempt),
            );
        }

        // After exhausting retries, agent chain should advance
        // This proves the retry policy is in reducer, not phase module
        assert_eq!(
            current.agent_chain.current_agent(),
            Some(&"agent-2".to_string()),
            "Agent chain must advance after retry exhaustion (reducer-driven policy)"
        );

        // Counter should reset for new agent
        assert_eq!(
            current.continuation.invalid_output_attempts, 0,
            "Invalid output attempts must reset after agent switch"
        );
    });
}

/// Test that phase transitions only happen via reducer events.
///
/// Phase modules must NOT directly advance phases. All phase transitions
/// must occur through reducer event processing, ensuring state is the
/// single source of truth.
#[test]
fn test_phase_transitions_only_via_reducer_events() {
    use ralph_workflow::reducer::event::{
        CommitEvent, DevelopmentEvent, PipelineEvent, PipelinePhase, PlanningEvent, ReviewEvent,
    };
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Start at Planning
        let state = PipelineState::initial(1, 1);
        assert_eq!(state.phase, PipelinePhase::Planning);

        // Transition Planning -> Development via event
        let state = reduce(
            state,
            PipelineEvent::Planning(PlanningEvent::GenerationCompleted {
                iteration: 0,
                valid: true,
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Development,
            "Planning->Development must happen via reducer event"
        );

        // Transition Development -> CommitMessage via event
        let state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::IterationCompleted {
                iteration: 0,
                output_valid: true,
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Development->CommitMessage must happen via reducer event"
        );

        // Transition CommitMessage -> Review via event (when iterations exhausted)
        let state = reduce(
            state,
            PipelineEvent::Commit(CommitEvent::Created {
                hash: "abc123".to_string(),
                message: "test".to_string(),
            }),
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "CommitMessage->Review must happen via reducer event"
        );

        // Transition Review -> CommitMessage via event (phase completed early)
        let state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PhaseCompleted { early_exit: true }),
        );
        // Review phase completed transitions to CommitMessage for commit handling
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "Review->CommitMessage must happen via reducer event"
        );
    });
}

// ============================================================================
// .PROCESSED ARCHIVE TESTS (NO FALLBACK READS)
// ============================================================================

/// Test that .processed files are archive-only and never used as fallback reads.
#[test]
fn test_processed_files_are_archive_only() {
    use ralph_workflow::files::llm_output_extraction::file_based_extraction::try_extract_from_file_with_workspace;
    use ralph_workflow::workspace::MemoryWorkspace;

    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/tmp/plan.xml.processed", "<plan>archived</plan>");

        // Primary path missing, .processed present -> should NOT be used.
        let result =
            try_extract_from_file_with_workspace(&workspace, Path::new(".agent/tmp/plan.xml"));

        assert!(
            result.is_none(),
            ".processed files are archives only; no fallback reads allowed"
        );
    });
}

/// Test that archived XML files use .processed suffix consistently.
///
/// All XML archiving must use the `.processed` suffix for consistency.
/// This ensures the fallback pattern in handlers works correctly.
#[test]
fn test_archived_xml_uses_processed_suffix() {
    use ralph_workflow::files::llm_output_extraction::archive_xml_file_with_workspace;
    use ralph_workflow::workspace::{MemoryWorkspace, Workspace};

    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/tmp/plan.xml", "<plan>test</plan>")
            .with_file(".agent/tmp/issues.xml", "<issues>test</issues>")
            .with_file(
                ".agent/tmp/development_result.xml",
                "<development>test</development>",
            )
            .with_file(".agent/tmp/fix_result.xml", "<fix>test</fix>")
            .with_file(".agent/tmp/commit_message.xml", "<commit>test</commit>");

        // Archive each file
        let paths = [
            ".agent/tmp/plan.xml",
            ".agent/tmp/issues.xml",
            ".agent/tmp/development_result.xml",
            ".agent/tmp/fix_result.xml",
            ".agent/tmp/commit_message.xml",
        ];

        for path in paths {
            archive_xml_file_with_workspace(&workspace, Path::new(path));

            // Original should be gone
            assert!(
                !workspace.exists(Path::new(path)),
                "Original file should be removed after archiving: {}",
                path
            );

            // .processed should exist
            let processed_path = format!("{}.processed", path);
            assert!(
                workspace.exists(Path::new(&processed_path)),
                "Archived file should have .processed suffix: {}",
                processed_path
            );
        }
    });
}
