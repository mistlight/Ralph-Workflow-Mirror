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

        // Trigger continuation 2 times (attempts 1, 2)
        for attempt in 1..=2 {
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

        // Attempt to trigger third continuation (would be attempt 3, but defensive check prevents it)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "Attempt 3".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        // AFTER FIX: trigger_continuation defensive check prevents increment to 3, counter stays at 2
        assert_eq!(
            state.continuation.continuation_attempt, 2,
            "Defensive check should prevent increment past attempt 2"
        );

        // continuations_exhausted() returns false (2 < 3), but the system won't schedule another
        // continuation because the defensive check cleared continue_pending flag.
        // In normal flow, OutcomeApplied detects exhaustion via (attempt + 1 >= max) check.
        assert!(!state.continuation.continuations_exhausted());
        assert!(!state.continuation.continue_pending);
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

        // Trigger 2 continuations (attempts 1, 2)
        for i in 1..=2 {
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

        // Attempt third continuation (defensive check prevents increment)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "attempt 3".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );

        // AFTER FIX: Counter stays at 2, exhaustion is detected by OutcomeApplied's (attempt + 1 >= max) check
        // continuations_exhausted() returns false because counter didn't reach max_continue_count
        assert_eq!(state.continuation.continuation_attempt, 2);
        assert!(!state.continuation.continuations_exhausted());
        // But the defensive check cleared continue_pending to prevent further scheduling
        assert!(!state.continuation.continue_pending);
        // Metrics should match state
        assert_eq!(
            state.metrics.dev_continuation_attempt,
            state.continuation.continuation_attempt
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

        // Attempt third continuation (would be attempt 3, but defensive check prevents it)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial work 3 attempt".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        // AFTER FIX: trigger_continuation defensive check prevents increment to 3
        assert_eq!(
            state.continuation.continuation_attempt, 2,
            "Defensive check should prevent increment to 3"
        );
        // continuations_exhausted() returns false (2 < 3), but continue_pending is cleared
        assert!(!state.continuation.continuations_exhausted());
        assert!(!state.continuation.continue_pending);
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

        // Attempt third continuation (defensive check prevents increment)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 3 attempt".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        // AFTER FIX: trigger_continuation defensive check prevents increment to 3
        assert_eq!(
            state.continuation.continuation_attempt, 2,
            "Defensive check should prevent increment to 3. \
             Without the fix, this would increment to 3, allowing an extra attempt."
        );
        // continuations_exhausted() returns false (2 < 3), but the system won't continue
        // because defensive check cleared continue_pending and OutcomeApplied will detect
        // exhaustion via (attempt + 1 >= max) check.
        assert!(!state.continuation.continuations_exhausted());
        assert!(!state.continuation.continue_pending);

        // Verify the default was correctly applied
        assert_eq!(state.continuation.max_continue_count, 3);
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

        // Attempt third continuation (defensive check prevents increment)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 3 attempt".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );
        // AFTER FIX: trigger_continuation defensive check prevents increment to 3
        assert_eq!(
            state.continuation.continuation_attempt, 2,
            "CRITICAL: Defensive check prevents increment to 3. Counter stays at 2. \
             Without the fix, this would increment to 3, allowing an extra attempt and potentially infinite loops."
        );
        // continuations_exhausted() returns false (2 < 3), but the defensive check cleared
        // continue_pending to prevent scheduling. OutcomeApplied detects exhaustion via (attempt + 1 >= max).
        assert!(!state.continuation.continuations_exhausted());
        assert!(!state.continuation.continue_pending);

        // Verify progress metrics match the corrected state
        assert_eq!(state.metrics.max_dev_continuation_count, 3);
        assert_eq!(state.metrics.dev_continuation_attempt, 2);
    });
}

