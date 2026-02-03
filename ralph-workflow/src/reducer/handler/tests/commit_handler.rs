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
    AgentChainState, CommitState, MaterializedCommitInputs, MaterializedPromptInput, PipelineState,
    PromptInputKind, PromptInputRepresentation, PromptMaterializationReason, PromptMode,
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

#[test]
fn test_prepare_commit_prompt_does_not_panic_when_materialized_attempt_mismatch() {
    use crate::reducer::state::{
        MaterializedCommitInputs, MaterializedPromptInput, PromptInputKind,
        PromptInputRepresentation, PromptInputsState, PromptMaterializationReason, PromptMode,
    };
    use std::panic::{catch_unwind, AssertUnwindSafe};

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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.commit = CommitState::Generating {
        attempt: 2,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["qwen".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    // Materialized for attempt 1, but current attempt is 2 (mismatch).
    handler.state.prompt_inputs = PromptInputsState {
        commit: Some(MaterializedCommitInputs {
            attempt: 1,
            diff: MaterializedPromptInput {
                kind: PromptInputKind::Diff,
                content_id_sha256: "hash".to_string(),
                consumer_signature_sha256: handler.state.agent_chain.consumer_signature_sha256(),
                original_bytes: 1,
                final_bytes: 1,
                model_budget_bytes: Some(100_000),
                inline_budget_bytes: Some(100_000),
                representation: PromptInputRepresentation::Inline,
                reason: PromptMaterializationReason::WithinBudgets,
            },
        }),
        ..Default::default()
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        handler.prepare_commit_prompt(&mut ctx, PromptMode::XsdRetry)
    }));
    assert!(
        result.is_ok(),
        "prepare_commit_prompt should not panic when commit inputs are missing for the current attempt"
    );
}

#[test]
fn test_materialize_commit_inputs_uses_min_model_budget_across_agent_chain() {
    use crate::reducer::event::PromptInputEvent;

    let large_diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(250_000));
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_diff.txt", &large_diff)
        .with_dir(".agent/tmp");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec![
            "claude".to_string(),
            "qwen".to_string(),
            "default-agent".to_string(),
        ],
        vec![vec![], vec![], vec![]],
        crate::agents::AgentRole::Commit,
    );

    let result = handler
        .materialize_commit_inputs(&mut ctx, 1)
        .expect("materialize_commit_inputs should succeed");

    let materialized = match result.event {
        PipelineEvent::PromptInput(PromptInputEvent::CommitInputsMaterialized {
            attempt,
            diff,
        }) => {
            assert_eq!(attempt, 1);
            diff
        }
        other => panic!("unexpected event: {other:?}"),
    };

    assert_eq!(
        materialized.model_budget_bytes,
        Some(100_000),
        "expected model budget to be min across agent chain (qwen-like => 100KB)"
    );
    assert!(
        workspace.was_written(".agent/tmp/commit_diff.model_safe.txt"),
        "materialized model-safe diff should be written once to a canonical path"
    );
    assert!(
        materialized.final_bytes <= 100_000,
        "model-safe diff should not exceed the effective model budget"
    );
}

#[test]
fn test_prepare_commit_prompt_xsd_retry_uses_commit_xsd_retry_template() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(
            ".agent/tmp/commit_diff.txt",
            "diff --git a/a b/a\n+change\n",
        )
        .with_file(
            ".agent/tmp/commit_diff.model_safe.txt",
            "diff --git a/a b/a\n+change\n",
        )
        .with_file(
            ".agent/tmp/commit_message.xml",
            "<ralph-commit>bad</ralph-commit>",
        )
        .with_dir(".agent/tmp");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    // Seed materialized commit inputs so prompt prep can proceed.
    handler.state.prompt_inputs.commit = Some(MaterializedCommitInputs {
        attempt: 1,
        diff: MaterializedPromptInput {
            kind: PromptInputKind::Diff,
            content_id_sha256: "hash".to_string(),
            consumer_signature_sha256: handler.state.agent_chain.consumer_signature_sha256(),
            original_bytes: 1,
            final_bytes: 1,
            model_budget_bytes: Some(200_000),
            inline_budget_bytes: Some(crate::prompts::MAX_INLINE_CONTENT_SIZE as u64),
            representation: PromptInputRepresentation::Inline,
            reason: PromptMaterializationReason::WithinBudgets,
        },
    });

    handler
        .prepare_commit_prompt(&mut ctx, PromptMode::XsdRetry)
        .expect("prepare_commit_prompt should succeed");

    let prompt = workspace
        .read(std::path::Path::new(".agent/tmp/commit_prompt.txt"))
        .expect("commit prompt should be written");

    assert!(
        prompt.contains("XSD VALIDATION FAILED - FIX XML ONLY"),
        "Expected commit XSD retry prompt template, got: {prompt}"
    );
}

