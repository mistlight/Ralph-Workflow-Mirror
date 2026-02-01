use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::PipelineEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{AgentChainState, CommitState, PipelineState};
use crate::workspace::{MemoryWorkspace, Workspace};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_invoke_commit_agent_clears_stale_commit_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_prompt.txt", "commit prompt")
        .with_file(
            xml_paths::COMMIT_MESSAGE_XML,
            "<ralph-commit-message>old</ralph-commit-message>",
        );

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
    let repo_root = PathBuf::from("/mock/repo");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    handler
        .invoke_commit_agent(&mut ctx)
        .expect("invoke_commit_agent should succeed");

    assert!(
        !workspace.exists(Path::new(xml_paths::COMMIT_MESSAGE_XML)),
        "stale commit XML should be cleared before invoking commit agent"
    );
}

#[test]
fn test_extract_commit_xml_emits_missing_event_when_absent() {
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
    let repo_root = PathBuf::from("/mock/repo");

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
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };

    let result = handler
        .extract_commit_xml(&mut ctx)
        .expect("extract_commit_xml should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Commit(crate::reducer::event::CommitEvent::CommitXmlMissing { attempt: 1 })
    ));
}

#[test]
fn test_check_commit_diff_emits_prepared_event() {
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
    let repo_root = PathBuf::from("/mock/repo");

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
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );
    let result = handler
        .check_commit_diff_with_content(&mut ctx, "")
        .expect("check_commit_diff_with_content should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Commit(crate::reducer::event::CommitEvent::DiffPrepared { empty: true })
    ));
}

#[test]
fn test_prepare_commit_prompt_does_not_emit_generation_started() {
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
    let repo_root = PathBuf::from("/mock/repo");

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
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );
    let result = handler
        .prepare_commit_prompt_with_diff(&mut ctx, "diff --git a/a b/a\n+change\n")
        .expect("prepare_commit_prompt_with_diff should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Commit(crate::reducer::event::CommitEvent::PromptPrepared { attempt: 1 })
    ));
    assert!(
        result.additional_events.iter().all(|event| !matches!(
            event,
            PipelineEvent::Commit(crate::reducer::event::CommitEvent::GenerationStarted)
        )),
        "prepare commit prompt should not emit commit_generation_started"
    );
}
