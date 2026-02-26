use crate::agents::AgentRegistry;
use crate::agents::AgentRole;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::AgentChainState;
use crate::reducer::state::PipelineState;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_invoke_planning_agent_does_not_clear_stale_plan_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/planning_prompt.txt", "prompt")
        .with_file(xml_paths::PLAN_XML, "<ralph-plan>old</ralph-plan>");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    assert!(workspace.exists(Path::new(xml_paths::PLAN_XML)));
}

#[test]
fn test_cleanup_planning_xml_clears_stale_plan_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace =
        MemoryWorkspace::new_test().with_file(xml_paths::PLAN_XML, "<ralph-plan>old</ralph-plan>");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .cleanup_planning_xml(&mut ctx, 0)
        .expect("cleanup_planning_xml should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::PLAN_XML)));
}

#[test]
fn test_invoke_development_agent_does_not_clear_stale_dev_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/development_prompt.txt", "prompt")
        .with_file(
            xml_paths::DEVELOPMENT_RESULT_XML,
            "<ralph-development>old</ralph-development>",
        );
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_development_agent(&mut ctx, 0)
        .expect("invoke_development_agent should succeed");

    assert!(workspace.exists(Path::new(xml_paths::DEVELOPMENT_RESULT_XML)));
}

#[test]
fn test_cleanup_development_xml_clears_stale_dev_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test().with_file(
        xml_paths::DEVELOPMENT_RESULT_XML,
        "<ralph-development>old</ralph-development>",
    );
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .cleanup_development_xml(&mut ctx, 0)
        .expect("cleanup_development_xml should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::DEVELOPMENT_RESULT_XML)));
}

#[test]
fn test_invoke_review_agent_does_not_clear_stale_issues_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/review_prompt.txt", "prompt")
        .with_file(xml_paths::ISSUES_XML, "<ralph-issues>old</ralph-issues>");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_review_agent(&mut ctx, 0)
        .expect("invoke_review_agent should succeed");

    assert!(workspace.exists(Path::new(xml_paths::ISSUES_XML)));
}

#[test]
fn test_cleanup_review_issues_xml_clears_stale_issues_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(xml_paths::ISSUES_XML, "<ralph-issues>old</ralph-issues>");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .cleanup_review_issues_xml(&mut ctx, 0)
        .expect("cleanup_review_issues_xml should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::ISSUES_XML)));
}

#[test]
fn test_invoke_fix_agent_does_not_clear_stale_fix_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/fix_prompt.txt", "prompt")
        .with_file(
            xml_paths::FIX_RESULT_XML,
            "<ralph-fix-result>old</ralph-fix-result>",
        );
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_fix_agent(&mut ctx, 0)
        .expect("invoke_fix_agent should succeed");

    assert!(workspace.exists(Path::new(xml_paths::FIX_RESULT_XML)));
}

#[test]
fn test_cleanup_fix_result_xml_clears_stale_fix_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test().with_file(
        xml_paths::FIX_RESULT_XML,
        "<ralph-fix-result>old</ralph-fix-result>",
    );
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .cleanup_fix_result_xml(&mut ctx, 0)
        .expect("cleanup_fix_result_xml should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::FIX_RESULT_XML)));
}

#[test]
fn test_invoke_commit_agent_does_not_clear_stale_commit_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_prompt.txt", "prompt")
        .with_file(
            xml_paths::COMMIT_MESSAGE_XML,
            "<ralph-commit>old</ralph-commit>",
        );
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..PipelineState::initial(1, 1)
    });

    handler
        .invoke_commit_agent(&mut ctx)
        .expect("invoke_commit_agent should succeed");

    assert!(workspace.exists(Path::new(xml_paths::COMMIT_MESSAGE_XML)));
}

#[test]
fn test_cleanup_commit_xml_clears_stale_commit_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test().with_file(
        xml_paths::COMMIT_MESSAGE_XML,
        "<ralph-commit>old</ralph-commit>",
    );
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .cleanup_commit_xml(&mut ctx)
        .expect("cleanup_commit_xml should succeed");

    assert!(!workspace.exists(Path::new(xml_paths::COMMIT_MESSAGE_XML)));
}