#[test]
fn test_materialize_commit_inputs_includes_size_info_in_ui_events() {
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::ui_event::UIEvent;

    // Create diff that exceeds model budget (100KB for qwen) but not inline budget
    let large_diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(150_000));
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_diff.txt", &large_diff)
        .with_dir(".agent/tmp");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    // Use qwen to trigger model budget truncation (100KB budget)
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["qwen".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    let result = handler
        .materialize_commit_inputs(&mut ctx, 1)
        .expect("materialize_commit_inputs should succeed");

    // Verify main event has correct sizes
    match &result.event {
        PipelineEvent::PromptInput(PromptInputEvent::CommitInputsMaterialized { diff, .. }) => {
            assert!(
                diff.original_bytes > 100_000,
                "original bytes should exceed budget"
            );
            assert!(
                diff.final_bytes <= 100_000,
                "final bytes should be within budget"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }

    // Verify UI event includes size information
    let has_size_ui_event = result.ui_events.iter().any(|event| {
        if let UIEvent::AgentActivity { message, .. } = event {
            message.contains("KB") && message.contains("->")
        } else {
            false
        }
    });
    assert!(
        has_size_ui_event,
        "UI events should include size information when truncation occurs"
    );
}

#[test]
fn test_materialize_commit_inputs_records_correct_materialization_reason() {
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::state::PromptMaterializationReason;

    // Create diff that exceeds model budget
    let large_diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(150_000));
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_diff.txt", &large_diff)
        .with_dir(".agent/tmp");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    // Use qwen to trigger model budget truncation
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["qwen".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    let result = handler
        .materialize_commit_inputs(&mut ctx, 1)
        .expect("materialize_commit_inputs should succeed");

    match &result.event {
        PipelineEvent::PromptInput(PromptInputEvent::CommitInputsMaterialized { diff, .. }) => {
            assert!(
                matches!(
                    diff.reason,
                    PromptMaterializationReason::ModelBudgetExceeded
                ),
                "reason should be ModelBudgetExceeded when diff is truncated for model budget"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn test_materialize_commit_inputs_records_combined_reason_when_truncated_and_referenced() {
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::state::{PromptInputRepresentation, PromptMaterializationReason};
    use std::path::PathBuf;

    // Create diff that exceeds both model budget (claude: 300KB) and inline budget (~100KB).
    // Use many medium-sized lines so truncation still leaves a large payload.
    let mut large_diff = String::from("diff --git a/a b/a\n");
    for _ in 0..6_000 {
        large_diff.push('+');
        large_diff.push_str(&"x".repeat(100));
        large_diff.push('\n');
    }
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_diff.txt", &large_diff)
        .with_dir(".agent/tmp");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    // Use claude to get a large model budget while still exceeding inline budget.
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude-opus".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    let result = handler
        .materialize_commit_inputs(&mut ctx, 1)
        .expect("materialize_commit_inputs should succeed");

    match &result.event {
        PipelineEvent::PromptInput(PromptInputEvent::CommitInputsMaterialized { diff, .. }) => {
            assert!(
                matches!(
                    diff.representation,
                    PromptInputRepresentation::FileReference { .. }
                ),
                "diff should be referenced by file when still above inline budget"
            );
            assert!(
                matches!(
                    diff.reason,
                    PromptMaterializationReason::ModelBudgetExceeded
                ),
                "reason should reflect model truncation even when a file reference is used"
            );
            if let PromptInputRepresentation::FileReference { path } = &diff.representation {
                assert!(
                    !path.is_absolute(),
                    "file reference path should be workspace-relative (checkpoints must not store absolute paths)"
                );
                assert_eq!(
                    path,
                    &PathBuf::from(".agent/tmp/commit_diff.model_safe.txt"),
                    "expected file reference to point at the model-safe diff artifact"
                );
            }
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn test_materialize_commit_inputs_within_budget_records_correct_reason() {
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::state::PromptMaterializationReason;

    // Create small diff within all budgets
    let small_diff = "diff --git a/a b/a\n+small change\n";
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_diff.txt", small_diff)
        .with_dir(".agent/tmp");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    let result = handler
        .materialize_commit_inputs(&mut ctx, 1)
        .expect("materialize_commit_inputs should succeed");

    match &result.event {
        PipelineEvent::PromptInput(PromptInputEvent::CommitInputsMaterialized { diff, .. }) => {
            assert!(
                matches!(diff.reason, PromptMaterializationReason::WithinBudgets),
                "reason should be WithinBudgets for small diff"
            );
            assert_eq!(
                diff.original_bytes, diff.final_bytes,
                "sizes should be equal when no truncation"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

// =============================================================================
// Idempotence and stability tests
// =============================================================================

/// Test that prepare_commit_prompt reads from materialized model-safe diff file.
///
/// Once commit inputs are materialized, the prepare_commit_prompt effect should
/// read from .agent/tmp/commit_diff.model_safe.txt, ensuring the prompt uses
/// the already-truncated content instead of re-truncating.
#[test]
fn test_prepare_commit_prompt_uses_materialized_diff() {
    use crate::reducer::state::{
        MaterializedCommitInputs, MaterializedPromptInput, PromptInputKind,
        PromptInputRepresentation, PromptInputsState, PromptMaterializationReason, PromptMode,
    };

    // Original large diff (will be truncated)
    let large_diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(150_000));
    // Simulated truncated diff from materialization
    let model_safe_diff = "diff --git a/a b/a\n+truncated_content [truncated...]\n";

    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_diff.txt", &large_diff)
        .with_file(".agent/tmp/commit_diff.model_safe.txt", model_safe_diff)
        .with_dir(".agent/tmp");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["qwen".to_string()], // qwen has 100KB budget
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );
    // Set up pre-materialized inputs
    let consumer_sig = handler.state.agent_chain.consumer_signature_sha256();
    handler.state.prompt_inputs = PromptInputsState {
        commit: Some(MaterializedCommitInputs {
            attempt: 1,
            diff: MaterializedPromptInput {
                kind: PromptInputKind::Diff,
                content_id_sha256: "hash".to_string(),
                consumer_signature_sha256: consumer_sig,
                original_bytes: large_diff.len() as u64,
                final_bytes: model_safe_diff.len() as u64,
                model_budget_bytes: Some(100_000),
                inline_budget_bytes: Some(100_000),
                representation: PromptInputRepresentation::Inline,
                reason: PromptMaterializationReason::ModelBudgetExceeded,
            },
        }),
        ..Default::default()
    };

    let result = handler
        .prepare_commit_prompt(&mut ctx, PromptMode::Normal)
        .expect("prepare_commit_prompt should succeed");

    // Should succeed with a prompt containing the truncated diff, not the original
    assert!(
        matches!(
            result.event,
            PipelineEvent::Commit(crate::reducer::event::CommitEvent::PromptPrepared { .. })
        ),
        "expected PromptPrepared event"
    );

    // The generated prompt file should contain the truncated diff content
    let prompt_content = workspace.get_file(".agent/tmp/commit_prompt.txt").unwrap();
    assert!(
        prompt_content.contains("truncated_content"),
        "prompt should contain materialized (truncated) diff content"
    );
    assert!(
        !prompt_content.contains(&"x".repeat(1000)),
        "prompt should NOT contain original large diff content"
    );
}

#[test]
fn test_prepare_commit_prompt_aborts_when_model_safe_diff_missing() {
    use crate::reducer::state::{
        MaterializedCommitInputs, MaterializedPromptInput, PromptInputKind,
        PromptInputRepresentation, PromptInputsState, PromptMaterializationReason, PromptMode,
    };

    let workspace = MemoryWorkspace::new_test()
        .with_file(
            ".agent/tmp/commit_diff.txt",
            "diff --git a/a b/a\n+change\n",
        )
        .with_dir(".agent/tmp");

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
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["qwen".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );
    let consumer_sig = handler.state.agent_chain.consumer_signature_sha256();
    handler.state.prompt_inputs = PromptInputsState {
        commit: Some(MaterializedCommitInputs {
            attempt: 1,
            diff: MaterializedPromptInput {
                kind: PromptInputKind::Diff,
                content_id_sha256: "hash".to_string(),
                consumer_signature_sha256: consumer_sig,
                original_bytes: 1,
                final_bytes: 1,
                model_budget_bytes: Some(100_000),
                inline_budget_bytes: Some(100_000),
                representation: PromptInputRepresentation::Inline,
                reason: PromptMaterializationReason::WithinBudgets,
            },
        }),
        ..Default::default()
    };

    let result = handler
        .prepare_commit_prompt(&mut ctx, PromptMode::Normal)
        .expect("prepare_commit_prompt should return an EffectResult");

    assert!(
        matches!(
            result.event,
            PipelineEvent::Lifecycle(crate::reducer::event::LifecycleEvent::Aborted { .. })
        ),
        "Expected pipeline_aborted when commit_diff.model_safe.txt is missing, got {:?}",
        result.event
    );
}

/// Test that model budget is calculated as min across all agents in chain.
///
/// When the agent chain contains [claude (300KB), qwen (100KB), default (200KB)],
/// the effective budget should be 100KB (the minimum).
#[test]
fn test_effective_model_budget_uses_min_across_agent_chain() {
    use crate::phases::commit::effective_model_budget_bytes;

    // claude (300KB) + qwen (100KB) + default (200KB) = min is 100KB
    let agents = vec![
        "claude-opus".to_string(),
        "qwen-turbo".to_string(),
        "gpt-4".to_string(),
    ];
    let budget = effective_model_budget_bytes(&agents);

    // qwen has the smallest budget at 100KB (GLM_MAX_PROMPT_SIZE)
    assert_eq!(
        budget, 100_000,
        "budget should be min across agent chain (qwen's 100KB)"
    );
}

/// Test that consumer_signature_sha256 changes when agent chain configuration changes.
///
/// This ensures that when the agent chain is modified (agents added/removed),
/// the materialized inputs will be invalidated and re-materialized with
/// the new budget.
#[test]
fn test_consumer_signature_changes_with_agent_chain() {
    let chain1 = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );
    let chain2 = AgentChainState::initial().with_agents(
        vec!["claude".to_string(), "qwen".to_string()],
        vec![vec![], vec![]],
        crate::agents::AgentRole::Commit,
    );

    let sig1 = chain1.consumer_signature_sha256();
    let sig2 = chain2.consumer_signature_sha256();

    assert_ne!(
        sig1, sig2,
        "consumer signature should change when agent chain changes"
    );
}

/// Test that consumer_signature_sha256 is stable when only current_agent_index changes.
///
/// During XSD retry or fallback attempts, the current_agent_index changes but
/// the overall chain configuration stays the same. The signature should be
/// stable so we don't unnecessarily re-materialize.
#[test]
fn test_consumer_signature_stable_during_fallback() {
    let chain1 = AgentChainState::initial().with_agents(
        vec!["claude".to_string(), "qwen".to_string()],
        vec![vec![], vec![]],
        crate::agents::AgentRole::Commit,
    );
    let mut chain2 = chain1.clone();
    chain2.current_agent_index = 1; // Fallback to second agent

    let sig1 = chain1.consumer_signature_sha256();
    let sig2 = chain2.consumer_signature_sha256();

    assert_eq!(
        sig1, sig2,
        "consumer signature should be stable when only current_agent_index changes"
    );
}
