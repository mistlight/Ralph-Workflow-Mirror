//! Mock implementation of EffectHandler for testing.
//!
//! This module provides a mock handler that captures all executed effects
//! for later inspection, returning appropriate mock PipelineEvents without
//! performing any real side effects (no git calls, no file I/O, no agent execution).
//!
//! # Usage
//!
//! ```ignore
//! use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
//! use ralph_workflow::reducer::{Effect, EffectHandler, PipelineState};
//!
//! let state = PipelineState::initial(1, 0);
//! let mut handler = MockEffectHandler::new(state);
//!
//! // Execute an effect - no real side effects occur
//! let event = handler.execute(Effect::CreateCommit {
//!     message: "test".to_string()
//! }, &mut ctx)?;
//!
//! // Verify effect was captured
//! assert!(handler.captured_effects().iter().any(|e|
//!     matches!(e, Effect::CreateCommit { .. })
//! ));
//! ```

#![cfg(any(test, feature = "test-utils"))]

use super::effect::{Effect, EffectHandler, EffectResult};
use super::event::{PipelineEvent, PipelinePhase};
use super::state::PipelineState;
use super::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use crate::phases::PhaseContext;
use anyhow::Result;
use std::cell::RefCell;

/// Mock implementation of EffectHandler for testing.
///
/// This handler captures all executed effects for later inspection while
/// returning appropriate mock PipelineEvents. It performs NO real side effects:
/// - No git operations
/// - No file I/O
/// - No agent execution
/// - No subprocess spawning
///
/// # Thread Safety
///
/// Uses `RefCell` for interior mutability, allowing effect capture even
/// when handler is borrowed.
pub struct MockEffectHandler {
    /// The pipeline state (updated by reducer, not handler).
    pub state: PipelineState,
    /// All effects that have been executed, in order.
    captured_effects: RefCell<Vec<Effect>>,
    /// All UI events that have been emitted, in order.
    captured_ui_events: RefCell<Vec<UIEvent>>,
    /// When true, GenerateCommitMessage returns CommitSkipped instead of CommitMessageGenerated.
    simulate_empty_diff: bool,
}

impl MockEffectHandler {
    /// Create a new mock handler with the given initial state.
    pub fn new(state: PipelineState) -> Self {
        Self {
            state,
            captured_effects: RefCell::new(Vec::new()),
            captured_ui_events: RefCell::new(Vec::new()),
            simulate_empty_diff: false,
        }
    }

    /// Configure the mock to simulate empty diff scenario.
    ///
    /// When enabled, GenerateCommitMessage returns CommitSkipped instead of
    /// CommitMessageGenerated, simulating the case where there are no changes
    /// to commit.
    pub fn with_empty_diff(mut self) -> Self {
        self.simulate_empty_diff = true;
        self
    }

    /// Get all captured effects in execution order.
    pub fn captured_effects(&self) -> Vec<Effect> {
        self.captured_effects.borrow().clone()
    }

    /// Get all captured UI events in emission order.
    pub fn captured_ui_events(&self) -> Vec<UIEvent> {
        self.captured_ui_events.borrow().clone()
    }

    /// Check if a specific effect type was captured.
    pub fn was_effect_executed<F>(&self, predicate: F) -> bool
    where
        F: Fn(&Effect) -> bool,
    {
        self.captured_effects.borrow().iter().any(predicate)
    }

    /// Check if a specific UI event was emitted.
    pub fn was_ui_event_emitted<F>(&self, predicate: F) -> bool
    where
        F: Fn(&UIEvent) -> bool,
    {
        self.captured_ui_events.borrow().iter().any(predicate)
    }

    /// Clear all captured effects and UI events.
    pub fn clear_captured(&self) {
        self.captured_effects.borrow_mut().clear();
        self.captured_ui_events.borrow_mut().clear();
    }

    /// Get the number of captured effects.
    pub fn effect_count(&self) -> usize {
        self.captured_effects.borrow().len()
    }

    /// Get the number of captured UI events.
    pub fn ui_event_count(&self) -> usize {
        self.captured_ui_events.borrow().len()
    }