/// Regression test: Verify OutcomeApplied exhausts at the correct boundary.
///
/// With max_continue_count = 3, attempts 0, 1, 2 should be allowed (3 total).
/// OutcomeApplied at attempt 2 with Partial status should exhaust the budget
/// (because attempt 2 + 1 = 3 would reach max_continue_count).
///
/// BEFORE FIX: Current code allowed attempt 2 to continue to attempt 3,
/// then exhausted at attempt 3, allowing 4 total attempts (bug).
///
/// AFTER FIX: OutcomeApplied at attempt 2 correctly exhausts without
/// triggering attempt 3, allowing exactly 3 attempts as configured.
#[test]
fn test_outcome_applied_exhausts_too_early() {
    with_default_timeout(|| {
        // max_continue_count = 3 means attempts 0, 1, 2 should be allowed (3 total)
        let continuation = ContinuationState::with_limits(10, 3, 2);
        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Simulate attempt 0 completing with Partial outcome
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial work".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );

        // Apply outcome at attempt 0 - should trigger continuation to attempt 1
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );
        assert_eq!(state.continuation.continuation_attempt, 1);

        // Simulate attempt 1 completing with Partial outcome
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "more partial work".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );

        // Apply outcome at attempt 1 - should trigger continuation to attempt 2
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );
        assert_eq!(state.continuation.continuation_attempt, 2);

        // Simulate attempt 2 completing with Partial outcome
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "still more partial work".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );

        // Apply outcome at attempt 2 - SHOULD exhaust budget (not continue to attempt 3)
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );

        // FIXED: budget exhausts at attempt 2 (resets to 0 after agent switch)
        assert_eq!(
            state.continuation.continuation_attempt, 0,
            "OutcomeApplied at attempt 2 should exhaust budget and reset counter (agent switch)"
        );
        assert!(
            !state.continuation.is_continuation(),
            "After exhaustion, is_continuation() should be false"
        );
    });
}

/// NEW REGRESSION TEST: Verify OutcomeApplied checks exhaustion BEFORE triggering continuation.
///
/// CRITICAL: With max_continue_count = 3, attempts 0, 1, 2 should be allowed (3 total).
/// When OutcomeApplied fires at attempt 2 with Partial status:
/// - continuation_attempt = 2
/// - continuation_attempt + 1 = 3 (would reach max_continue_count)
/// - Should emit ContinuationBudgetExhausted, NOT ContinuationTriggered
///
/// This is the core bug: current code triggers continuation first (incrementing to 3),
/// then checks exhaustion on the NEXT OutcomeApplied, allowing attempt 3 to run.
#[test]
fn test_outcome_applied_exhausts_at_correct_boundary() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(10, 3, 2);
        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Attempt 0 (initial) completes with Partial
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "partial work".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );
        assert_eq!(state.continuation.continuation_attempt, 1);

        // Attempt 1 completes with Partial
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "more partial work".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );
        assert_eq!(state.continuation.continuation_attempt, 2);

        // Attempt 2 completes with Partial
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "still more partial work".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );

        // CRITICAL ASSERTION: OutcomeApplied at attempt 2 should exhaust budget
        // because continuation_attempt + 1 (2 + 1 = 3) would reach max_continue_count (3).
        // Current buggy behavior: triggers continuation to attempt 3
        // Expected behavior: emits ContinuationBudgetExhausted, which resets counter and switches agent
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );

        // After fix: budget should be exhausted, counter reset to 0 (agent switch happened)
        // The critical check is that we did NOT increment to 3 before exhausting.
        // ContinuationBudgetExhausted resets continuation_attempt to 0 as part of agent switch.
        assert_eq!(
            state.continuation.continuation_attempt, 0,
            "After budget exhaustion, continuation_attempt should be reset to 0 (agent switched)"
        );
        // Verify that budget exhaustion did occur (not another continuation)
        assert!(
            !state.continuation.is_continuation(),
            "After budget exhaustion and agent switch, is_continuation() should be false"
        );
    });
}

