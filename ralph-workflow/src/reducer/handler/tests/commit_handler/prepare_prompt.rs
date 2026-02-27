use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::prompts::template_registry::TemplateRegistry;
use crate::reducer::event::{AgentEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{
    AgentChainState, CommitState, ContinuationState, MaterializedCommitInputs,
    MaterializedPromptInput, PipelineState, PromptInputKind, PromptInputRepresentation,
    PromptInputsState, PromptMaterializationReason, PromptMode, SameAgentRetryReason,
};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_prepare_commit_prompt_does_not_emit_generation_started() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test();

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
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
fn test_prepare_commit_prompt_emits_template_rendered_on_validation_failure() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let tempdir = tempdir().expect("create temp dir");
    let template_path = tempdir.path().join("commit_message_xml.txt");
    fs::write(&template_path, "Diff:\n{{DIFF}}\nMissing: {{MISSING}}\n")
        .expect("write commit template");

    let workspace = MemoryWorkspace::new_test().with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context =
        TemplateContext::new(TemplateRegistry::new(Some(tempdir.path().to_path_buf())));
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
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
            PromptMode::Normal,
        )
        .expect("prepare_commit_prompt_with_diff_and_mode should succeed");

    match result.event {
        PipelineEvent::PromptInput(PromptInputEvent::TemplateRendered {
            phase,
            template_name,
            log,
        }) => {
            assert_eq!(phase, PipelinePhase::CommitMessage);
            assert_eq!(template_name, "commit_message_xml");
            assert!(log.unsubstituted.contains(&"MISSING".to_string()));
        }
        other => panic!("expected TemplateRendered event, got {other:?}"),
    }

    assert!(
        result.additional_events.iter().any(|event| matches!(
            event,
            PipelineEvent::Agent(AgentEvent::TemplateVariablesInvalid { missing_variables, .. })
                if missing_variables.contains(&"MISSING".to_string())
        )),
        "expected TemplateVariablesInvalid with missing variables"
    );
}

#[test]
fn test_prepare_commit_prompt_xsd_retry_uses_commit_xsd_retry_template() {
    let cloud = crate::config::types::CloudConfig::disabled();
    // The XSD retry prompt now validates that required input files exist.
    // This test provides those files to verify the retry prompt generation works.
    let workspace = MemoryWorkspace::new_test()
        .with_dir(".agent/tmp")
        .with_file(
            ".agent/tmp/commit_message.xml",
            "<ralph-commit><ralph-subject>test: subject</ralph-subject></ralph-commit>",
        );
    // Note: commit_message.xsd is automatically written by prompt_commit_xsd_retry_with_context

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
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
    handler.state.continuation.last_xsd_error =
        Some("XSD validation failed: MISSING REQUIRED ELEMENT".to_string());

    handler
        .prepare_commit_prompt(&mut ctx, PromptMode::XsdRetry)
        .expect("prepare_commit_prompt should succeed");

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
fn test_prepare_commit_prompt_does_not_panic_when_materialized_attempt_mismatch() {
    let cloud = crate::config::types::CloudConfig::disabled();

    let workspace = MemoryWorkspace::new_test()
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/commit_diff.model_safe.txt", "diff");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
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
    handler.state.prompt_inputs.commit = Some(MaterializedCommitInputs {
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
    });

    let result = catch_unwind(AssertUnwindSafe(|| {
        handler.prepare_commit_prompt(&mut ctx, PromptMode::Normal)
    }));
    assert!(
        result.is_ok(),
        "prepare_commit_prompt should not panic when commit inputs are missing for the current attempt"
    );
}

#[test]
fn test_prepare_commit_prompt_same_agent_retry_uses_previous_prepared_prompt() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let marker = "<<<PREVIOUS_COMMIT_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/commit_prompt.txt", marker);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
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

    handler
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

#[test]
fn test_prepare_commit_prompt_same_agent_retry_does_not_stack_retry_notes() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let marker = "<<<PREVIOUS_COMMIT_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/commit_prompt.txt", marker);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
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

    handler
        .prepare_commit_prompt_with_diff_and_mode(
            &mut ctx,
            "diff --git a/a b/a\n+change\n",
            PromptMode::SameAgentRetry,
        )
        .expect("prepare_commit_prompt_with_diff_and_mode should succeed");

    handler.state.continuation.same_agent_retry_count = 2;
    handler
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

/// Test that `prepare_commit_prompt` reads from materialized model-safe diff file.
///
/// Once commit inputs are materialized, the `prepare_commit_prompt` effect should
/// read from .`agent/tmp/commit_diff.model_safe.txt`, ensuring the prompt uses
/// the already-truncated content instead of re-truncating.
#[test]
fn test_prepare_commit_prompt_uses_materialized_diff() {
    let cloud = crate::config::types::CloudConfig::disabled();

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

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
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
fn test_prepare_commit_prompt_invalidates_materialized_inputs_when_model_safe_diff_missing() {
    let cloud = crate::config::types::CloudConfig::disabled();

    let workspace = MemoryWorkspace::new_test()
        .with_file(
            ".agent/tmp/commit_diff.txt",
            "diff --git a/a b/a\n+change\n",
        )
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.commit_diff_prepared = true;
    handler.state.commit_diff_empty = false;
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
            PipelineEvent::Commit(crate::reducer::event::CommitEvent::DiffInvalidated { .. })
        ),
        "Expected DiffInvalidated event to force diff recomputation when commit_diff.model_safe.txt is missing, got {:?}",
        result.event
    );
}

#[test]
fn test_prepare_commit_prompt_invalidates_materialized_inputs_when_diff_file_reference_missing() {
    let cloud = crate::config::types::CloudConfig::disabled();

    let workspace = MemoryWorkspace::new_test()
        .with_file(
            ".agent/tmp/commit_diff.txt",
            "diff --git a/a b/a\n+change\n",
        )
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor;
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.commit_diff_prepared = true;
    handler.state.commit_diff_empty = false;
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
                inline_budget_bytes: Some(1),
                representation: PromptInputRepresentation::FileReference {
                    path: std::path::PathBuf::from(".agent/tmp/commit_diff.model_safe.txt"),
                },
                reason: PromptMaterializationReason::InlineBudgetExceeded,
            },
        }),
        ..Default::default()
    };

    // The file reference points at `.agent/tmp/commit_diff.model_safe.txt` but it doesn't exist.
    // The handler should invalidate diff-prepared state by emitting DiffInvalidated, forcing
    // CheckCommitDiff (and subsequent rematerialization) on the next orchestration loop.
    let result = handler
        .prepare_commit_prompt(&mut ctx, PromptMode::Normal)
        .expect("prepare_commit_prompt should return an EffectResult");

    assert!(
        matches!(
            result.event,
            PipelineEvent::Commit(crate::reducer::event::CommitEvent::DiffInvalidated { .. })
        ),
        "Expected DiffInvalidated event to force diff recomputation when a diff file reference is missing, got {:?}",
        result.event
    );
}