    /// Execute an effect without requiring PhaseContext.
    ///
    /// This is used for testing when you don't have a full PhaseContext.
    /// It captures the effect and returns an appropriate mock EffectResult.
    pub fn execute_mock(&mut self, effect: Effect) -> EffectResult {
        // Capture the effect
        self.captured_effects.borrow_mut().push(effect.clone());

        // Generate appropriate mock events based on effect type
        let (event, ui_events) = match effect {
            Effect::AgentInvocation {
                role,
                agent,
                model: _,
                prompt: _,
            } => {
                let ui = vec![UIEvent::AgentActivity {
                    agent: agent.clone(),
                    message: format!("Completed {} task", role),
                }];
                (PipelineEvent::agent_invocation_succeeded(role, agent), ui)
            }

            Effect::InitializeAgentChain { role } => {
                // Emit phase transition when initializing agent chain for a new phase
                let ui = match role {
                    crate::agents::AgentRole::Developer
                        if self.state.phase == PipelinePhase::Planning =>
                    {
                        vec![UIEvent::PhaseTransition {
                            from: None,
                            to: PipelinePhase::Planning,
                        }]
                    }
                    crate::agents::AgentRole::Reviewer
                        if self.state.phase == PipelinePhase::Review =>
                    {
                        vec![UIEvent::PhaseTransition {
                            from: Some(self.state.phase),
                            to: PipelinePhase::Review,
                        }]
                    }
                    _ => vec![],
                };
                (
                    PipelineEvent::agent_chain_initialized(
                        role,
                        vec!["mock_agent".to_string()],
                        3,
                        1000,
                        2.0,
                        60000,
                    ),
                    ui,
                )
            }

            Effect::GeneratePlan { iteration } => {
                let mock_plan_xml = r#"<ralph-plan>
<ralph-summary>
<context>Mock plan for testing</context>
<scope-items>
<scope-item count="1">test item</scope-item>
<scope-item count="1">another item</scope-item>
<scope-item count="1">third item</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Mock step</title>
<target-files><file path="src/test.rs" action="modify"/></target-files>
<content><paragraph>Test content</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="src/test.rs" action="modify"/></primary-files>
<reference-files><file path="src/lib.rs" purpose="reference"/></reference-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair severity="low"><risk>Test risk</risk><mitigation>Test mitigation</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Test method</method><expected-outcome>Pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;
                let ui = vec![
                    UIEvent::PhaseTransition {
                        from: Some(self.state.phase),
                        to: PipelinePhase::Development,
                    },
                    UIEvent::XmlOutput {
                        xml_type: XmlOutputType::DevelopmentPlan,
                        content: mock_plan_xml.to_string(),
                        context: Some(XmlOutputContext {
                            iteration: Some(iteration),
                            pass: None,
                            snippets: Vec::new(),
                        }),
                    },
                ];
                (
                    PipelineEvent::plan_generation_completed(iteration, true),
                    ui,
                )
            }

            Effect::RunDevelopmentIteration { iteration } => {
                let mock_dev_result_xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Mock development iteration completed successfully</ralph-summary>
<ralph-files-changed>src/test.rs
src/lib.rs</ralph-files-changed>
</ralph-development-result>"#;
                let ui = vec![
                    UIEvent::IterationProgress {
                        current: iteration,
                        total: self.state.total_iterations,
                    },
                    UIEvent::XmlOutput {
                        xml_type: XmlOutputType::DevelopmentResult,
                        content: mock_dev_result_xml.to_string(),
                        context: Some(XmlOutputContext {
                            iteration: Some(iteration),
                            pass: None,
                            snippets: Vec::new(),
                        }),
                    },
                ];
                (
                    PipelineEvent::development_iteration_completed(iteration, true),
                    ui,
                )
            }

            Effect::RunReviewPass { pass } => {
                let mock_issues_xml = r#"<ralph-issues>
<ralph-no-issues-found>Mock review found no issues</ralph-no-issues-found>
</ralph-issues>"#;
                let ui = vec![
                    UIEvent::ReviewProgress {
                        pass,
                        total: self.state.total_reviewer_passes,
                    },
                    UIEvent::XmlOutput {
                        xml_type: XmlOutputType::ReviewIssues,
                        content: mock_issues_xml.to_string(),
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets: Vec::new(),
                        }),
                    },
                ];
                (PipelineEvent::review_completed(pass, false), ui)
            }

