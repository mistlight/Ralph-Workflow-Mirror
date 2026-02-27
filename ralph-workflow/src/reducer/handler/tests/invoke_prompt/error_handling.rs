//! Error handling tests for `invoke_prompt`
//!
//! Tests error scenarios when prompts are missing or unreadable:
//! - Missing prompt files (`NotFound` errors)
//! - Non-NotFound I/O errors (`PermissionDenied`, etc.)
//! - Agent invocation failures don't mark agent as invoked

use super::*;

#[test]
fn test_invoke_planning_agent_returns_error_when_prompt_missing() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test();
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect_err("invoke_planning_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("planning prompt"),
        "Expected error about missing planning prompt, got: {err}"
    );
}

#[test]
fn test_invoke_planning_agent_maps_non_not_found_prompt_read_errors_to_workspace_read_failed() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let inner = MemoryWorkspace::new_test();
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/tmp/planning_prompt.txt"),
        io::ErrorKind::PermissionDenied,
    );

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect_err("invoke_planning_agent should error on non-NotFound prompt read");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/tmp/planning_prompt.txt"
        ),
        "expected WorkspaceReadFailed, got: {error_event:?}"
    );
}

#[test]
fn test_invoke_planning_agent_does_not_mark_invoked_on_failure() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "planning prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new().with_agent_result(
        "claude",
        Ok(crate::executor::AgentCommandResult::failure(1, "boom")),
    ));

    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );

    let result = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Agent(AgentEvent::InvocationStarted { .. })
    ));
    assert!(
        result.additional_events.iter().any(|e| {
            matches!(
                e,
                PipelineEvent::Agent(
                    AgentEvent::InvocationFailed { .. }
                        | AgentEvent::RateLimited { .. }
                        | AgentEvent::AuthFailed { .. }
                        | AgentEvent::TimedOut { .. }
                )
            )
        }),
        "invoke_agent should emit a failure fact event after InvocationStarted"
    );
    assert!(
        !result
            .additional_events
            .iter()
            .any(|e| matches!(e, PipelineEvent::Lifecycle(_))),
        "planning agent invoked should not be emitted on failure"
    );
}
