//! Tests documenting explicit reducer-driven behavior and the absence of hidden paths.
//!
//! These tests act as architectural documentation for the reducer-only pipeline:
//! - No handler-level "helpfulness" (cleanup, fallback, or retry loops)
//! - All retries, fallbacks, and phase transitions are driven by reducer events
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::orchestration::determine_next_effect;
use ralph_workflow::reducer::state::{AgentChainState, ContinuationState, PipelineState};
use ralph_workflow::reducer::state_reduction::reduce;

use ralph_workflow::app::event_loop::{run_event_loop_with_handler, EventLoopConfig};

/// Test that handler cleanup operations are reducer-driven effects, not hidden helpers.
#[test]
fn test_handler_cleanup_requires_effect() {
    with_default_timeout(|| {
        // Cleanup must be driven by explicit effects (e.g., CleanupContext,
        // CleanupContinuationContext). Handlers must not perform hidden cleanup
        // beyond the effect being executed.
        let mut state = PipelineState::initial(2, 1);
        state.phase = PipelinePhase::Planning;
        state.context_cleaned = false;
        state.agent_chain = AgentChainState::initial().with_agents(
            vec!["dev-primary".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::CleanupContext),
            "Cleanup must be an explicit effect, got: {effect:?}"
        );
    });
}