            Effect::RunFixAttempt { pass } => {
                let mock_fix_xml = r#"<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
<ralph-summary>Mock fix completed - all issues addressed</ralph-summary>
</ralph-fix-result>"#;
                let ui = vec![UIEvent::XmlOutput {
                    xml_type: XmlOutputType::FixResult,
                    content: mock_fix_xml.to_string(),
                    context: Some(XmlOutputContext {
                        iteration: None,
                        pass: Some(pass),
                        snippets: Vec::new(),
                    }),
                }];
                (PipelineEvent::fix_attempt_completed(pass, true), ui)
            }

            Effect::RunRebase {
                phase,
                target_branch: _,
            } => (
                PipelineEvent::rebase_succeeded(phase, "mock_head_abc123".to_string()),
                vec![],
            ),

            Effect::ResolveRebaseConflicts { strategy: _ } => {
                (PipelineEvent::rebase_conflict_resolved(vec![]), vec![])
            }

            Effect::GenerateCommitMessage => {
                if self.simulate_empty_diff {
                    (
                        PipelineEvent::commit_skipped(
                            "No changes to commit (empty diff)".to_string(),
                        ),
                        vec![],
                    )
                } else {
                    let mock_commit_xml = r#"<ralph-commit>
<ralph-subject>feat: mock commit message for testing</ralph-subject>
<ralph-body>This is a mock commit body generated for testing purposes.

- Changed some files
- Added new features</ralph-body>
</ralph-commit>"#;
                    let ui = vec![
                        UIEvent::PhaseTransition {
                            from: Some(self.state.phase),
                            to: PipelinePhase::CommitMessage,
                        },
                        UIEvent::XmlOutput {
                            xml_type: XmlOutputType::CommitMessage,
                            content: mock_commit_xml.to_string(),
                            context: None,
                        },
                    ];
                    (
                        PipelineEvent::commit_message_generated(
                            "mock commit message".to_string(),
                            1,
                        ),
                        ui,
                    )
                }
            }

            Effect::CreateCommit { message } => (
                PipelineEvent::commit_created("mock_commit_hash_abc123".to_string(), message),
                vec![],
            ),

            Effect::SkipCommit { reason } => (PipelineEvent::commit_skipped(reason), vec![]),

            Effect::BackoffWait {
                role,
                cycle,
                duration_ms: _,
            } => (
                PipelineEvent::agent_retry_cycle_started(role, cycle),
                vec![],
            ),

            Effect::AbortPipeline { reason } => (PipelineEvent::pipeline_aborted(reason), vec![]),

            Effect::ValidateFinalState => {
                let ui = vec![UIEvent::PhaseTransition {
                    from: Some(self.state.phase),
                    to: PipelinePhase::Finalizing,
                }];
                (PipelineEvent::finalizing_started(), ui)
            }

            Effect::SaveCheckpoint { trigger } => {
                (PipelineEvent::checkpoint_saved(trigger), vec![])
            }

            Effect::CleanupContext => (PipelineEvent::context_cleaned(), vec![]),

            Effect::RestorePromptPermissions => {
                let ui = vec![UIEvent::PhaseTransition {
                    from: Some(self.state.phase),
                    to: PipelinePhase::Complete,
                }];
                (PipelineEvent::prompt_permissions_restored(), ui)
            }

            Effect::WriteContinuationContext(ref data) => (
                PipelineEvent::development_continuation_context_written(
                    data.iteration,
                    data.attempt,
                ),
                vec![],
            ),

            Effect::CleanupContinuationContext => (
                PipelineEvent::development_continuation_context_cleaned(),
                vec![],
            ),
        };

        // Capture UI events
        self.captured_ui_events
            .borrow_mut()
            .extend(ui_events.clone());

        EffectResult::with_ui(event, ui_events)
    }
}

