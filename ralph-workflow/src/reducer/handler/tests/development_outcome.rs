use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{
    ContinuationState, DevelopmentStatus, DevelopmentValidatedOutcome, PipelineState,
};
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_apply_development_outcome_exhausts_when_next_attempt_reaches_limit() {
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.development_validated_outcome = Some(DevelopmentValidatedOutcome {
        iteration: 0,
        status: DevelopmentStatus::Partial,
        summary: "partial".to_string(),
        files_changed: None,
        next_steps: None,
    });
    handler.state.continuation = ContinuationState {
        continuation_attempt: 2,
        max_continue_count: 3,
        ..ContinuationState::new()
    };

    let workspace = MemoryWorkspace::new_test();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: "claude",
        reviewer_agent: "codex",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: HashMap::new(),
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let result = handler
        .apply_development_outcome(&mut ctx, 0)
        .expect("apply_development_outcome should succeed");

    assert!(matches!(
        result.event,
        crate::reducer::event::PipelineEvent::Development(
            crate::reducer::event::DevelopmentEvent::OutcomeApplied { .. }
        )
    ));
}
