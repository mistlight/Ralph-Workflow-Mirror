//! Integration tests for premature exit prevention.
//!
//! Verifies that the reducer does not transition to Complete phase
//! unless all configured work is satisfied.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (phase transitions, iteration/pass counters)
//! - Tests are deterministic and isolated
//! - Tests behavior, not implementation details

use ralph_workflow::reducer::event::{PipelineEvent, PipelinePhase, ReviewEvent};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

use crate::test_timeout::with_default_timeout;

#[test]
fn test_pipeline_does_not_complete_before_all_dev_iterations() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(5, 0);

        // Simulate 3 iterations completing
        for i in 0..3 {
            state = reduce(state, PipelineEvent::development_iteration_started(i));
            state = reduce(
                state,
                PipelineEvent::development_iteration_completed(i, true),
            );
        }

        // Should NOT be in Complete or FinalValidation phase yet
        assert_ne!(state.phase, PipelinePhase::Complete);
        assert_ne!(state.phase, PipelinePhase::FinalValidation);
        assert_eq!(state.metrics.dev_iterations_completed, 3);
        assert!(state.iteration < state.total_iterations);
    });
}

#[test]
fn test_pipeline_completes_after_all_dev_iterations_no_review() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);

        // Simulate all 3 iterations completing with commits
        for i in 0..3 {
            state = reduce(state, PipelineEvent::development_iteration_started(i));
            state = reduce(
                state,
                PipelineEvent::development_iteration_completed(i, true),
            );
            // Create commit after each iteration
            state = reduce(
                state,
                PipelineEvent::commit_created(format!("hash{}", i), format!("Commit {}", i)),
            );
        }

        // After last commit with 0 review passes configured, should go to FinalValidation
        assert_eq!(state.phase, PipelinePhase::FinalValidation);
        assert_eq!(state.metrics.dev_iterations_completed, 3);
    });
}

#[test]
fn test_pipeline_transitions_to_review_after_all_dev_iterations() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(2, 3);

        // Simulate all 2 dev iterations completing
        for i in 0..2 {
            state = reduce(state, PipelineEvent::development_iteration_started(i));
            state = reduce(
                state,
                PipelineEvent::development_iteration_completed(i, true),
            );
            // Simulate commit created (to advance to next iteration)
            state = reduce(
                state,
                PipelineEvent::commit_created(
                    format!("hash{}", i),
                    format!("Commit message {}", i),
                ),
            );
        }

        // Should transition to Review phase (3 review passes configured)
        assert_eq!(state.phase, PipelinePhase::Review);
        assert_eq!(state.metrics.dev_iterations_completed, 2);
    });
}

#[test]
fn test_pipeline_does_not_complete_before_all_review_passes() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(0, 5);
        state.phase = PipelinePhase::Review;

        // Simulate 2 review passes completing clean
        for pass in 1..=2 {
            state = reduce(state, PipelineEvent::review_pass_started(pass));
            state = reduce(
                state,
                PipelineEvent::Review(ReviewEvent::PassCompletedClean { pass }),
            );
        }

        // Should NOT be in Complete or FinalValidation phase yet
        assert_ne!(state.phase, PipelinePhase::Complete);
        assert_ne!(state.phase, PipelinePhase::FinalValidation);
        assert_eq!(state.metrics.review_passes_completed, 2);
        assert!(state.reviewer_pass < state.total_reviewer_passes);
    });
}

#[test]
fn test_pipeline_completes_after_all_review_passes() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(0, 3);
        state.phase = PipelinePhase::Review;

        // Simulate all 3 review passes completing clean
        for pass in 1..=3 {
            state = reduce(state, PipelineEvent::review_pass_started(pass));
            state = reduce(
                state,
                PipelineEvent::Review(ReviewEvent::PassCompletedClean { pass }),
            );
        }

        // Should transition to CommitMessage phase (last pass completed)
        assert_eq!(state.phase, PipelinePhase::CommitMessage);
        assert_eq!(state.metrics.review_passes_completed, 3);
        assert_eq!(state.reviewer_pass, 4); // Advanced past last pass
    });
}

#[test]
fn test_pipeline_continues_review_after_fix() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(0, 3);
        state.phase = PipelinePhase::Review;

        // Pass 1: Issues found
        state = reduce(state, PipelineEvent::review_pass_started(1));
        state = reduce(state, PipelineEvent::review_agent_invoked(1));
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::Completed {
                pass: 1,
                issues_found: true,
            }),
        );

        // Fix applied
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::FixAttemptCompleted {
                pass: 1,
                changes_made: true,
            }),
        );

        // Should transition to CommitMessage to commit the fix
        assert_eq!(state.phase, PipelinePhase::CommitMessage);
        assert_eq!(state.metrics.review_passes_completed, 1);

        // Simulate commit
        state = reduce(
            state,
            PipelineEvent::commit_created("hash1".to_string(), "Fix issues".to_string()),
        );

        // Should continue to next review pass, not exit
        assert_eq!(state.phase, PipelinePhase::Review);
        assert_eq!(state.reviewer_pass, 2);
        assert_ne!(state.phase, PipelinePhase::Complete);
    });
}

