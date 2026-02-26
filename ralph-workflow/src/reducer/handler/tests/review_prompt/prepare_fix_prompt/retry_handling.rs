use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{ContinuationState, PipelineState, PromptMode, SameAgentRetryReason};
use crate::workspace::{MemoryWorkspace, Workspace};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn test_prepare_fix_prompt_same_agent_retry_uses_previous_prepared_prompt() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let marker = "<<<PREVIOUS_FIX_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "<issues/>\n")
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/fix_prompt.txt", marker);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
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
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Other),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });
    let _ = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_fix_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/fix_prompt.txt"))
        .expect("fix prompt file should be written");

    assert!(
        prompt.contains(marker),
        "Same-agent retry should reuse the previously prepared prompt; got: {prompt}"
    );
    assert!(
        prompt.contains("## Retry Note (attempt 1)"),
        "Same-agent retry should prepend retry note; got: {prompt}"
    );
}

#[test]
fn test_prepare_fix_prompt_same_agent_retry_does_not_stack_retry_notes() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let marker = "<<<PREVIOUS_FIX_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "<issues/>\n")
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/fix_prompt.txt", marker);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
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
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Other),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let _ = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_fix_prompt should succeed");

    handler.state.continuation.same_agent_retry_count = 2;
    let _ = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_fix_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/fix_prompt.txt"))
        .expect("fix prompt file should be written");

    assert!(
        prompt.contains(marker),
        "Same-agent retry should keep the base prompt content; got: {prompt}"
    );
    assert_eq!(
        prompt.matches("## Retry Note").count(),
        1,
        "Expected exactly one retry note block, got: {prompt}"
    );
    assert!(
        prompt.contains("## Retry Note (attempt 2)"),
        "Expected retry note attempt 2 after second retry, got: {prompt}"
    );
    assert!(
        !prompt.contains("## Retry Note (attempt 1)"),
        "Expected previous retry note to be replaced, got: {prompt}"
    );
}
