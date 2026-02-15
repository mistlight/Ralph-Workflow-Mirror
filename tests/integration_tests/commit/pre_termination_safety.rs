//! Integration tests for pre-termination commit safety checks.
//!
//! These tests verify that before pipeline termination:
//! 1. `CheckUncommittedChangesBeforeTermination` effect is derived
//! 2. Uncommitted changes route back through `CommitMessage` (not AwaitingDevFix)
//! 3. Clean working directory allows termination to proceed
//! 4. User-initiated interrupts (Ctrl+C) skip the safety check
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** via effect capture and state transitions
//! - Uses `MockAppEffectHandler` for git snapshot simulation
//! - NO `TempDir`, `std::fs`, or real git operations
//! - Tests are deterministic and verify state machine behavior

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;
use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use std::path::PathBuf;

/// Standard PROMPT.md content for safety check tests.
const STANDARD_PROMPT: &str = r#"## Goal

Do something.

## Acceptance

- Tests pass
"#;

/// Test that CheckUncommittedChangesBeforeTermination effect is derived before Complete.
///
/// This verifies that when pipeline is about to complete:
/// 1. Phase transitions to Complete
/// 2. pre_termination_commit_checked flag is initially false
/// 3. Orchestration derives CheckUncommittedChangesBeforeTermination effect
#[test]
fn test_safety_check_effect_derived_before_complete() {
    with_default_timeout(|| {
        // Set up minimal scenario to reach completion
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff("")
            .with_staged_changes(false); // No changes to commit
                                         // Note: MockAppEffectHandler returns empty snapshot by default (clean working directory)

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify that CheckUncommittedChangesBeforeTermination was executed
        let safety_check_executed = effect_handler
            .was_effect_executed(|e| matches!(e, Effect::CheckUncommittedChangesBeforeTermination));

        assert!(
            safety_check_executed,
            "CheckUncommittedChangesBeforeTermination effect should be derived before termination"
        );

        // Verify that pre_termination_commit_checked flag was set
        assert!(
            effect_handler.state.pre_termination_commit_checked,
            "pre_termination_commit_checked flag should be set after safety check"
        );
    });
}

/// Test that clean working directory unblocks termination.
///
/// This verifies that when git snapshot shows no uncommitted changes:
/// 1. Handler emits `PreTerminationSafetyCheckPassed` event
/// 2. Reducer sets `pre_termination_commit_checked = true`
/// 3. Pipeline proceeds to termination (SaveCheckpoint)
#[test]
fn test_clean_working_directory_allows_termination() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff("")
            .with_staged_changes(false);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Verify flag was set (this indicates safety check passed event was processed)
        assert!(
            effect_handler.state.pre_termination_commit_checked,
            "Flag should be set after clean check (indicates event was emitted and processed)"
        );
    });
}

/// Test that uncommitted changes route back through commit phase.
///
/// This verifies that when git snapshot shows uncommitted changes:
/// 1. Handler emits `PreTerminationUncommittedChangesDetected`
/// 2. Reducer routes to `CommitMessage` to force a commit (or explicit skip)
/// 3. Pipeline does NOT terminate with uncommitted work
#[test]
fn test_uncommitted_changes_route_back_through_commit_phase() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff("")
            .with_staged_changes(false);

        let mut initial_state = PipelineState::initial(0, 0);
        initial_state.phase = PipelinePhase::Complete;
        initial_state.pre_termination_commit_checked = false;

        let mut effect_handler =
            MockEffectHandler::new(initial_state).with_dirty_pre_termination_snapshot(2);
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Observable behavior: safety check executes and the pipeline routes back
        // through commit phase (commit effects execute).
        assert!(
            effect_handler.was_effect_executed(|e| {
                matches!(e, Effect::CheckUncommittedChangesBeforeTermination)
            }),
            "Safety check effect should execute"
        );
        assert!(
            effect_handler.was_effect_executed(|e| matches!(e, Effect::CheckCommitDiff)),
            "Dirty status should route back through CommitMessage"
        );
        assert!(
            effect_handler.state.pre_termination_commit_checked,
            "Termination should be unblocked after safety commit completes"
        );
    });
}

/// Test that Ctrl+C (interrupted_by_user=true) exception is handled correctly.
///
/// Note: This is a unit test disguised as an integration test because we cannot
/// trigger actual SIGINT in the integration test harness. The behavior is tested
/// via unit tests in orchestration/phase_effects/mod.rs which verify that when
/// interrupted_by_user=true, the safety check effect is NOT derived.
///
/// This test documents that in normal pipeline flow (not user-interrupted),
/// the safety check DOES execute, establishing the baseline for comparison.
#[test]
fn test_user_interrupt_exception_is_documented() {
    with_default_timeout(|| {
        // Normal flow (no user interrupt) - safety check should execute
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff("")
            .with_staged_changes(false);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // In normal flow (interrupted_by_user=false), safety check executes
        let safety_check_executed = effect_handler
            .was_effect_executed(|e| matches!(e, Effect::CheckUncommittedChangesBeforeTermination));

        assert!(
            safety_check_executed,
            "Normal flow should execute safety check (establishes baseline)"
        );

        // interrupted_by_user remains false (was never set by SIGINT)
        assert!(
            !effect_handler.state.interrupted_by_user,
            "Normal flow should NOT set interrupted_by_user flag"
        );

        // The actual test for interrupted_by_user=true exception is in unit tests
        // (orchestration/phase_effects/mod.rs) because integration tests can't
        // simulate SIGINT signal handling that sets the flag.
    });
}

