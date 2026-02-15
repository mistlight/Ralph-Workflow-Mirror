//! Tests covering reducer-visible state reset semantics for recovery escalation.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::reducer::event::{AwaitingDevFixEvent, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

/// State reset behavior across escalation levels.
#[test]
fn recovery_state_reset_at_each_level() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.iteration = 1;
        state.failed_phase_for_recovery = Some(PipelinePhase::Planning);
        state.dev_fix_attempt_count = 0;
        state.recovery_escalation_level = 0;

        state.planning_prompt_prepared_iteration = Some(1);
        state.planning_agent_invoked_iteration = Some(1);
        state.planning_xml_extracted_iteration = Some(1);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
            success: true,
            summary: Some("Attempt 1".to_string()),
        });
        state = reduce(state, event);
        assert_eq!(state.recovery_escalation_level, 1);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 1,
            attempt_count: 1,
            target_phase: PipelinePhase::Planning,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.planning_prompt_prepared_iteration, Some(1));
        assert_eq!(state.planning_agent_invoked_iteration, Some(1));
        assert_eq!(state.iteration, 1);

        state.phase = PipelinePhase::AwaitingDevFix;
        state.planning_prompt_prepared_iteration = Some(1);
        state.planning_agent_invoked_iteration = Some(1);

        for i in 2..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 2);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
            target_phase: PipelinePhase::Planning,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.planning_prompt_prepared_iteration, None);
        assert_eq!(state.planning_agent_invoked_iteration, None);
        assert_eq!(state.planning_xml_extracted_iteration, None);
        assert_eq!(state.iteration, 1);

        state.phase = PipelinePhase::AwaitingDevFix;
        state.iteration = 2;

        for i in 5..=7 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 3);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 3,
            attempt_count: 7,
            target_phase: PipelinePhase::Planning,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 1);
        assert_eq!(state.planning_prompt_prepared_iteration, None);

        state.phase = PipelinePhase::AwaitingDevFix;
        state.iteration = 3;

        for i in 8..=10 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 4);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 4,
            attempt_count: 10,
            target_phase: PipelinePhase::Planning,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 0);
        assert_eq!(state.planning_prompt_prepared_iteration, None);
    });
}

#[test]
fn recovery_clears_development_flags() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);
        state.iteration = 2;

        state.development_context_prepared_iteration = Some(2);
        state.development_prompt_prepared_iteration = Some(2);
        state.development_agent_invoked_iteration = Some(2);
        state.analysis_agent_invoked_iteration = Some(2);
        state.development_xml_extracted_iteration = Some(2);

        for i in 1..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 2);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
            target_phase: PipelinePhase::Development,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::Development);
        assert_eq!(state.development_context_prepared_iteration, None);
        assert_eq!(state.development_prompt_prepared_iteration, None);
        assert_eq!(state.development_agent_invoked_iteration, None);
        assert_eq!(state.analysis_agent_invoked_iteration, None);
        assert_eq!(state.development_xml_extracted_iteration, None);
        assert_eq!(state.iteration, 2);
    });
}

#[test]
fn recovery_clears_review_flags() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 3);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Review);
        state.reviewer_pass = 1;

        state.review_context_prepared_pass = Some(1);
        state.review_prompt_prepared_pass = Some(1);
        state.review_agent_invoked_pass = Some(1);
        state.review_issues_xml_extracted_pass = Some(1);
        state.review_issues_found = true;
        state.fix_prompt_prepared_pass = Some(1);
        state.fix_agent_invoked_pass = Some(1);

        for i in 1..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 2);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
            target_phase: PipelinePhase::Review,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::Review);
        assert_eq!(state.review_context_prepared_pass, None);
        assert_eq!(state.review_prompt_prepared_pass, None);
        assert_eq!(state.review_agent_invoked_pass, None);
        assert_eq!(state.review_issues_xml_extracted_pass, None);
        assert!(!state.review_issues_found);
        assert_eq!(state.fix_prompt_prepared_pass, None);
        assert_eq!(state.fix_agent_invoked_pass, None);
        assert_eq!(state.reviewer_pass, 1);
    });
}

#[test]
fn recovery_clears_commit_flags() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::CommitMessage);
        state.iteration = 3;

        state.commit_prompt_prepared = true;
        state.commit_diff_prepared = true;
        state.commit_diff_empty = false;
        state.commit_diff_content_id_sha256 = Some("abc123".to_string());
        state.commit_agent_invoked = true;
        state.commit_xml_cleaned = true;

        for i in 1..=4 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 2);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 2,
            attempt_count: 4,
            target_phase: PipelinePhase::CommitMessage,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::CommitMessage);
        assert!(!state.commit_prompt_prepared);
        assert!(!state.commit_diff_prepared);
        assert!(!state.commit_diff_empty);
        assert_eq!(state.commit_diff_content_id_sha256, None);
        assert!(!state.commit_agent_invoked);
        assert!(!state.commit_xml_cleaned);
        assert_eq!(state.iteration, 3);
    });
}

#[test]
fn recovery_iteration_reset_floor_at_zero() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.failed_phase_for_recovery = Some(PipelinePhase::Planning);
        state.iteration = 0;

        for i in 1..=7 {
            let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted {
                success: true,
                summary: Some(format!("Attempt {}", i)),
            });
            state = reduce(state, event);
        }
        assert_eq!(state.recovery_escalation_level, 3);

        let event = PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::RecoveryAttempted {
            level: 3,
            attempt_count: 7,
            target_phase: PipelinePhase::Planning,
        });
        state = reduce(state, event);

        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.iteration, 0);
    });
}
