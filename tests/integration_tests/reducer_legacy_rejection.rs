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
use ralph_workflow::files::result_extraction::extract_last_result;
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
                "reviewer_reviews": 2,
                "skip_rebase": false
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
// RESULT EXTRACTION TESTS
// ============================================================================

/// Test that result extraction ignores legacy directory mode.
///
/// When a directory exists at the log path (legacy behavior), extraction
/// should NOT scan the directory. Only prefix-based file matching should work.
#[test]
fn test_result_extraction_ignores_directory_mode() {
    with_default_timeout(|| {
        // Create a workspace with a directory at the log path (legacy)
        // and a file inside it
        // Note: result field must be a string, not an object
        let result_event = "{\"type\":\"result\",\"result\":\"# Plan\\n\\n## Summary\\nLegacy directory content\"}";
        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent/logs/planning_1")
            .with_file(".agent/logs/planning_1/output.log", result_event);

        let log_path = Path::new(".agent/logs/planning_1");
        let result = extract_last_result(&workspace, log_path).unwrap();

        // Legacy directory mode should be ignored
        assert!(
            result.is_none(),
            "Should not extract from directory (legacy mode removed)"
        );
    });
}

/// Test that result extraction ignores subdirectory fallback.
///
/// Legacy logs where agent names with "/" created nested directories
/// (e.g., "planning_1_ccs/glm_0.log") should no longer be found.
#[test]
fn test_result_extraction_ignores_subdirectory_fallback() {
    with_default_timeout(|| {
        // Create a workspace with nested subdirectory structure (legacy)
        // Note: result field must be a string, not an object
        let result_event = "{\"type\":\"result\",\"result\":\"# Plan\\n\\n## Summary\\nNested subdirectory content\"}";
        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent/logs")
            .with_file(".agent/logs/planning_1_ccs/glm_0.log", result_event);

        let log_path = Path::new(".agent/logs/planning_1");
        let result = extract_last_result(&workspace, log_path).unwrap();

        // Legacy subdirectory fallback should be ignored
        assert!(
            result.is_none(),
            "Should not extract from subdirectory fallback (legacy mode removed)"
        );
    });
}

/// Test that result extraction works with current prefix mode.
///
/// The primary extraction mode uses prefix-based file matching:
/// `{prefix}_*.log` files in the parent directory.
#[test]
fn test_result_extraction_uses_prefix_mode() {
    with_default_timeout(|| {
        // Create a workspace with prefix-based log file (current convention)
        // Note: result field must be a string, not an object
        let result_event =
            "{\"type\":\"result\",\"result\":\"# Plan\\n\\n## Summary\\nPrefix mode content\"}";
        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent/logs")
            .with_file(".agent/logs/planning_1_claude_0.log", result_event);

        let log_path = Path::new(".agent/logs/planning_1");
        let result = extract_last_result(&workspace, log_path).unwrap();

        // Prefix mode should work
        assert!(
            result.is_some(),
            "Should extract from prefix-based log file"
        );
        assert!(
            result.unwrap().contains("Prefix mode content"),
            "Should contain expected content"
        );
    });
}

/// Test that exact file fallback still works.
///
/// If the exact path exists as a file, it should be read (this is Strategy 4,
/// now Strategy 2 after removing legacy modes).
#[test]
fn test_result_extraction_exact_file_fallback() {
    with_default_timeout(|| {
        // Create a workspace with exact file at log path
        // Note: result field must be a string, not an object
        let result_event =
            "{\"type\":\"result\",\"result\":\"# Plan\\n\\n## Summary\\nExact file content\"}";
        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/logs/exact.log", result_event);

        let log_path = Path::new(".agent/logs/exact.log");
        let result = extract_last_result(&workspace, log_path).unwrap();

        // Exact file fallback should still work
        assert!(result.is_some(), "Should extract from exact file path");
        assert!(
            result.unwrap().contains("Exact file content"),
            "Should contain expected content"
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