/// Test that safety check prevents termination on programmatic interrupt.
///
/// This verifies that non-user interrupts (like AwaitingDevFix exhaustion)
/// still go through the safety check.
#[test]
fn test_programmatic_interrupt_requires_safety_check() {
    with_default_timeout(|| {
        // Simulate a programmatic interrupt scenario (not user-initiated)
        let mut initial_state = PipelineState::initial(0, 0);
        initial_state.interrupted_by_user = false; // Key: NOT user-initiated
        initial_state.phase = PipelinePhase::Interrupted;
        initial_state.pre_termination_commit_checked = false;

        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT);

        let mut effect_handler = MockEffectHandler::new(initial_state);
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Observable behavior: Programmatic interrupts should still execute
        // the safety check before terminating
        let safety_check_executed = effect_handler
            .was_effect_executed(|e| matches!(e, Effect::CheckUncommittedChangesBeforeTermination));

        assert!(
            safety_check_executed,
            "Programmatic interrupts should execute safety check"
        );

        assert!(
            effect_handler.state.pre_termination_commit_checked,
            "Safety check flag should be set after programmatic interrupt"
        );
    });
}

/// Test that safety check is only executed once per termination.
///
/// This verifies the flag prevents infinite loops:
/// 1. First cycle: pre_termination_commit_checked=false → derives effect
/// 2. Effect executes → emits PreTerminationSafetyCheckPassed
/// 3. Reducer sets pre_termination_commit_checked=true
/// 4. Second cycle: pre_termination_commit_checked=true → skips effect
#[test]
fn test_safety_check_executes_only_once() {
    with_default_timeout(|| {
        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff("")
            .with_staged_changes(false);

        let mut effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Count how many times the safety check effect was executed
        let safety_check_count = effect_handler
            .captured_effects()
            .iter()
            .filter(|e| matches!(e, Effect::CheckUncommittedChangesBeforeTermination))
            .count();

        assert_eq!(
            safety_check_count, 1,
            "Safety check should execute exactly once per termination sequence"
        );

        assert!(
            effect_handler.state.pre_termination_commit_checked,
            "Flag should prevent re-execution"
        );
    });
}

/// Test that git snapshot failure routes to error handling.
///
/// This verifies that if git_snapshot() fails during safety check:
/// 1. GitStatusFailed error event is emitted
/// 2. Pipeline routes to AwaitingDevFix phase
/// 3. Does not silently allow termination
#[test]
fn test_git_snapshot_failure_routes_to_error() {
    with_default_timeout(|| {
        // Reducer-only: simulating a snapshot failure via the full event loop would
        // enter the recovery loop (non-terminating by design).
        use ralph_workflow::reducer::event::PipelineEvent;
        use ralph_workflow::reducer::event::{ErrorEvent, PromptInputEvent, WorkspaceIoErrorKind};
        use ralph_workflow::reducer::state_reduction::reduce;

        let mut state = PipelineState::initial(0, 0);
        state.phase = PipelinePhase::Complete;

        let event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: state.phase,
            error: ErrorEvent::GitStatusFailed {
                kind: WorkspaceIoErrorKind::Other,
            },
        });

        let new_state = reduce(state, event);
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
    });
}

/// Test that safety check catches the exact bug scenario from the issue.
///
/// This test reproduces the bug scenario where:
/// 1. Development iteration completes successfully (code changes made)
/// 2. Some bug causes commit phase to be skipped or bypassed
/// 3. Pipeline attempts to transition to Complete
/// 4. Safety check MUST execute before termination
/// 5. If uncommitted changes exist, termination is prevented
///
/// Note: The MockAppEffectHandler doesn't support simulating actual uncommitted
/// changes in git state, so this test verifies the safety check executes.
/// The uncommitted changes error path is tested in unit tests.
#[test]
fn test_safety_check_prevents_lost_work_on_premature_termination() {
    with_default_timeout(|| {
        // Simulate the bug scenario:
        // - Development completed (would have staged changes)
        // - Commit phase was bypassed (commit_diff_prepared = false)
        // - Pipeline reaches FinalValidation/Complete
        let mut initial_state = PipelineState::initial(1, 0);
        // Simulate that development happened but commit was skipped
        initial_state.phase = PipelinePhase::FinalValidation;
        initial_state.pre_termination_commit_checked = false;
        // Key indicator of bug: commit_diff_prepared should be true if commit
        // phase ran properly, but it's false (commit was skipped)

        let mut app_handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            .with_file("PROMPT.md", STANDARD_PROMPT)
            .with_diff("+ new code") // Changes exist
            .with_staged_changes(true); // Work is staged

        let mut effect_handler = MockEffectHandler::new(initial_state);
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler)
            .unwrap();

        // Key verification: Safety check MUST execute before termination
        let safety_check_executed = effect_handler
            .was_effect_executed(|e| matches!(e, Effect::CheckUncommittedChangesBeforeTermination));

        assert!(
            safety_check_executed,
            "Pre-termination safety check must execute before completion, \
             even if commit phase was bypassed by a bug"
        );

        // Safety check flag should be set
        assert!(
            effect_handler.state.pre_termination_commit_checked,
            "Flag should be set after safety check executes"
        );

        // Note: In a real scenario with uncommitted changes, the safety check
        // handler would detect them and emit an error event that routes to
        // AwaitingDevFix. We can't test that path with MockAppEffectHandler,
        // but the unit tests verify that behavior.
        //
        // This integration test verifies:
        // 1. Safety check effect is derived when reaching termination
        // 2. Handler executes the check regardless of prior phase state
        // 3. The check runs even if commit phase was improperly bypassed
    });
}
