//! Integration tests for UIEvent emission during pipeline execution.
//!
//! These tests verify that:
//! - Phase transitions emit appropriate UIEvents
//! - Progress events are emitted during iterations
//! - UIEvents do not affect reducer state
//! - UIEvents are properly formatted for display

use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::ui_event::UIEvent;
use ralph_workflow::reducer::PipelineState;

use crate::test_timeout::with_default_timeout;

#[test]
fn test_development_iteration_emits_progress_ui() {
    with_default_timeout(|| {
        let state = PipelineState::initial(3, 1);
        let mut handler = MockEffectHandler::new(state);

        // Simulate development iteration
        let _result = handler.execute_mock(Effect::RunDevelopmentIteration { iteration: 1 });

        // Verify UI event was emitted
        assert!(
            handler.was_ui_event_emitted(|e| {
                matches!(
                    e,
                    UIEvent::IterationProgress {
                        current: 1,
                        total: 3
                    }
                )
            }),
            "Should emit IterationProgress UI event"
        );
    });
}

#[test]
fn test_review_pass_emits_progress_ui() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 3);
        let mut handler = MockEffectHandler::new(state);

        // Simulate review pass
        let _result = handler.execute_mock(Effect::RunReviewPass { pass: 2 });

        // Verify UI event was emitted
        assert!(
            handler.was_ui_event_emitted(|e| {
                matches!(e, UIEvent::ReviewProgress { pass: 2, total: 3 })
            }),
            "Should emit ReviewProgress UI event"
        );
    });
}

#[test]
fn test_phase_transition_ui_event_format() {
    with_default_timeout(|| {
        let event = UIEvent::PhaseTransition {
            from: Some(PipelinePhase::Planning),
            to: PipelinePhase::Development,
        };

        let display = event.format_for_display();
        assert!(
            display.contains("Development"),
            "Should contain phase name, got: {}",
            display
        );
    });
}

#[test]
fn test_validate_final_state_emits_phase_transition() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        // ValidateFinalState should emit phase transition to Finalizing
        let _result = handler.execute_mock(Effect::ValidateFinalState);

        // Verify UI event was emitted
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::PhaseTransition {
                    to: PipelinePhase::Finalizing,
                    ..
                }
            )),
            "Should emit phase transition UI event to Finalizing"
        );
    });
}

#[test]
fn test_restore_prompt_permissions_emits_phase_transition() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        // RestorePromptPermissions should emit phase transition to Complete
        let _result = handler.execute_mock(Effect::RestorePromptPermissions);

        // Verify UI event was emitted
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::PhaseTransition {
                    to: PipelinePhase::Complete,
                    ..
                }
            )),
            "Should emit phase transition UI event to Complete"
        );
    });
}

#[test]
fn test_ui_events_do_not_affect_reducer_state() {
    with_default_timeout(|| {
        // This test verifies that UIEvents are purely display-only
        // and do not cause any state mutations via the reducer
        use ralph_workflow::reducer::reduce;

        let initial_state = PipelineState::initial(1, 0);

        // Create a pipeline event that would normally transition state
        let event = ralph_workflow::reducer::PipelineEvent::PipelineStarted;

        // Reduce state
        let new_state = reduce(initial_state.clone(), event);

        // State should be updated based on the PipelineEvent, not any UIEvent
        // UIEvents exist separately and never go through the reducer
        assert_eq!(
            new_state.phase,
            PipelinePhase::Planning,
            "State should be updated by PipelineEvent, not UIEvent"
        );
    });
}

#[test]
fn test_ui_event_serialization_roundtrip() {
    with_default_timeout(|| {
        let event = UIEvent::IterationProgress {
            current: 5,
            total: 10,
        };

        let json = serde_json::to_string(&event).expect("Should serialize");
        let deserialized: UIEvent = serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(event, deserialized);
    });
}

#[test]
fn test_all_phase_emojis_are_defined() {
    with_default_timeout(|| {
        // Verify all phases have emojis
        let phases = [
            PipelinePhase::Planning,
            PipelinePhase::Development,
            PipelinePhase::Review,
            PipelinePhase::CommitMessage,
            PipelinePhase::FinalValidation,
            PipelinePhase::Finalizing,
            PipelinePhase::Complete,
            PipelinePhase::Interrupted,
        ];

        for phase in phases {
            let emoji = UIEvent::phase_emoji(&phase);
            assert!(!emoji.is_empty(), "Phase {:?} should have an emoji", phase);
        }
    });
}

#[test]
fn test_agent_activity_ui_event() {
    with_default_timeout(|| {
        use ralph_workflow::agents::AgentRole;

        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        // Simulate agent invocation
        let _result = handler.execute_mock(Effect::AgentInvocation {
            role: AgentRole::Developer,
            agent: "claude".to_string(),
            model: Some("claude-3".to_string()),
            prompt: "Test prompt".to_string(),
        });

        // Verify UI event was emitted
        assert!(
            handler.was_ui_event_emitted(
                |e| matches!(e, UIEvent::AgentActivity { agent, .. } if agent == "claude")
            ),
            "Should emit AgentActivity UI event"
        );
    });
}

#[test]
fn test_generate_plan_emits_phase_transition_on_success() {
    with_default_timeout(|| {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        // Simulate plan generation
        let _result = handler.execute_mock(Effect::GeneratePlan { iteration: 1 });

        // Verify phase transition UI event was emitted
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::PhaseTransition {
                    to: PipelinePhase::Development,
                    ..
                }
            )),
            "Should emit phase transition UI event to Development"
        );
    });
}

#[test]
fn test_captured_ui_events_cleared_on_clear() {
    with_default_timeout(|| {
        let state = PipelineState::initial(3, 1);
        let mut handler = MockEffectHandler::new(state);

        // Emit some UI events
        let _result = handler.execute_mock(Effect::RunDevelopmentIteration { iteration: 1 });

        // Verify UI events were captured
        assert!(
            handler.ui_event_count() > 0,
            "Should have captured UI events"
        );

        // Clear captured events
        handler.clear_captured();

        // Verify all events are cleared
        assert_eq!(handler.ui_event_count(), 0, "UI events should be cleared");
        assert_eq!(handler.effect_count(), 0, "Effects should be cleared");
    });
}
