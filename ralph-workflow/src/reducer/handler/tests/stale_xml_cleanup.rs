use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_invoke_planning_agent_clears_stale_plan_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/planning_prompt.txt", "prompt")
        .with_file(xml_paths::PLAN_XML, "<ralph-plan>old</ralph-plan>");
    let executor = Arc::new(MockProcessExecutor::new());
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::PLAN_XML)));
}

#[test]
fn test_invoke_development_agent_clears_stale_dev_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/development_prompt.txt", "prompt")
        .with_file(
            xml_paths::DEVELOPMENT_RESULT_XML,
            "<ralph-development>old</ralph-development>",
        );
    let executor = Arc::new(MockProcessExecutor::new());
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_development_agent(&mut ctx, 0)
        .expect("invoke_development_agent should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::DEVELOPMENT_RESULT_XML)));
}

#[test]
fn test_invoke_review_agent_clears_stale_issues_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/review_prompt.txt", "prompt")
        .with_file(xml_paths::ISSUES_XML, "<ralph-issues>old</ralph-issues>");
    let executor = Arc::new(MockProcessExecutor::new());
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_review_agent(&mut ctx, 0)
        .expect("invoke_review_agent should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::ISSUES_XML)));
}

#[test]
fn test_invoke_fix_agent_clears_stale_fix_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/fix_prompt.txt", "prompt")
        .with_file(
            xml_paths::FIX_RESULT_XML,
            "<ralph-fix-result>old</ralph-fix-result>",
        );
    let executor = Arc::new(MockProcessExecutor::new());
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_fix_agent(&mut ctx, 0)
        .expect("invoke_fix_agent should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::FIX_RESULT_XML)));
}
