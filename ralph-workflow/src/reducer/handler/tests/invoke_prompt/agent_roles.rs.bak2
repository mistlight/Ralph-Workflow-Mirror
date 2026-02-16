//! Agent role-specific invocation tests
//!
//! Tests invocation behavior for each agent role:
//! - Development agent prompt handling and errors
//! - Review agent prompt handling and errors
//! - Fix agent prompt handling and errors
//! - Commit agent prompt handling, errors, and uninitialized chain detection

use super::*;

#[test]
fn test_invoke_development_agent_returns_error_when_prompt_missing() {
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_development_agent(&mut ctx, 0)
        .expect_err("invoke_development_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("development prompt"),
        "Expected error about missing development prompt, got: {err}"
    );
}

#[test]
fn test_invoke_review_agent_returns_error_when_prompt_missing() {
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_review_agent(&mut ctx, 0)
        .expect_err("invoke_review_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("review prompt"),
        "Expected error about missing review prompt, got: {err}"
    );
}

#[test]
fn test_invoke_review_agent_maps_non_not_found_prompt_read_errors_to_workspace_read_failed() {
    let inner = MemoryWorkspace::new_test();
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/tmp/review_prompt.txt"),
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_review_agent(&mut ctx, 0)
        .expect_err("invoke_review_agent should error on non-NotFound prompt read");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/tmp/review_prompt.txt"
        ),
        "expected WorkspaceReadFailed, got: {error_event:?}"
    );
}

#[test]
fn test_invoke_fix_agent_returns_error_when_prompt_missing() {
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_fix_agent(&mut ctx, 0)
        .expect_err("invoke_fix_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("fix prompt"),
        "Expected error about missing fix prompt, got: {err}"
    );
}

#[test]
fn test_invoke_fix_agent_maps_non_not_found_prompt_read_errors_to_workspace_read_failed() {
    let inner = MemoryWorkspace::new_test();
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/tmp/fix_prompt.txt"),
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_fix_agent(&mut ctx, 0)
        .expect_err("invoke_fix_agent should error on non-NotFound prompt read");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/tmp/fix_prompt.txt"
        ),
        "expected WorkspaceReadFailed, got: {error_event:?}"
    );
}

#[test]
fn test_invoke_commit_agent_returns_error_when_prompt_missing() {
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        AgentRole::Commit,
    );

    let err = handler
        .invoke_commit_agent(&mut ctx)
        .expect_err("invoke_commit_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("commit prompt"),
        "Expected error about missing commit prompt, got: {err}"
    );
}

#[test]
fn test_invoke_commit_agent_maps_non_not_found_prompt_read_errors_to_workspace_read_failed() {
    let inner = MemoryWorkspace::new_test();
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/tmp/commit_prompt.txt"),
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        AgentRole::Commit,
    );

    let err = handler
        .invoke_commit_agent(&mut ctx)
        .expect_err("invoke_commit_agent should error on non-NotFound prompt read");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/tmp/commit_prompt.txt"
        ),
        "expected WorkspaceReadFailed, got: {error_event:?}"
    );
}

#[test]
fn test_invoke_commit_agent_surfaces_uninitialized_agent_chain_as_error_event() {
    // When the agent chain is empty/uninitialized, invoke_commit_agent must not panic.
    // It must surface a typed ErrorEvent so the reducer can decide interruption policy.
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_prompt.txt", "commit prompt content");
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    // Intentionally leave the agent chain uninitialized/empty.
    handler.state.agent_chain = AgentChainState::initial();

    let err = handler
        .invoke_commit_agent(&mut ctx)
        .expect_err("invoke_commit_agent should return typed error when agent chain is empty");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::CommitAgentNotInitialized { attempt: 1 }
        ),
        "expected CommitAgentNotInitialized, got: {error_event:?}"
    );

    // Defensive: ensure the error type is not a string-based anyhow error.
    assert!(
        !matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                kind: WorkspaceIoErrorKind::Other,
                ..
            }
        ),
        "expected a specific invariant error, not a generic workspace error"
    );
}
