use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::ErrorEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_create_commit_returns_typed_error_event_when_git_add_all_fails() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test();

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    // Use a unique, non-existent repo root so git discovery fails deterministically.
    // This avoids mutating process-wide CWD (which would be flaky under parallel test execution).
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let repo_root = std::env::temp_dir().join(format!("ralph-nonexistent-repo-{unique}"));

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud: &cloud,
    };

    let err = MainEffectHandler::create_commit(&mut ctx, "test message".to_string())
        .expect_err("create_commit should fail when repo discovery fails");

    assert!(
        err.downcast_ref::<ErrorEvent>().is_some(),
        "expected Err() to carry an ErrorEvent, got: {err:?}"
    );
}
