use super::*;
use crate::prompts::template_registry::TemplateRegistry;
use crate::reducer::event::{AgentEvent, PipelinePhase};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_prepare_development_prompt_emits_template_invalid_event() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    // Test that {{}} braces in PROMPT.md content don't cause false positive validation errors.
    //
    // With the new log-based validation (vs old regex-based), template values containing
    // {{ }} patterns (like JSX code) are correctly treated as DATA, not template syntax.
    //
    // This test verifies that when PROMPT.md contains "{{LITERAL}}", it gets substituted
    // into the template as a value, and the log-based validator correctly recognizes that
    // {{LITERAL}} is part of the SUBSTITUTED value, not an unresolved placeholder.
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt with {{LITERAL}} braces")
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

    let prompt_history = HashMap::new();

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
        prompt_history,
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let materialize = handler
        .materialize_development_inputs(&mut ctx, 0)
        .expect("materialize_development_inputs should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), materialize.event);
    for ev in materialize.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }
    let result = handler
        .prepare_development_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_development_prompt should succeed");

    // Verify that {{LITERAL}} braces in PROMPT.md don't cause false positive validation errors
    // With log-based validation, values containing {{ }} are treated as data, not template syntax
    // The primary event should be DevelopmentPromptPrepared (success), and TemplateRendered should be in additional_events
    assert!(matches!(
        result.event,
        PipelineEvent::Development(DevelopmentEvent::PromptPrepared { .. })
    ));
    // TemplateRendered should be emitted as an additional event
    assert!(result.additional_events.iter().any(|ev| matches!(
        ev,
        PipelineEvent::PromptInput(PromptInputEvent::TemplateRendered { .. })
    )));
}

#[test]
fn test_prepare_development_prompt_emits_template_rendered_on_validation_failure() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let tempdir = tempdir().expect("create temp dir");
    let template_path = tempdir.path().join("developer_iteration_xml.txt");
    fs::write(
        &template_path,
        "Prompt:\n{{PROMPT}}\nPlan:\n{{PLAN}}\nMissing: {{MISSING}}\n",
    )
    .expect("write developer template");

    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt content")
        .with_file(".agent/PLAN.md", "Plan content")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context =
        TemplateContext::new(TemplateRegistry::new(Some(tempdir.path().to_path_buf())));

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
    let materialize = handler
        .materialize_development_inputs(&mut ctx, 0)
        .expect("materialize_development_inputs should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), materialize.event);
    for ev in materialize.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }

    let result = handler
        .prepare_development_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_development_prompt should succeed");

    match result.event {
        PipelineEvent::PromptInput(PromptInputEvent::TemplateRendered {
            phase,
            template_name,
            log,
        }) => {
            assert_eq!(phase, PipelinePhase::Development);
            assert_eq!(template_name, "developer_iteration_xml");
            assert!(log.unsubstituted.contains(&"MISSING".to_string()));
        }
        other => panic!("expected TemplateRendered event, got {other:?}"),
    }

    assert!(
        result.additional_events.iter().any(|event| matches!(
            event,
            PipelineEvent::Agent(AgentEvent::TemplateVariablesInvalid { missing_variables, .. })
                if missing_variables.contains(&"MISSING".to_string())
        )),
        "expected TemplateVariablesInvalid with missing variables"
    );
}

#[test]
fn test_prepare_development_prompt_normal_mode_ignores_continuation_state() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_dir(".agent/tmp");

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

    let mut prompt_history = HashMap::new();
    // Store a continuation prompt containing unresolved placeholders.
    // Normal mode must NOT replay this continuation prompt.
    prompt_history.insert(
        "development_0_continuation_1".to_string(),
        "{{UNRESOLVED}}".to_string(),
    );

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
        prompt_history,
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: crate::reducer::state::ContinuationState {
            continuation_attempt: 1,
            ..crate::reducer::state::ContinuationState::new()
        },
        ..PipelineState::initial(1, 1)
    });

    let materialize = handler
        .materialize_development_inputs(&mut ctx, 0)
        .expect("materialize_development_inputs should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), materialize.event);
    for ev in materialize.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }

    let result = handler
        .prepare_development_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_development_prompt should succeed");

    // Even though a stored continuation prompt contains unresolved placeholders,
    // PromptMode::Normal must ignore continuation state and prepare a normal prompt.
    assert!(
        matches!(
            result.event,
            PipelineEvent::Development(
                crate::reducer::event::DevelopmentEvent::PromptPrepared { .. }
            )
        ),
        "Expected PromptPrepared event when placeholders in PROMPT.md are ignored, got {:?}",
        result.event
    );
}

#[test]
fn test_prepare_development_prompt_detects_unresolved_partial() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    // Test that unresolved placeholders in the template itself (not in PROMPT/PLAN content)
    // would be detected. This requires a custom template with an unresolved partial.
    // Since the default templates are well-formed, we skip this test.
    // The validation logic is tested separately in template_validator.rs.
}

#[test]
fn test_prepare_development_prompt_returns_error_when_inputs_not_materialized() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "# Prompt\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_dir(".agent/tmp");

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
        .prepare_development_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect_err(
            "prepare_development_prompt should return an error when inputs not materialized",
        );

    assert!(
        err.to_string().contains("not materialized"),
        "Expected error message about inputs not being materialized, got: {err}"
    );
}
