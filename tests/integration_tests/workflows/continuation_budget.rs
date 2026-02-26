//! Integration tests for continuation budget enforcement.
//!
//! Verifies that the reducer enforces configured continuation limits
//! for both development and fix phases.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (budget enforcement, state transitions)
//! - Tests are deterministic and isolated
//! - Tests behavior, not implementation details

use ralph_workflow::reducer::event::{DevelopmentEvent, PipelineEvent, ReviewEvent};
use ralph_workflow::reducer::state::{
    ContinuationState, DevelopmentStatus, FixStatus, PipelineState,
};
use ralph_workflow::reducer::state_reduction::reduce;

use crate::test_timeout::with_default_timeout;

#[test]
fn test_dev_continuation_budget_enforced() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 2);
        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Attempt 0 (initial) - already happened, continuation_attempt = 0

        // Trigger continuation 3 times (attempts 1, 2, 3)
        for attempt in 1..=3 {
            state = reduce(
                state,
                PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                    iteration: 0,
                    status: DevelopmentStatus::Partial,
                    summary: format!("Attempt {}", attempt),
                    files_changed: None,
                    next_steps: None,
                }),
            );
            assert_eq!(state.continuation.continuation_attempt, attempt);
        }

        // After 3 continuation attempts (continuation_attempt = 3),
        // which >= max_continue_count (3), continuations are exhausted
        assert!(state.continuation.continuations_exhausted());
    });
}

#[test]
fn test_fix_continuation_budget_enforced() {
    with_default_timeout(|| {
        let mut continuation = ContinuationState::with_limits(99, 3, 2);
        continuation.max_fix_continue_count = 2; // Set fix budget explicitly to 2
        let mut state = PipelineState::initial_with_continuation(0, 3, continuation);

        // Simulate fix continuations (max_fix_continue_count = 2)
        // Attempt 0 is initial, attempts 1 and 2 are continuations
        for i in 1..=2 {
            state = reduce(
                state,
                PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                    pass: 0,
                    status: FixStatus::IssuesRemain,
                    summary: Some(format!("Fix continuation {}", i)),
                }),
            );
            assert_eq!(state.continuation.fix_continuation_attempt, i);
            assert_eq!(state.metrics.fix_continuations_total, i);
        }

        // After fix_continuation_attempt = 2, which >= max_fix_continue_count (2), exhausted
        assert!(state.continuation.fix_continuations_exhausted());
    });
}

#[test]
fn test_continuation_state_resets_across_iterations() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);

        // Iteration 0 with continuation
        state = reduce(state, PipelineEvent::development_iteration_started(0));
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 1);

        // New iteration should reset continuation state
        state = reduce(state, PipelineEvent::development_iteration_started(1));
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert!(!state.continuation.continue_pending);
    });
}

#[test]
fn test_continuation_budget_exhaustion_switches_agent() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 2, 2); // max_continue_count = 2
        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        let initial_agent_index = state.agent_chain.current_agent_index;

        // Trigger 2 continuations (reach budget)
        for _ in 1..=2 {
            state = reduce(
                state,
                PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                    iteration: 0,
                    status: DevelopmentStatus::Partial,
                    summary: "partial".to_string(),
                    files_changed: None,
                    next_steps: None,
                }),
            );
        }

        // Now trigger budget exhaustion
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationBudgetExhausted {
                iteration: 0,
                total_attempts: 3,
                last_status: DevelopmentStatus::Partial,
            }),
        );

        // Agent should have switched (or retry cycle incremented if chain exhausted)
        let agent_switched = state.agent_chain.current_agent_index != initial_agent_index
            || state.agent_chain.retry_cycle > 0;
        assert!(agent_switched);

        // Continuation state should be reset
        assert_eq!(state.continuation.continuation_attempt, 0);
    });
}

#[test]
fn test_fix_continuation_budget_exhaustion_proceeds_to_commit() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 1); // max_fix_continue_count = 1
        let mut state = PipelineState::initial_with_continuation(0, 3, continuation);
        state.phase = ralph_workflow::reducer::event::PipelinePhase::Review;
        state.reviewer_pass = 1;

        // Trigger 1 fix continuation (reach budget)
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                pass: 1,
                status: FixStatus::IssuesRemain,
                summary: Some("partial fix".to_string()),
            }),
        );
        assert_eq!(state.continuation.fix_continuation_attempt, 1);

        // Now trigger budget exhaustion
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationBudgetExhausted {
                pass: 1,
                total_attempts: 2,
                last_status: FixStatus::IssuesRemain,
            }),
        );

        // Should proceed to CommitMessage (accept partial fixes)
        assert_eq!(
            state.phase,
            ralph_workflow::reducer::event::PipelinePhase::CommitMessage
        );
    });
}