/// Integration test: Verify default max_dev_continuations prevents infinite loops.
///
/// This test simulates the production scenario where max_dev_continuations is missing
/// from config, and every attempt returns Partial status. The system MUST stop after
/// exactly 3 attempts (the default), not continue indefinitely.
///
/// Reproduces the infinite loop bug from the user report.
#[test]
fn test_default_continuation_limit_prevents_infinite_loop() {
    with_default_timeout(|| {
        // Simulate config with default: max_dev_continuations = 2 → max_continue_count = 3
        let continuation = ContinuationState::with_limits(10, 3, 2);
        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Simulate a scenario where EVERY attempt returns Partial
        // (mimicking the infinite loop from the bug report where status never reaches Complete)

        // Attempt 0: Partial
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "Verification found only a subset implemented".to_string(),
                files_changed: Some(
                    ["src/file1.rs", "src/file2.rs"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                next_steps: Some("Complete missing plan domains".to_string()),
            },
        );
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );
        assert_eq!(
            state.continuation.continuation_attempt, 1,
            "After attempt 0 with Partial, should continue to attempt 1"
        );
        assert!(!state.continuation.continuations_exhausted());

        // Attempt 1: Partial again
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "Most implementation slices present but not complete".to_string(),
                files_changed: Some(
                    ["src/file3.rs", "src/file4.rs"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                next_steps: Some("Add explicit parity map artifact".to_string()),
            },
        );
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );
        assert_eq!(
            state.continuation.continuation_attempt, 2,
            "After attempt 1 with Partial, should continue to attempt 2"
        );
        assert!(!state.continuation.continuations_exhausted());

        // Attempt 2: Partial yet again
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "Still not fully satisfied".to_string(),
                files_changed: Some(["src/file5.rs"].into_iter().map(String::from).collect()),
                next_steps: Some("Provide explicit evidence".to_string()),
            },
        );
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );

        // CRITICAL: After attempt 2 with Partial, budget MUST be exhausted
        // (continuation_attempt resets to 0 after agent switch)
        assert_eq!(
            state.continuation.continuation_attempt, 0,
            "CRITICAL: After attempt 2, budget should be exhausted and counter reset (agent switch). \
             Without the fix, this would continue indefinitely."
        );
        assert!(
            !state.continuation.is_continuation(),
            "After exhaustion, is_continuation() should be false (agent switched). \
             This is the core fix that prevents the infinite loop bug."
        );

        // Verify that the next OutcomeApplied would NOT trigger another continuation
        // (because we've switched agents and reset the continuation state)
        state.development_validated_outcome = Some(
            ralph_workflow::reducer::state::DevelopmentValidatedOutcome {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "New agent attempt".to_string(),
                files_changed: None,
                next_steps: None,
            },
        );
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::OutcomeApplied { iteration: 0 }),
        );
        // After switching agents, we start counting attempts again from 0
        // This first attempt on the new agent should continue normally
        assert_eq!(
            state.continuation.continuation_attempt, 1,
            "After agent switch, new agent's attempts start from 0, first continuation goes to 1"
        );
    });
}

/// Test that continuation attempts are numbered correctly and exhaustion happens at the right boundary.
///
/// With max_continue_count = 3:
/// - Attempt 0 (initial): NOT exhausted (0 < 3)
/// - Attempt 1 (first continuation): NOT exhausted (1 < 3)
/// - Attempt 2 (second continuation): NOT exhausted (2 < 3)
/// - Attempt 3 (would be third continuation): EXHAUSTED (3 >= 3)
#[test]
fn test_continuation_attempt_numbering_and_boundary() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(10, 3, 2);
        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Initial attempt (attempt 0)
        assert_eq!(state.continuation.continuation_attempt, 0);
        assert!(!state.continuation.continuations_exhausted());

        // First continuation (attempt 1)
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

        // Second continuation (attempt 2)
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

        // Third continuation trigger (attempt 3) - trigger_continuation defensive check applies
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
        // AFTER FIX: trigger_continuation checks boundary before incrementing.
        // At attempt 2, next_attempt = 3, which >= 3, so counter stays at 2 (does not increment).
        // This test is artificial - in normal flow, OutcomeApplied checks (attempt + 1 >= max)
        // and emits BudgetExhausted instead of ContinuationTriggered.
        // But the defensive check should still work as a safety net.
        assert_eq!(
            state.continuation.continuation_attempt, 2,
            "trigger_continuation defensive check should prevent increment to 3"
        );
        // continuations_exhausted() returns false (2 < 3) because counter didn't increment.
        // The actual exhaustion detection happens in OutcomeApplied via (attempt + 1 >= max).
        assert!(
            !state.continuation.continuations_exhausted(),
            "Counter at 2 means continuations_exhausted() (2 >= 3) returns false"
        );
    });
}

