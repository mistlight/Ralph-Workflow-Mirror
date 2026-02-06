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
use crate::reducer::state::PipelineState;
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_extract_planning_xml_emits_missing_event() {
    let workspace = MemoryWorkspace::new_test();
    let executor = Arc::new(MockProcessExecutor::new());
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let repo_root = PathBuf::from("/mock/repo");
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    let result = handler
        .extract_planning_xml(&mut ctx, 0)
        .expect("extract_planning_xml should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Planning(crate::reducer::event::PlanningEvent::PlanXmlMissing { .. })
    ));
}

#[test]
fn test_extract_development_xml_emits_missing_event() {
    let workspace = MemoryWorkspace::new_test();
    let executor = Arc::new(MockProcessExecutor::new());
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let repo_root = PathBuf::from("/mock/repo");
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    let result = handler
        .extract_development_xml(&mut ctx, 0)
        .expect("extract_development_xml should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Development(crate::reducer::event::DevelopmentEvent::XmlMissing { .. })
    ));
}

#[test]
fn test_extract_review_issues_xml_emits_missing_event() {
    let workspace = MemoryWorkspace::new_test();
    let executor = Arc::new(MockProcessExecutor::new());
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let repo_root = PathBuf::from("/mock/repo");
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .extract_review_issues_xml(&mut ctx, 0)
        .expect("extract_review_issues_xml should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::IssuesXmlMissing { .. })
    ));
}

#[test]
fn test_extract_fix_result_xml_emits_missing_event() {
    let workspace = MemoryWorkspace::new_test();
    let executor = Arc::new(MockProcessExecutor::new());
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let repo_root = PathBuf::from("/mock/repo");
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .extract_fix_result_xml(&mut ctx, 0)
        .expect("extract_fix_result_xml should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::FixResultXmlMissing { .. })
    ));
}