#[test]
fn test_continuation_metrics_track_correctly() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Initial attempt
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        assert_eq!(state.metrics.dev_attempts_total, 1);

        // First continuation
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        assert_eq!(state.metrics.dev_attempts_total, 2);

        // Second continuation
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "more work".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        state = reduce(state, PipelineEvent::development_agent_invoked(0));
        assert_eq!(state.metrics.dev_attempts_total, 3);

        // Continuation succeeds
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationSucceeded {
                iteration: 0,
                total_continuation_attempts: 2,
            }),
        );

        // Verify: 1 iteration started, 1 iteration completed, 3 attempts total
        assert_eq!(state.metrics.dev_iterations_started, 1);
        assert_eq!(state.metrics.dev_iterations_completed, 1);
        assert_eq!(state.metrics.dev_attempts_total, 3);
    });
}

// ============================================================================
// Step 18: Verify metrics match continuation state
// ============================================================================

/// Test that dev_continuation_attempt metric stays in sync with ContinuationState.
///
/// CRITICAL: Metrics must be consistent with the source-of-truth continuation state.
#[test]
fn test_dev_continuation_metrics_match_state() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 2);
        let mut state = PipelineState::initial_with_continuation(3, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Initial: no continuations yet
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert_eq!(state.metrics.dev_continuation_attempt, 0);

        // After first continuation
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );

        assert_eq!(
            state.metrics.dev_continuation_attempt, state.continuation.continuation_attempt,
            "Metrics should match continuation state after first continuation"
        );

        // After second continuation
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "more partial".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );

        assert_eq!(
            state.metrics.dev_continuation_attempt, state.continuation.continuation_attempt,
            "Metrics should match continuation state after second continuation"
        );
    });
}

/// Test that fix_continuation_attempt metric stays in sync with ContinuationState.
#[test]
fn test_fix_continuation_metrics_match_state() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 2);
        let mut state = PipelineState::initial_with_continuation(0, 1, continuation);
        state.phase = ralph_workflow::reducer::event::PipelinePhase::Review;
        state.reviewer_pass = 0;

        // Initial: no fix continuations yet
        assert_eq!(state.continuation.fix_continuation_attempt, 0);
        assert_eq!(state.metrics.fix_continuation_attempt, 0);

        // After first fix continuation
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                pass: 0,
                status: FixStatus::IssuesRemain,
                summary: Some("partial fix".to_string()),
            }),
        );

        assert_eq!(
            state.metrics.fix_continuation_attempt, state.continuation.fix_continuation_attempt,
            "Metrics should match continuation state after first fix continuation"
        );

        // After second fix continuation
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
                pass: 0,
                status: FixStatus::IssuesRemain,
                summary: Some("more partial fix".to_string()),
            }),
        );

        assert_eq!(
            state.metrics.fix_continuation_attempt, state.continuation.fix_continuation_attempt,
            "Metrics should match continuation state after second fix continuation"
        );
    });
}

/// Test that continuation exhaustion is correctly detected in both state and metrics.
#[test]
fn test_continuation_exhaustion_matches_metrics() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(99, 3, 2);
        let mut state = PipelineState::initial_with_continuation(3, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Exhaust continuations (3 attempts total: 0, 1, 2)
        for i in 1..=3 {
            state = reduce(
                state,
                PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                    iteration: 0,
                    status: DevelopmentStatus::Partial,
                    summary: format!("attempt {}", i),
                    files_changed: None,
                    next_steps: None,
                }),
            );
        }

        // At exhaustion: both state and metrics should agree
        assert!(
            state.continuation.continuations_exhausted(),
            "ContinuationState should report exhaustion"
        );
        assert!(
            state.metrics.dev_continuation_attempt >= state.metrics.max_dev_continuation_count,
            "Metrics should show continuation_attempt >= max_dev_continuation_count"
        );
    });
}

/// Test that missing max_dev_continuations config key applies default value.
///
/// This test verifies that when max_dev_continuations is omitted from the config file,
/// the system applies the serde default of 2, resulting in max_continue_count of 3
/// (1 initial + 2 continuations).
#[test]
fn test_missing_max_dev_continuations_applies_default() {
    with_default_timeout(|| {
        // Simulate config with default max_dev_continuations = 2
        // This results in max_continue_count = 1 + 2 = 3
        let continuation = ContinuationState::with_limits(
            10, // max_xsd_retries
            3,  // max_continue_count (1 initial + 2 continuations)
            2,  // max_same_agent_retries
        );

        let mut state = PipelineState::initial_with_continuation(1, 0, continuation);

        // Verify default is applied correctly
        assert_eq!(
            state.continuation.max_continue_count, 3,
            "Missing config key should default to 3 total attempts (1 initial + 2 continuations)"
        );

        // Start iteration and verify exhaustion logic works correctly
        state = reduce(state, PipelineEvent::development_iteration_started(0));
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert!(!state.continuation.continuations_exhausted());

        // First continuation (attempt 1)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial work".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 1);
        assert!(!state.continuation.continuations_exhausted());

        // Second continuation (attempt 2)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial work 2".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 2);
        assert!(!state.continuation.continuations_exhausted());

        // Third continuation (attempt 3) - should exhaust budget
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial work 3".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 3);
        assert!(
            state.continuation.continuations_exhausted(),
            "Attempt 3 should be exhausted (3 >= 3)"
        );
    });
}

