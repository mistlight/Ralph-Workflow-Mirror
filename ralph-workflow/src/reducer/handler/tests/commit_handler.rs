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
use crate::reducer::state::{
    AgentChainState, CommitState, ContinuationState, PipelineState, PromptMode,
    SameAgentRetryReason,
};
use crate::workspace::{MemoryWorkspace, Workspace};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_cleanup_commit_xml_removes_stale_commit_xml() {
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
        .cleanup_commit_xml(&mut ctx)
        .expect("cleanup_commit_xml should succeed");

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
fn test_check_commit_diff_emits_failed_event_on_error() {
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
        .check_commit_diff_with_result(&mut ctx, Err(anyhow::anyhow!("diff failed")))
        .expect("check_commit_diff_with_result should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Commit(crate::reducer::event::CommitEvent::DiffFailed { .. })
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
        .prepare_commit_prompt_with_diff_and_mode(
            &mut ctx,
            "diff --git a/a b/a\n+change\n",
            crate::reducer::state::PromptMode::Normal,
        )
        .expect("prepare_commit_prompt_with_diff_and_mode should succeed");

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

#[test]
fn test_prepare_commit_prompt_xsd_retry_uses_commit_xsd_retry_template() {
    let workspace = MemoryWorkspace::new_test()
        .with_dir(".agent/tmp")
        .with_file(
            ".agent/tmp/commit_message.xml",
            "<ralph-commit><ralph-subject>feat: bad</ralph-subject>",
        )
        .with_file(
            ".agent/tmp/commit_xsd_error.txt",
            "XSD validation failed: MISSING REQUIRED ELEMENT",
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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    let result = handler
        .prepare_commit_prompt_with_diff_and_mode(
            &mut ctx,
            "diff --git a/a b/a\n+change\n",
            crate::reducer::state::PromptMode::XsdRetry,
        )
        .expect("prepare_commit_prompt_with_diff_and_mode should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Commit(crate::reducer::event::CommitEvent::PromptPrepared { attempt: 1 })
    ));

    let prompt = workspace
        .read(std::path::Path::new(".agent/tmp/commit_prompt.txt"))
        .expect("commit_prompt.txt should be written");
    assert!(
        prompt.contains("XSD VALIDATION FAILED - FIX XML ONLY"),
        "Expected commit_xsd_retry prompt template, got: {prompt}"
    );
    assert!(
        prompt.contains("MISSING REQUIRED ELEMENT"),
        "Expected XSD error to be included in retry prompt, got: {prompt}"
    );
    assert!(
        !prompt.contains("diff --git"),
        "XSD retry prompt should not include diff content, got: {prompt}"
    );
}

#[test]
fn test_prepare_commit_prompt_same_agent_retry_uses_previous_prepared_prompt() {
    let marker = "<<<PREVIOUS_COMMIT_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/commit_prompt.txt", marker);

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

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Timeout),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(1, 0)
    });
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    let _ = handler
        .prepare_commit_prompt_with_diff_and_mode(
            &mut ctx,
            "diff --git a/a b/a\n+change\n",
            PromptMode::SameAgentRetry,
        )
        .expect("prepare_commit_prompt_with_diff_and_mode should succeed");

    let prompt = workspace
        .read(std::path::Path::new(".agent/tmp/commit_prompt.txt"))
        .expect("commit_prompt.txt should be written");

    assert!(
        prompt.contains(marker),
        "Same-agent retry should reuse the previously prepared prompt; got: {prompt}"
    );
    assert!(
        prompt.contains("## Retry Note (attempt 1)"),
        "Same-agent retry should prepend retry note; got: {prompt}"
    );
}
