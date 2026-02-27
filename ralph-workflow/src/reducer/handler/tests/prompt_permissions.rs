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

struct ContextDeps<'a> {
    workspace: &'a MemoryWorkspace,
    workspace_arc: &'a std::sync::Arc<dyn crate::workspace::Workspace>,
    repo_root: &'a PathBuf,
    executor: &'a Arc<MockProcessExecutor>,
    config: &'a Config,
    registry: &'a AgentRegistry,
    logger: &'a Logger,
    colors: &'a Colors,
    template_context: &'a TemplateContext,
    run_log_context: &'a crate::logging::RunLogContext,
}

fn build_context<'a>(
    deps: &ContextDeps<'a>,
    timer: &'a mut Timer,
    cloud: &'a crate::config::types::CloudConfig,
) -> crate::phases::PhaseContext<'a> {
    crate::phases::PhaseContext {
        config: deps.config,
        registry: deps.registry,
        logger: deps.logger,
        colors: deps.colors,
        timer,
        developer_agent: "test-developer",
        reviewer_agent: "test-reviewer",
        review_guidelines: None,
        template_context: deps.template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: HashMap::new(),
        executor: &**deps.executor,
        executor_arc: Arc::clone(deps.executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: deps.repo_root,
        workspace: deps.workspace,
        workspace_arc: Arc::clone(deps.workspace_arc),
        run_log_context: deps.run_log_context,
        cloud_reporter: None,
        cloud,
    }
}

#[test]
fn restore_prompt_permissions_emits_complete_transition_in_finalizing() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());
    let workspace_arc =
        std::sync::Arc::new(workspace.clone()) as std::sync::Arc<dyn crate::workspace::Workspace>;
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let deps = ContextDeps {
        workspace: &workspace,
        workspace_arc: &workspace_arc,
        repo_root: &repo_root,
        executor: &executor,
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        template_context: &template_context,
        run_log_context: &run_log_context,
    };
    let mut ctx = build_context(&deps, &mut timer, &cloud);

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