// ============================================================================
// Step 15: Edge-case nesting rules (review passes vs fix continuations)
// ============================================================================

/// Test that fix continuations do NOT accidentally advance to next pass.
///
/// CRITICAL: Fix continuations are attempts within the same pass, not new passes.
#[test]
fn test_fix_continuation_does_not_advance_pass() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::FixStatus;

        // Given: State at review pass 0 (configured for 2 passes total)
        let mut state = PipelineState::initial(0, 2);
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;

        // Start the review pass
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PassStarted { pass: 0 }),
        );
        // NOTE: review_passes_started increments when pass != current reviewer_pass.
        // Initial PipelineState has reviewer_pass = 0, so starting pass 0 is not counted as a
        // "new pass" by the reducer.
        assert_eq!(state.metrics.review_passes_started, 0);

        // When: Trigger first fix continuation
        let event = PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
            pass: 0,
            status: FixStatus::IssuesRemain,
            summary: Some("partial fixes".to_string()),
        });
        state = reduce(state, event);

        // When: Trigger second fix continuation
        let event = PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
            pass: 0,
            status: FixStatus::IssuesRemain,
            summary: Some("more partial fixes".to_string()),
        });
        state = reduce(state, event);

        // Then: reviewer_pass should still be 0 (not advanced to 1)
        assert_eq!(
            state.reviewer_pass, 0,
            "Fix continuations must NOT advance reviewer_pass"
        );

        // And: fix_continuation_attempt should track continuation count
        assert_eq!(
            state.metrics.fix_continuation_attempt, 2,
            "Fix continuations should increment fix_continuation_attempt"
        );

        // And: Should still be in Review phase
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Fix continuations should keep phase as Review"
        );
    });
}

/// Test that clean pass (no issues found) correctly increments completed counter.
#[test]
fn test_clean_pass_increments_completed_counter() {
    with_default_timeout(|| {
        // Given: State at review pass 0
        let mut state = PipelineState::initial(0, 1);
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;

        // Start the review pass
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PassStarted { pass: 0 }),
        );

        assert_eq!(state.metrics.review_passes_completed, 0);

        // When: Review finds no issues (clean pass)
        let event = PipelineEvent::Review(ReviewEvent::Completed {
            pass: 0,
            issues_found: false,
        });
        let state = reduce(state, event);

        // Then: review_passes_completed should increment
        assert_eq!(
            state.metrics.review_passes_completed, 1,
            "Clean pass should increment review_passes_completed"
        );

        // And: Should advance to next phase (CommitMessage since this was the only pass)
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "After completing all review passes, should advance to commit"
        );
    });
}

/// Test that fix continuation state resets at new pass.
#[test]
fn test_fix_continuation_state_resets_at_new_pass() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::FixStatus;

        // Given: State at review pass 0 with fix continuation in progress
        let mut state = PipelineState::initial(0, 2);
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 0;

        // Start pass 0
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PassStarted { pass: 0 }),
        );

        // Trigger fix continuation in pass 0
        let event = PipelineEvent::Review(ReviewEvent::FixContinuationTriggered {
            pass: 0,
            status: FixStatus::IssuesRemain,
            summary: Some("partial fixes".to_string()),
        });
        let mut state = reduce(state, event);

        assert_eq!(state.metrics.fix_continuation_attempt, 1);

        // When: Advance to pass 1
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PassStarted { pass: 1 }),
        );

        // Then: fix_continuation_attempt should reset to 0 for the new pass
        assert_eq!(
            state.metrics.fix_continuation_attempt, 0,
            "Fix continuation attempt should reset when starting new review pass"
        );
    });
}

/// Test that pipeline completes exactly at total_reviewer_passes, not before.
#[test]
fn test_exactly_completes_at_total_reviewer_passes() {
    with_default_timeout(|| {
        // Given: Configure 2 review passes
        let mut state = PipelineState::initial(0, 2);
        state.phase = PipelinePhase::Review;

        // Run pass 0
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PassStarted { pass: 0 }),
        );
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::Completed {
                pass: 0,
                issues_found: false,
            }),
        );

        // After pass 0 completes cleanly, should advance to pass 1, not to CommitMessage
        assert_eq!(
            state.reviewer_pass, 1,
            "After completing pass 0, should advance to pass 1"
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Review,
            "Should remain in Review phase until all passes complete"
        );

        // Run pass 1
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::PassStarted { pass: 1 }),
        );
        state = reduce(
            state,
            PipelineEvent::Review(ReviewEvent::Completed {
                pass: 1,
                issues_found: false,
            }),
        );

        // After completing pass 1 (the 2nd and final pass), should advance to CommitMessage
        assert_eq!(
            state.phase,
            PipelinePhase::CommitMessage,
            "After completing all review passes, should advance to commit phase"
        );
        assert_eq!(
            state.metrics.review_passes_completed, 2,
            "Should have completed all 2 review passes"
        );
    });
}
