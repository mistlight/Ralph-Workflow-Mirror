use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::ErrorEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_create_commit_returns_typed_error_event_when_git_add_all_fails() {
    let workspace = MemoryWorkspace::new_test();

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    // Use a non-existent repo root so git discovery fails. This avoids mutating
    // process-wide CWD (which would be flaky under parallel test execution).
    let repo_root = PathBuf::from("/mock/repo/does-not-exist");

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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));

    let err = handler
        .create_commit(&mut ctx, "test message".to_string())
        .expect_err("create_commit should fail when repo discovery fails");

    assert!(
        err.downcast_ref::<ErrorEvent>().is_some(),
        "expected Err() to carry an ErrorEvent, got: {err:?}"
    );
}