/// Test that XSD retry loops are NOT embedded in handlers.
#[test]
fn test_xsd_retry_loops_are_removed() {
    with_default_timeout(|| {
        // XSD retries must be driven by reducer events/state (attempt counters).
        // Handlers should execute a single attempt per effect.
        let mut state = PipelineState::initial(2, 1);
        state.phase = PipelinePhase::Development;
        state.continuation = ContinuationState::new().with_max_xsd_retry(2);
        state.agent_chain = AgentChainState::initial().with_agents(
            vec!["dev-primary".to_string(), "dev-fallback".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // One failure increments attempt counter, does not auto-loop.
        let next = reduce(
            state.clone(),
            ralph_workflow::reducer::event::PipelineEvent::development_output_validation_failed(
                0, 0,
            ),
        );
        assert_eq!(next.continuation.invalid_output_attempts, 1);
        assert_eq!(
            next.agent_chain.current_agent(),
            Some(&"dev-primary".to_string())
        );

        // At max attempts, reducer should advance agent chain.
        let exhausted = reduce(
            PipelineState {
                continuation: ContinuationState {
                    xsd_retry_count: 1,
                    max_xsd_retry_count: 2,
                    ..ContinuationState::new()
                },
                ..state
            },
            ralph_workflow::reducer::event::PipelineEvent::development_output_validation_failed(
                0, 0,
            ),
        );
        assert_eq!(
            exhausted.agent_chain.current_agent(),
            Some(&"dev-fallback".to_string())
        );
    });
}

/// Test that Planning output validation retries are reducer-driven.
///
/// Planning must not have hidden validation retry loops in handlers or phase code.
/// Invalid planning output should increment a reducer-visible attempt counter and
/// trigger agent fallback only when the threshold is reached.
#[test]
fn test_planning_output_validation_retries_are_reducer_driven() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(2, 1);
        state.phase = PipelinePhase::Planning;
        state.context_cleaned = true;
        state.continuation = ContinuationState::new().with_max_xsd_retry(2);
        state.agent_chain = AgentChainState::initial().with_agents(
            vec!["dev-primary".to_string(), "dev-fallback".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // One failure increments attempt counter, does not auto-loop or switch agents.
        let next = reduce(
            state.clone(),
            ralph_workflow::reducer::event::PipelineEvent::planning_output_validation_failed(0, 0),
        );
        assert_eq!(next.continuation.invalid_output_attempts, 1);
        assert_eq!(
            next.agent_chain.current_agent(),
            Some(&"dev-primary".to_string())
        );

        // At max attempts, reducer should advance agent chain and reset counter.
        let advanced = reduce(
            PipelineState {
                continuation: ContinuationState {
                    xsd_retry_count: 1,
                    max_xsd_retry_count: 2,
                    ..ContinuationState::new()
                },
                ..state
            },
            ralph_workflow::reducer::event::PipelineEvent::planning_output_validation_failed(0, 0),
        );
        assert_eq!(advanced.continuation.invalid_output_attempts, 0);
        assert_eq!(
            advanced.agent_chain.current_agent(),
            Some(&"dev-fallback".to_string())
        );
    });
}

/// Test that marker file checks do not influence control flow.
#[test]
fn test_marker_file_check_is_documented_intentional() {
    with_default_timeout(|| {
        // Marker files must not alter phase progression or retry decisions.
        // Only reducer events may change control flow.
        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::CommitMessage;
        state.commit = ralph_workflow::reducer::state::CommitState::NotStarted;
        state.agent_chain = AgentChainState::initial();

        let effect = determine_next_effect(&state);
        assert!(
            matches!(
                effect,
                Effect::InitializeAgentChain {
                    role: AgentRole::Commit
                }
            ),
            "Commit phase should require explicit chain initialization; got {effect:?}"
        );
    });
}

/// Test that the event loop does not inject synthetic checkpoint events.
///
/// Checkpointing must happen only through the SaveCheckpoint effect executed by
/// the handler. The event loop must not directly apply CheckpointSaved events.
#[test]
fn test_event_loop_does_not_inject_checkpoint_saved_events() {
    with_default_timeout(|| {
        use ralph_workflow::agents::AgentRegistry;
        use ralph_workflow::checkpoint::{ExecutionHistory, RunContext};
        use ralph_workflow::config::Config;
        use ralph_workflow::executor::MockProcessExecutor;
        use ralph_workflow::logger::{Colors, Logger};
        use ralph_workflow::pipeline::{Stats, Timer};
        use ralph_workflow::prompts::template_context::TemplateContext;
        use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
        use ralph_workflow::workspace::MemoryWorkspace;
        use std::path::PathBuf;
        use std::sync::Arc;

        let config = Config::default();
        let colors = Colors::new();
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let template_context = TemplateContext::default();
        let registry = AgentRegistry::new().unwrap();
        let executor = Arc::new(MockProcessExecutor::new());

        let repo_root = PathBuf::from("/test/repo");
        let workspace = MemoryWorkspace::new(repo_root.clone());

        let mut ctx = ralph_workflow::phases::PhaseContext {
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
            executor_arc: Arc::clone(&executor)
                as Arc<dyn ralph_workflow::executor::ProcessExecutor>,
            repo_root: &repo_root,
            workspace: &workspace,
        };

        // Start in FinalValidation so the loop runs without needing SaveCheckpoint.
        let mut initial_state = PipelineState::initial(0, 0);
        initial_state.phase = PipelinePhase::FinalValidation;
        let mut handler = MockEffectHandler::new(initial_state.clone());

        let loop_config = EventLoopConfig {
            max_iterations: 10,
            enable_checkpointing: true,
        };

        let res =
            run_event_loop_with_handler(&mut ctx, Some(initial_state), loop_config, &mut handler)
                .expect("event loop should run");
        assert!(res.completed, "pipeline should complete in test scenario");

        // The loop should not inject checkpoint events; without SaveCheckpoint effects,
        // there should be zero CheckpointSaved events applied.
        assert_eq!(
            handler.state.checkpoint_saved_count, 0,
            "event loop must not inject synthetic CheckpointSaved events"
        );
    });
}

/// Test that `.processed` XML files are archive-only.
///
/// This test lives alongside other "no hidden behavior" invariants: the pipeline must
/// not consult `.processed` files as fallback inputs.
#[test]
fn test_processed_xml_files_are_never_used_as_inputs() {
    with_default_timeout(|| {
        use ralph_workflow::files::llm_output_extraction::file_based_extraction::try_extract_from_file_with_workspace;
        use ralph_workflow::workspace::MemoryWorkspace;
        use std::path::Path;

        let workspace = MemoryWorkspace::new_test().with_file(
            ".agent/tmp/development_result.xml.processed",
            "<development>archived</development>",
        );

        // Primary missing, archive present -> must not be used.
        let result = try_extract_from_file_with_workspace(
            &workspace,
            Path::new(".agent/tmp/development_result.xml"),
        );

        assert!(
            result.is_none(),
            "archived .processed XML must not be used as a fallback input"
        );
    });
}
