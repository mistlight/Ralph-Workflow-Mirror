use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::PipelineEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{FixStatus, FixValidatedOutcome, PipelineState};
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_apply_fix_outcome_emits_fix_continuation_triggered_for_issues_remain() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new_test();

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.fix_validated_outcome = Some(FixValidatedOutcome {
        pass: 0,
        status: FixStatus::IssuesRemain,
        summary: Some("needs more".to_string()),
    });

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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let result = handler
        .apply_fix_outcome(&mut ctx, 0)
        .expect("apply_fix_outcome should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::FixOutcomeApplied { pass: 0 })
    ));
}

#[test]
fn test_apply_fix_outcome_emits_fix_attempt_completed_for_all_issues_addressed() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new_test();

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.fix_validated_outcome = Some(FixValidatedOutcome {
        pass: 0,
        status: FixStatus::AllIssuesAddressed,
        summary: Some("done".to_string()),
    });

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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let result = handler
        .apply_fix_outcome(&mut ctx, 0)
        .expect("apply_fix_outcome should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::FixOutcomeApplied { pass: 0 })
    ));
}

#[test]
fn test_apply_fix_outcome_emits_fix_continuation_triggered_for_failed() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new_test();

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.fix_validated_outcome = Some(FixValidatedOutcome {
        pass: 0,
        status: FixStatus::Failed,
        summary: Some("blocked".to_string()),
    });

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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let result = handler
        .apply_fix_outcome(&mut ctx, 0)
        .expect("apply_fix_outcome should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::FixOutcomeApplied { pass: 0 })
    ));
}

#[test]
fn test_apply_fix_outcome_emits_fix_continuation_budget_exhausted_when_limit_reached() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new_test();

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.continuation.max_fix_continue_count = 3;
    handler.state.continuation.fix_continuation_attempt = 2;
    handler.state.fix_validated_outcome = Some(FixValidatedOutcome {
        pass: 0,
        status: FixStatus::IssuesRemain,
        summary: Some("still failing".to_string()),
    });

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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let result = handler
        .apply_fix_outcome(&mut ctx, 0)
        .expect("apply_fix_outcome should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::FixOutcomeApplied { pass: 0 })
    ));
}

#[test]
fn test_apply_fix_outcome_returns_error_when_missing_outcome() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new_test();

    let handler = MainEffectHandler::new(PipelineState::initial(1, 1));

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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let err = handler
        .apply_fix_outcome(&mut ctx, 0)
        .expect_err("apply_fix_outcome should return error when fix outcome is missing");

    assert!(
        err.to_string().contains("Missing validated fix outcome"),
        "Expected error about missing validated fix outcome, got: {err}"
    );
}