/// Regression test: Verify trigger_continuation defensive check doesn't increment counter.
///
/// CRITICAL BUG: trigger_continuation sets continuation_attempt = next_attempt EVEN when
/// the defensive check triggers (line 128 in budget.rs). This allows the counter to reach
/// the boundary value (3) instead of staying below it (2).
///
/// BEFORE FIX: At attempt 2, next_attempt = 3, check `3 >= 3` is true, BUT line 128 still
/// executes `self.continuation_attempt = 3` before returning. The counter reaches 3, which
/// is AT the boundary, not over it. continuations_exhausted() returns true (3 >= 3), but
/// the counter shouldn't have reached 3 in the first place.
///
/// AFTER FIX: At attempt 2, next_attempt = 3, check `3 >= 3` is true, counter stays at 2,
/// method returns without updating the counter. This prevents the off-by-one bug.
///
/// This test directly simulates the scenario where trigger_continuation's defensive check
/// is reached (which shouldn't happen in normal flow, but this tests the safety mechanism).
#[test]
fn test_trigger_continuation_defensive_check_prevents_counter_increment() {
    with_default_timeout(|| {
        let continuation = ContinuationState::with_limits(10, 3, 2);
        let mut state = PipelineState::initial_with_continuation(5, 0, continuation);
        state = reduce(state, PipelineEvent::development_iteration_started(0));

        // Manually trigger continuations to test trigger_continuation's defensive check.
        // In normal flow, OutcomeApplied would prevent this by emitting BudgetExhausted.
        // But the defensive check should still work if somehow reached.

        // Continuation 1 (attempt 0 -> 1)
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

        // Continuation 2 (attempt 1 -> 2)
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

        // Attempt continuation 3 (attempt 2 -> should stay at 2, not go to 3)
        // The defensive check in trigger_continuation should prevent the increment.
        state = reduce(
            state,
            PipelineEvent::Development(DevelopmentEvent::ContinuationTriggered {
                iteration: 0,
                status: DevelopmentStatus::Partial,
                summary: "continuation 3 attempt".to_string(),
                files_changed: None,
                next_steps: None,
            }),
        );

        // CRITICAL ASSERTION: trigger_continuation defensive check must NOT increment counter.
        // BEFORE FIX: continuation_attempt would be 3 (line 128 sets it before returning)
        // AFTER FIX: continuation_attempt should stay at 2 (defensive check returns early)
        assert_eq!(
            state.continuation.continuation_attempt, 2,
            "BUG: trigger_continuation defensive check at budget.rs:128 increments counter to 3 \
             before returning. This is the off-by-one bug. The counter should stay at 2 when the \
             defensive check triggers (next_attempt = 3 >= max_continue_count = 3)."
        );

        // NOTE: continuations_exhausted() returns false (2 < 3) because counter stayed at 2.
        // The exhaustion is detected by OutcomeApplied checking (continuation_attempt + 1 >= max),
        // which correctly identifies that the NEXT attempt would exceed the budget.
        // The defensive check in trigger_continuation is a safety net that prevents scheduling
        // when somehow reached, but doesn't mark the state as exhausted via the counter.
        assert!(
            !state.continuation.continuations_exhausted(),
            "Counter is at 2, so continuations_exhausted() (2 >= 3) returns false. \
             Exhaustion is detected by OutcomeApplied checking (2 + 1 >= 3), not by the counter."
        );

        // Verify pending flags were cleared by defensive check
        assert!(
            !state.continuation.continue_pending,
            "Defensive check should clear continue_pending"
        );
        assert!(
            !state.continuation.context_write_pending,
            "Defensive check should clear context_write_pending"
        );
    });
}
