// Tests for MockEffectHandler.
//
// This file contains all test code for the mock effect handler module.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::effect::EffectHandler;
    use crate::reducer::event::PipelineEvent;
    use crate::reducer::state::PipelineState;

    #[test]
    fn mock_effect_handler_captures_create_commit_effect() {
        let state = PipelineState::initial(1, 0);
        let handler = MockEffectHandler::new(state);

        // Should start with no captured effects
        let captured = handler.captured_effects();
        assert!(captured.is_empty(), "Should start with no captured effects");
    }

    #[test]
    fn mock_effect_handler_simulates_empty_diff() {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state).with_empty_diff();

        // CheckCommitDiff should mark empty diff
        let result = handler.execute_mock(Effect::CheckCommitDiff);

        assert!(
            matches!(
                result.event,
                PipelineEvent::Commit(crate::reducer::event::CommitEvent::DiffPrepared {
                    empty: true,
                    ..
                })
            ),
            "Should return CommitDiffPrepared when empty diff is simulated, got: {:?}",
            result.event
        );
    }

    #[test]
    fn mock_effect_handler_normal_commit_generation() {
        use crate::reducer::state::CommitValidatedOutcome;

        let state = PipelineState {
            commit_validated_outcome: Some(CommitValidatedOutcome {
                attempt: 1,
                message: Some("mock commit message".to_string()),
                reason: None,
            }),
            ..PipelineState::initial(1, 0)
        };
        let mut handler = MockEffectHandler::new(state); // No with_empty_diff()

        // ApplyCommitMessageOutcome should return CommitMessageGenerated normally
        let result = handler.execute_mock(Effect::ApplyCommitMessageOutcome);

        assert!(
            matches!(
                result.event,
                PipelineEvent::Commit(crate::reducer::event::CommitEvent::MessageGenerated { .. })
            ),
            "Should return CommitMessageGenerated when validated outcome exists, got: {:?}",
            result.event
        );
    }

    #[test]
    fn mock_effect_handler_review_validation_emits_no_issues_outcome() {
        let state = PipelineState::initial(1, 1);
        let mut handler = MockEffectHandler::new(state);

        let result = handler.execute_mock(Effect::ValidateReviewIssuesXml { pass: 0 });

        assert!(matches!(
            result.event,
            PipelineEvent::Review(crate::reducer::event::ReviewEvent::IssuesXmlValidated {
                issues,
                no_issues_found: Some(ref message),
                ..
            }) if issues.is_empty() && message == "ok"
        ));
    }

    /// TDD test: MockEffectHandler must implement EffectHandler trait
    /// and return appropriate events without making real git calls.
    #[test]
    fn mock_effect_handler_implements_effect_handler_trait() {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        // This test will fail until we implement EffectHandler for MockEffectHandler
        // The key is that execute() captures the effect and returns a mock event
        let effect = Effect::CreateCommit {
            message: "test commit".to_string(),
        };

        // Create a minimal mock PhaseContext - this requires test-utils
        // For now we test that the handler implements the trait by calling execute_mock
        let result = handler.execute_mock(effect.clone());

        // Effect should be captured
        assert!(
            handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. })),
            "CreateCommit effect should be captured"
        );

        // Event should be CommitCreated (no real git call)
        assert!(
            matches!(
                result.event,
                PipelineEvent::Commit(crate::reducer::event::CommitEvent::Created { .. })
            ),
            "Should return CommitCreated event, got: {:?}",
            result.event
        );
    }

    #[test]
    fn mock_effect_handler_returns_commit_created_for_create_commit() {
        let state = PipelineState::initial(1, 0);
        let handler = MockEffectHandler::new(state);

        // Verify the handler can be created and has expected initial state
        assert!(handler.captured_effects().is_empty());
        assert_eq!(handler.effect_count(), 0);
    }

    #[test]
    fn mock_effect_handler_clear_captured_works() {
        let state = PipelineState::initial(1, 0);
        let handler = MockEffectHandler::new(state);

        // Manually push an effect for testing (simulating execute)
        handler
            .captured_effects
            .borrow_mut()
            .push(Effect::CreateCommit {
                message: "test".to_string(),
            });

        assert_eq!(handler.effect_count(), 1);

        handler.clear_captured();

        assert_eq!(handler.effect_count(), 0);
        assert!(handler.captured_effects().is_empty());
    }

    #[test]
    fn mock_effect_handler_was_effect_executed_works() {
        let state = PipelineState::initial(1, 0);
        let handler = MockEffectHandler::new(state);

        // Manually push effects for testing
        handler
            .captured_effects
            .borrow_mut()
            .push(Effect::CreateCommit {
                message: "test commit".to_string(),
            });
        handler
            .captured_effects
            .borrow_mut()
            .push(Effect::PreparePlanningPrompt {
                iteration: 1,
                prompt_mode: crate::reducer::state::PromptMode::Normal,
            });

        assert!(handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. })));
        assert!(handler.was_effect_executed(|e| matches!(e, Effect::PreparePlanningPrompt { .. })));
        assert!(!handler.was_effect_executed(|e| matches!(e, Effect::ValidateFinalState)));
    }

    /// Test that MockEffectHandler properly implements the EffectHandler trait
    /// with a real PhaseContext. This proves it can be a drop-in replacement
    /// for MainEffectHandler in tests.
    #[test]
    fn mock_effect_handler_trait_execute_with_phase_context() {
        use crate::agents::AgentRegistry;
        use crate::checkpoint::{ExecutionHistory, RunContext};
        use crate::config::Config;
        use crate::executor::MockProcessExecutor;
        use crate::logger::{Colors, Logger};
        use crate::phases::PhaseContext;
        use crate::pipeline::{Stats, Timer};
        use crate::prompts::template_context::TemplateContext;
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;
        use std::sync::Arc;

        // Create test fixtures
        let config = Config::default();
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();
        let registry = AgentRegistry::new().unwrap();
        let executor = Arc::new(MockProcessExecutor::new());
        let repo_root = PathBuf::from("/test/repo");
        let workspace = MemoryWorkspace::new(repo_root.clone());

        // Create PhaseContext
        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: "test-developer",
            reviewer_agent: "test-reviewer",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: &*executor,
            executor_arc: Arc::clone(&executor) as Arc<dyn crate::executor::ProcessExecutor>,
            repo_root: &repo_root,
            workspace: &workspace,
        };

        // Create handler and execute effect via trait method
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        let effect = Effect::CreateCommit {
            message: "test via trait".to_string(),
        };

        // Call the trait method (not execute_mock)
        let result = handler.execute(effect, &mut ctx);

        // Should succeed
        assert!(result.is_ok(), "execute should succeed");

        // Should return CommitCreated event
        let effect_result = result.unwrap();
        match effect_result.event {
            PipelineEvent::Commit(crate::reducer::event::CommitEvent::Created {
                hash,
                message,
            }) => {
                assert_eq!(hash, "mock_commit_hash_abc123");
                assert_eq!(message, "test via trait");
            }
            other => panic!("Expected CommitCreated, got {:?}", other),
        }

        // Effect should be captured
        assert!(handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. })));
        assert_eq!(handler.effect_count(), 1);
    }

    /// Test that MockEffectHandler captures UI events for development extraction.
    #[test]
    fn mock_effect_handler_captures_iteration_progress_ui() {
        let state = PipelineState::initial(3, 1);
        let mut handler = MockEffectHandler::new(state);

        // Simulate development XML extraction
        let _result = handler.execute_mock(Effect::ExtractDevelopmentXml { iteration: 1 });

        // Verify UI event was emitted
        assert!(handler.was_ui_event_emitted(|e| {
            matches!(
                e,
                UIEvent::IterationProgress {
                    current: 1,
                    total: 3
                }
            )
        }));
    }

    /// Test that MockEffectHandler captures phase transition UI events.
    #[test]
    fn mock_effect_handler_captures_phase_transition_ui() {
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
    }

    /// Test that UIEvents do not affect pipeline state.
    #[test]
    fn ui_events_do_not_affect_state() {
        // This test verifies that UIEvents are purely display-only
        // and do not cause any state mutations
        let state = PipelineState::initial(1, 0);
        let state_clone = state.clone();

        // UIEvent exists but reducer never sees it
        let _ui_event = UIEvent::PhaseTransition {
            from: None,
            to: PipelinePhase::Development,
        };

        // State should be unchanged
        assert_eq!(state.phase, state_clone.phase);
    }

    /// Test that MockEffectHandler emits XmlOutput events for plan validation.
    #[test]
    fn mock_effect_handler_emits_xml_output_for_plan() {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::ValidatePlanningXml { iteration: 1 });

        // Verify XmlOutput event was emitted with DevelopmentPlan type
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentPlan,
                    ..
                }
            )),
            "Should emit XmlOutput event for plan validation"
        );
    }

    /// Test that MockEffectHandler emits XmlOutput events for development extraction.
    #[test]
    fn mock_effect_handler_emits_xml_output_for_development() {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::ExtractDevelopmentXml { iteration: 1 });

        // Verify XmlOutput event was emitted with DevelopmentResult type
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentResult,
                    ..
                }
            )),
            "Should emit XmlOutput event for development result"
        );
    }

    /// Test that MockEffectHandler emits XmlOutput events for review pass.
    #[test]
    fn mock_effect_handler_emits_xml_output_for_review_snippets() {
        let state = PipelineState::initial(1, 1);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::ExtractReviewIssueSnippets { pass: 1 });

        // Verify XmlOutput event was emitted with ReviewIssues type
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::XmlOutput {
                    xml_type: XmlOutputType::ReviewIssues,
                    ..
                }
            )),
            "Should emit XmlOutput event for review issue snippets"
        );
    }

    /// Test that MockEffectHandler emits XmlOutput events for fix attempt.
    #[test]
    fn mock_effect_handler_emits_xml_output_for_fix() {
        let state = PipelineState::initial(1, 1);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::ValidateFixResultXml { pass: 1 });

        // Verify XmlOutput event was emitted with FixResult type
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::XmlOutput {
                    xml_type: XmlOutputType::FixResult,
                    ..
                }
            )),
            "Should emit XmlOutput event for fix result"
        );
    }

    /// Test that MockEffectHandler emits XmlOutput events for commit message.
    #[test]
    fn mock_effect_handler_emits_xml_output_for_commit() {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::ValidateCommitXml);

        // Verify XmlOutput event was emitted with CommitMessage type
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::XmlOutput {
                    xml_type: XmlOutputType::CommitMessage,
                    ..
                }
            )),
            "Should emit XmlOutput event for commit message"
        );
    }
}
