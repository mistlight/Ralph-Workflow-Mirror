use super::*;

#[test]
fn test_materialize_development_inputs_returns_error_when_prompt_missing() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test().with_file(".agent/PLAN.md", "# Plan\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        developer_agent: "dev",
        reviewer_agent: "rev",
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
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .materialize_development_inputs(&mut ctx, 0)
        .expect_err(
            "materialize_development_inputs should return an error when PROMPT.md is missing",
        );

    assert!(
        err.to_string().contains("PROMPT.md"),
        "Expected error message about PROMPT.md, got: {err}"
    );
}

#[test]
fn test_materialize_development_inputs_returns_error_when_plan_missing() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test().with_file("PROMPT.md", "Prompt\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        developer_agent: "dev",
        reviewer_agent: "rev",
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
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .materialize_development_inputs(&mut ctx, 0)
        .expect_err(
            "materialize_development_inputs should return an error when PLAN.md is missing",
        );

    assert!(
        err.to_string().contains("PLAN.md"),
        "Expected error message about PLAN.md, got: {err}"
    );
}

#[test]
fn test_materialize_development_inputs_stores_workspace_relative_file_references() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::state::PromptInputRepresentation;
    use std::path::PathBuf;

    // Make PROMPT exceed inline budget so it becomes a file reference.
    let oversize_prompt = "x".repeat(150 * 1024);
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", &oversize_prompt)
        .with_file(".agent/PLAN.md", "Plan content");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        developer_agent: "dev",
        reviewer_agent: "rev",
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
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let result = handler
        .materialize_development_inputs(&mut ctx, 0)
        .expect("materialize_development_inputs should succeed");

    match &result.event {
        PipelineEvent::PromptInput(PromptInputEvent::DevelopmentInputsMaterialized {
            prompt,
            ..
        }) => {
            let PromptInputRepresentation::FileReference { path } = &prompt.representation else {
                panic!("expected PROMPT to be a file reference when oversize");
            };
            assert!(
                !path.is_absolute(),
                "file reference path should be workspace-relative (checkpoints must not store absolute paths)"
            );
            assert_eq!(
                path,
                &PathBuf::from(".agent/PROMPT.md.backup"),
                "expected PROMPT file reference to point at the PROMPT backup artifact"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