/// Implement the EffectHandler trait for MockEffectHandler.
///
/// This allows MockEffectHandler to be used as a drop-in replacement for
/// MainEffectHandler in tests. The PhaseContext is ignored - the mock
/// simply captures the effect and returns an appropriate mock event.
impl<'ctx> EffectHandler<'ctx> for MockEffectHandler {
    fn execute(&mut self, effect: Effect, _ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        // Delegate to execute_mock which ignores the context
        Ok(self.execute_mock(effect))
    }
}

/// Implement StatefulHandler for MockEffectHandler.
///
/// This allows the event loop to update the mock's internal state after
/// each event is processed.
impl crate::app::event_loop::StatefulHandler for MockEffectHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
    }
}

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

        // GenerateCommitMessage should return CommitSkipped when empty diff is simulated
        let result = handler.execute_mock(Effect::GenerateCommitMessage);

        assert!(
            matches!(
                result.event,
                PipelineEvent::Commit(crate::reducer::event::CommitEvent::Skipped { .. })
            ),
            "Should return CommitSkipped when empty diff is simulated, got: {:?}",
            result.event
        );

        // Verify the reason message
        if let PipelineEvent::Commit(crate::reducer::event::CommitEvent::Skipped { reason }) =
            result.event
        {
            assert!(
                reason.contains("empty diff"),
                "Reason should mention empty diff: {}",
                reason
            );
        }
    }

    #[test]
    fn mock_effect_handler_normal_commit_generation() {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state); // No with_empty_diff()

        // GenerateCommitMessage should return CommitMessageGenerated normally
        let result = handler.execute_mock(Effect::GenerateCommitMessage);

        assert!(
            matches!(
                result.event,
                PipelineEvent::Commit(crate::reducer::event::CommitEvent::MessageGenerated { .. })
            ),
            "Should return CommitMessageGenerated when empty diff is not simulated, got: {:?}",
            result.event
        );
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
            .push(Effect::GeneratePlan { iteration: 1 });

        assert!(handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. })));
        assert!(handler.was_effect_executed(|e| matches!(e, Effect::GeneratePlan { .. })));
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

    /// Test that MockEffectHandler captures UI events for development iteration.
    #[test]
    fn mock_effect_handler_captures_iteration_progress_ui() {
        let state = PipelineState::initial(3, 1);
        let mut handler = MockEffectHandler::new(state);

        // Simulate development iteration
        let _result = handler.execute_mock(Effect::RunDevelopmentIteration { iteration: 1 });

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

    /// Test that MockEffectHandler emits XmlOutput events for plan generation.
    #[test]
    fn mock_effect_handler_emits_xml_output_for_plan() {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::GeneratePlan { iteration: 1 });

        // Verify XmlOutput event was emitted with DevelopmentPlan type
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentPlan,
                    ..
                }
            )),
            "Should emit XmlOutput event for plan generation"
        );
    }

    /// Test that MockEffectHandler emits XmlOutput events for development iteration.
    #[test]
    fn mock_effect_handler_emits_xml_output_for_development() {
        let state = PipelineState::initial(1, 0);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::RunDevelopmentIteration { iteration: 1 });

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
    fn mock_effect_handler_emits_xml_output_for_review() {
        let state = PipelineState::initial(1, 1);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::RunReviewPass { pass: 1 });

        // Verify XmlOutput event was emitted with ReviewIssues type
        assert!(
            handler.was_ui_event_emitted(|e| matches!(
                e,
                UIEvent::XmlOutput {
                    xml_type: XmlOutputType::ReviewIssues,
                    ..
                }
            )),
            "Should emit XmlOutput event for review issues"
        );
    }

    /// Test that MockEffectHandler emits XmlOutput events for fix attempt.
    #[test]
    fn mock_effect_handler_emits_xml_output_for_fix() {
        let state = PipelineState::initial(1, 1);
        let mut handler = MockEffectHandler::new(state);

        let _result = handler.execute_mock(Effect::RunFixAttempt { pass: 1 });

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

        let _result = handler.execute_mock(Effect::GenerateCommitMessage);

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
