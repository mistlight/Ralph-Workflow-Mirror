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
