use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_invoke_analysis_agent_gracefully_handles_missing_plan_and_diff() {
    // Regression: analysis should still run even when PLAN.md is missing or git diff cannot
    // be generated. These inputs should be substituted with placeholders.
    let workspace = MemoryWorkspace::new_test().with_dir(".agent/tmp");

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
        phase: crate::reducer::event::PipelinePhase::Development,
        iteration: 0,
        ..PipelineState::initial(1, 0)
    });

    handler
        .invoke_analysis_agent(&mut ctx, 0)
        .expect("invoke_analysis_agent should not fail when PLAN/DIFF inputs are missing");

    // Validate that the prompt passed to the agent contains placeholder context.
    //
    // This test is intentionally resilient to environments where a real git repository is
    // discoverable from the process CWD (e.g., when running unit tests from a checkout).
    // In those cases, diff generation can succeed even if the in-memory workspace is missing
    // `.agent/start_commit`, so the prompt will contain an actual diff instead of a
    // "[DIFF unavailable" placeholder.
    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.contains("[PLAN unavailable"),
        "expected plan placeholder in prompt, got: {prompt}"
    );
    assert!(
        prompt.contains("[DIFF unavailable") || prompt.contains("diff --git"),
        "expected diff placeholder or an actual git diff in prompt, got: {prompt}"
    );
}

#[test]
fn test_invoke_analysis_agent_writes_diff_backup_when_git_diff_succeeds() {
    // When git diff generation succeeds, the handler should still write/update
    // `.agent/DIFF.backup` as a best-effort fallback for prompt materialization.
    let workspace = MemoryWorkspace::new_test()
        .with_dir(".agent/tmp")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/DIFF.backup", "DIFF_BACKUP_MARKER");

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
        phase: crate::reducer::event::PipelinePhase::Development,
        iteration: 0,
        ..PipelineState::initial(1, 0)
    });

    handler
        .invoke_analysis_agent(&mut ctx, 0)
        .expect("invoke_analysis_agent should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.contains("diff --git") || prompt.contains("[DIFF unavailable"),
        "expected a git diff or a diff-unavailable placeholder in prompt"
    );

    let backup = workspace
        .read(std::path::Path::new(".agent/DIFF.backup"))
        .expect("expected .agent/DIFF.backup to exist");
    assert!(
        backup.contains("diff --git") || backup.contains("[DIFF unavailable"),
        "expected .agent/DIFF.backup to contain a git diff or placeholder"
    );
    assert_ne!(
        backup, "DIFF_BACKUP_MARKER",
        "expected .agent/DIFF.backup to be refreshed"
    );
}
