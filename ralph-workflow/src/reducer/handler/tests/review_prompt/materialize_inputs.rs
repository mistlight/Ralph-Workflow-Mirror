use super::AtomicWriteEnforcingWorkspace;
use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::PipelineEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_materialize_review_inputs_uses_sentinel_plan_when_missing() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let mut config = Config::default();
    config.isolation_mode = false;
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should succeed with sentinel PLAN");

    assert!(
        matches!(
            result.event,
            PipelineEvent::PromptInput(
                crate::reducer::event::PromptInputEvent::ReviewInputsMaterialized { .. }
            )
        ),
        "Expected ReviewInputsMaterialized event with sentinel PLAN, got {:?}",
        result.event
    );

    // Verify the PLAN file was created with sentinel content (no isolation mode context)
    let plan_content = workspace
        .read(std::path::Path::new(".agent/PLAN.md"))
        .expect("PLAN.md should exist after materialization");
    assert_eq!(
        plan_content, "No PLAN provided",
        "Sentinel PLAN content should not include isolation mode context when isolation_mode=false"
    );
}

#[test]
fn test_materialize_review_inputs_uses_sentinel_plan_with_isolation_mode_context() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let mut config = Config::default();
    config.isolation_mode = true;
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should succeed with sentinel PLAN in isolation mode");

    assert!(
        matches!(
            result.event,
            PipelineEvent::PromptInput(
                crate::reducer::event::PromptInputEvent::ReviewInputsMaterialized { .. }
            )
        ),
        "Expected ReviewInputsMaterialized event with sentinel PLAN, got {:?}",
        result.event
    );

    // Verify the PLAN file was created with sentinel content including isolation mode context
    let plan_content = workspace
        .read(std::path::Path::new(".agent/PLAN.md"))
        .expect("PLAN.md should exist after materialization");
    assert_eq!(
        plan_content, "No PLAN provided (normal in isolation mode)",
        "Sentinel PLAN content should include isolation mode context when isolation_mode=true"
    );
}

#[test]
fn test_materialize_review_inputs_uses_fallback_diff_instructions_when_missing() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should succeed with fallback DIFF instructions");

    assert!(
        matches!(
            result.event,
            PipelineEvent::PromptInput(
                crate::reducer::event::PromptInputEvent::ReviewInputsMaterialized { .. }
            )
        ),
        "Expected ReviewInputsMaterialized event with fallback DIFF, got {:?}",
        result.event
    );
}

#[test]
fn test_materialize_review_inputs_writes_oversize_diff_with_atomic_write() {
    let large_diff = "d".repeat(crate::prompts::MAX_INLINE_CONTENT_SIZE + 1);
    let inner = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/DIFF.backup", &large_diff)
        .with_dir(".agent/tmp");
    let workspace =
        AtomicWriteEnforcingWorkspace::new(inner, std::path::PathBuf::from(".agent/tmp/diff.txt"));

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut ctx = crate::phases::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should return an EffectResult");

    assert!(
        matches!(
            result.event,
            PipelineEvent::PromptInput(
                crate::reducer::event::PromptInputEvent::ReviewInputsMaterialized { .. }
            )
        ),
        "Expected ReviewInputsMaterialized event, got {:?}",
        result.event
    );

    let written = workspace
        .read(std::path::Path::new(".agent/tmp/diff.txt"))
        .expect("materialized diff file should be written");
    assert_eq!(written, large_diff);
}