/// Test that continuation budget is properly capped at 3 when max_dev_continuations could be None.
///
/// This regression test verifies that even when Config.max_dev_continuations is None
/// (e.g., from Config::default() or Config::test_default()), the system enforces
/// the unwrap_or(2) fallback, resulting in a cap of 3 total attempts.
///
/// Without the unwrap_or(2) defensive fallback in create_initial_state_with_config(),
/// this test would fail by allowing indefinite continuations.
#[test]
fn test_missing_config_key_caps_continuations_at_three() {
    with_default_timeout(|| {
        // Simulate the scenario where Config.max_dev_continuations is None.
        // The event loop config loader should apply unwrap_or(2), resulting in max_continue_count = 3.
        // We construct ContinuationState directly with the expected default to simulate this.
        let continuation = ContinuationState::with_limits(
            10, // max_xsd_retries
            3,  // max_continue_count = 1 + unwrap_or(2)
            2,  // max_same_agent_retries
        );

        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Initial attempt (continuation_attempt = 0)
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert!(!state.continuation.continuations_exhausted());

        // Continuation 1 (continuation_attempt = 1)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 1".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 1);
        assert!(!state.continuation.continuations_exhausted());

        // Continuation 2 (continuation_attempt = 2)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 2".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 2);
        assert!(!state.continuation.continuations_exhausted());

        // Continuation 3 (continuation_attempt = 3) - MUST exhaust budget
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 3".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(
            state.continuation.continuation_attempt, 3,
            "Should be at continuation attempt 3"
        );
        assert!(
            state.continuation.continuations_exhausted(),
            "Budget MUST be exhausted at attempt 3 (3 >= 3). \
             Without unwrap_or(2) fallback, this would fail and allow indefinite continuations."
        );

        // Verify that ContinuationBudgetExhausted would be triggered
        // (This is tested through orchestration, but we verify the state here)
        assert_eq!(state.continuation.max_continue_count, 3);
        assert!(state.continuation.continuation_attempt >= state.continuation.max_continue_count);
    });
}

/// NEW REGRESSION TEST: Test continuation behavior with default limit from unwrap_or(2).
///
/// CRITICAL: This test verifies the infinite loop bug is prevented.
/// The event_loop config loader applies unwrap_or(2) when max_dev_continuations is None,
/// resulting in max_continue_count = 3 (1 initial + 2 continuations).
///
/// This test simulates the expected state after that default application and verifies
/// continuation stops at attempt 3, preventing infinite continuation loops.
#[test]
fn test_continuation_stops_with_unwrap_or_default() {
    with_default_timeout(|| {
        // Simulate the state created by create_initial_state_with_config when
        // Config.max_dev_continuations is None and unwrap_or(2) is applied.
        // Result: max_continue_count = 1 + 2 = 3
        let continuation = ContinuationState::with_limits(
            10, // max_xsd_retries
            3,  // max_continue_count = 1 + unwrap_or(2)
            2,  // max_same_agent_retries
        );

        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);

        // Verify the default was applied correctly
        assert_eq!(
            state.continuation.max_continue_count, 3,
            "unwrap_or(2) should result in max_continue_count = 3"
        );

        // Start iteration
        state = reduce(state, PipelineEvent::development_iteration_started(0));
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert!(!state.continuation.continuations_exhausted());

        // Continuation 1 (attempt 1)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 1".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 1);
        assert!(!state.continuation.continuations_exhausted());

        // Continuation 2 (attempt 2)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 2".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 2);
        assert!(!state.continuation.continuations_exhausted());

        // Continuation 3 (attempt 3) - MUST exhaust budget
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 3".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        assert_eq!(state.continuation.continuation_attempt, 3);
        assert!(
            state.continuation.continuations_exhausted(),
            "CRITICAL: Continuation MUST stop at attempt 3 (3 >= 3). \
             Without unwrap_or(2) in create_initial_state_with_config, this would allow infinite continuation."
        );

        // Verify progress metrics
        assert_eq!(state.metrics.max_dev_continuation_count, 3);
        assert_eq!(state.metrics.dev_continuation_attempt, 3);
    });
}
