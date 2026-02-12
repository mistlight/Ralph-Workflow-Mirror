use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::effect::{Effect, EffectHandler};
use crate::reducer::event::PipelinePhase;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::reducer::ui_event::UIEvent;
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[allow(clippy::too_many_arguments)] // Test helper function
fn build_context<'a>(
    workspace: &'a MemoryWorkspace,
    repo_root: &'a PathBuf,
    executor: &'a Arc<MockProcessExecutor>,
    config: &'a Config,
    registry: &'a AgentRegistry,
    logger: &'a Logger,
    colors: &'a Colors,
    template_context: &'a TemplateContext,
    timer: &'a mut Timer,
    run_log_context: &'a crate::logging::RunLogContext,
) -> crate::phases::PhaseContext<'a> {
    crate::phases::PhaseContext {
        config,
        registry,
        logger,
        colors,
        timer,
        developer_agent: "test-developer",
        reviewer_agent: "test-reviewer",
        review_guidelines: None,
        template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: HashMap::new(),
        executor: &**executor,
        executor_arc: Arc::clone(executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root,
        workspace,
        run_log_context,
    }
}

#[test]
fn restore_prompt_permissions_emits_complete_transition_in_finalizing() {
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let mut ctx = build_context(
        &workspace,
        &repo_root,
        &executor,
        &config,
        &registry,
        &logger,
        &colors,
        &template_context,
        &mut timer,
        &run_log_context,
    );

    let mut state = PipelineState::initial(1, 0);
    state.phase = PipelinePhase::Finalizing;
    let mut handler = MainEffectHandler::new(state);

    let result = handler.execute(Effect::RestorePromptPermissions, &mut ctx);

    assert!(result.is_ok(), "RestorePromptPermissions should succeed");

    let result = result.unwrap();
    assert!(
        result.ui_events.iter().any(|event| matches!(
            event,
            UIEvent::PhaseTransition {
                to: PipelinePhase::Complete,
                ..
            }
        )),
        "Expected phase transition UI event to Complete"
    );
}
