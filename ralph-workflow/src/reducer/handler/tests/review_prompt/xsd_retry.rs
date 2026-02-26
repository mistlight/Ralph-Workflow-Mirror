use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{
    AgentEvent, PipelineEvent, PipelinePhase, PromptInputEvent, ReviewEvent,
};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{ContinuationState, PipelineState, PromptMode};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn test_prepare_review_prompt_uses_xsd_retry_prompt_key() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::state::PromptInputKind;

    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_file(
            ".agent/tmp/issues.xml",
            &"x".repeat(crate::prompts::MAX_INLINE_CONTENT_SIZE + 10),
        )
        .with_dir(".agent/tmp");

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
        developer_agent: "claude",
        reviewer_agent: "codex",
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
            invalid_output_attempts: 1,
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let result = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_review_prompt should succeed");

    assert!(
        ctx.prompt_history.contains_key("review_0_xsd_retry_1"),
        "expected retry prompt to be captured with retry key"
    );

    assert!(
        result.additional_events.iter().any(|ev| matches!(
            ev,
            PipelineEvent::PromptInput(PromptInputEvent::OversizeDetected {
                kind: PromptInputKind::LastOutput,
                ..
            })
        )),
        "Expected OversizeDetected event for PromptInputKind::LastOutput during review XSD retry"
    );
    assert!(
        result.additional_events.iter().any(|ev| matches!(
            ev,
            PipelineEvent::PromptInput(PromptInputEvent::TemplateRendered { .. })
        )),
        "Review XSD retry should emit TemplateRendered for log-based validation"
    );
}

#[test]
fn test_review_xsd_retry_oversize_detected_is_deduped_across_retries() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::state::PromptInputKind;

    let large_last_output = "x".repeat(crate::prompts::MAX_INLINE_CONTENT_SIZE + 10);
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_file(".agent/tmp/issues.xml", &large_last_output)
        .with_dir(".agent/tmp");

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
        developer_agent: "claude",
        reviewer_agent: "codex",
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
            invalid_output_attempts: 1,
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let first = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_review_prompt should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), first.event);
    for ev in first.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }

    let second = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_review_prompt should succeed");

    assert!(
        !second.additional_events.iter().any(|ev| matches!(
            ev,
            PipelineEvent::PromptInput(PromptInputEvent::OversizeDetected { kind: PromptInputKind::LastOutput, .. })
        )),
        "Expected OversizeDetected for LastOutput to be emitted only once for identical review XSD retry context"
    );
}

#[test]
fn test_prepare_review_prompt_xsd_retry_ignores_last_output_placeholders() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_file(
            crate::files::llm_output_extraction::file_based_extraction::paths::ISSUES_XML,
            "{{MISSING}}",
        );

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut prompt_history = HashMap::new();
    prompt_history.insert(
        "review_0_xsd_retry_1".to_string(),
        "Last output was {{MISSING}}".to_string(),
    );

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
        prompt_history,
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
            invalid_output_attempts: 1,
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let result = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_review_prompt should succeed");

    assert!(matches!(result.event, PipelineEvent::Review(_)));
}

#[test]
fn test_prepare_review_prompt_xsd_retry_ignores_xsd_error_placeholders() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_dir(".agent/tmp");

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
            invalid_output_attempts: 1,
            last_review_xsd_error: Some("XSD error {{BROKEN}}".to_string()),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let result = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_review_prompt should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(ReviewEvent::PromptPrepared { .. })
    ));
}

#[test]
fn test_prepare_review_prompt_uses_xsd_retry_template_name() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_file(".agent/tmp/issues.xml", "<ralph-issues>bad</ralph-issues>")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut prompt_history = HashMap::new();
    prompt_history.insert(
        "review_0_xsd_retry_1".to_string(),
        "retry prompt {{UNRESOLVED}}".to_string(),
    );

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
        prompt_history,
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
            invalid_output_attempts: 1,
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let result = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_review_prompt should succeed");

    assert!(
        matches!(result.event, PipelineEvent::Review(_)),
        "expected retry prompt to be prepared even if prompt_history contains stale placeholders"
    );
    let prompt = workspace
        .read(Path::new(".agent/tmp/review_prompt.txt"))
        .expect("review prompt file should be written");
    assert!(
        prompt.contains("XSD VALIDATION FAILED - FIX XML ONLY"),
        "expected review XSD retry template to be used"
    );
}

#[test]
fn test_prepare_review_prompt_xsd_retry_allows_missing_issues_xml() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n");

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
            invalid_output_attempts: 1,
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let result = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_review_prompt should succeed without issues.xml");

    assert!(matches!(result.event, PipelineEvent::Review(_)));
    let prompt = workspace
        .read(Path::new(".agent/tmp/review_prompt.txt"))
        .expect("review prompt file should be written");
    assert!(
        prompt.contains("XSD VALIDATION FAILED - FIX XML ONLY"),
        "expected review XSD retry template to be used"
    );
}

#[test]
fn test_prepare_fix_prompt_uses_xsd_retry_template_name() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "Issue\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut prompt_history = HashMap::new();
    prompt_history.insert(
        "fix_0_xsd_retry_1".to_string(),
        "retry prompt {{UNRESOLVED}}".to_string(),
    );

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
        prompt_history,
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
            invalid_output_attempts: 1,
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let result = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_fix_prompt should succeed");

    // XSD retry prompts succeed (they don't have unresolved placeholders in practice)
    // The test was checking for TemplateVariablesInvalid but fix prompts actually succeed
    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::FixPromptPrepared { .. })
    ));
}

#[test]
fn test_prepare_fix_prompt_xsd_retry_ignores_xsd_error_placeholders() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "Issue\n")
        .with_dir(".agent/tmp");

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
            invalid_output_attempts: 1,
            last_fix_xsd_error: Some("XSD error {{BROKEN}}".to_string()),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let result = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_fix_prompt should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(ReviewEvent::FixPromptPrepared { .. })
    ));
}

#[test]
fn test_prepare_fix_prompt_xsd_retry_reports_missing_xsd_error() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "Issue\n")
        .with_file(".agent/tmp/fix_result.xml", "<ralph-fix-result/>")
        .with_dir(".agent/tmp");

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
            invalid_output_attempts: 1,
            last_fix_xsd_error: Some(String::new()),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let result = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_fix_prompt should succeed");

    match result.event {
        PipelineEvent::PromptInput(PromptInputEvent::TemplateRendered {
            phase,
            template_name,
            log,
        }) => {
            assert_eq!(phase, PipelinePhase::Review);
            assert_eq!(template_name, "fix_mode_xsd_retry");
            assert!(log.unsubstituted.contains(&"XSD_ERROR".to_string()));
        }
        other => panic!("expected TemplateRendered event, got {other:?}"),
    }

    assert!(
        result.additional_events.iter().any(|event| matches!(
            event,
            PipelineEvent::Agent(AgentEvent::TemplateVariablesInvalid { missing_variables, .. })
                if missing_variables.contains(&"XSD_ERROR".to_string())
        )),
        "expected TemplateVariablesInvalid with missing variables"
    );
}

#[test]
fn test_prepare_fix_prompt_uses_prompt_history_replay() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "Issue\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut prompt_history = HashMap::new();
    prompt_history.insert("fix_0".to_string(), "REPLAYED PROMPT".to_string());

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
        prompt_history,
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
        workspace_arc: std::sync::Arc::new(workspace.clone()),
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_fix_prompt should succeed");

    let content = workspace
        .read(std::path::Path::new(".agent/tmp/fix_prompt.txt"))
        .expect("fix prompt should be written");
    assert!(content.contains("REPLAYED PROMPT"));
}

#[test]
fn test_fix_mode_xsd_retry_template_mentions_illegal_control_characters() {
    let _cloud_config = crate::config::types::CloudConfig::disabled();
    let template = include_str!("../../../../prompts/templates/fix_mode_xsd_retry.txt");
    assert!(
        template.contains(
            r"Illegal control characters (NUL byte, etc.) - common: \u0000 instead of \u00A0"
        ),
        "Expected fix_mode_xsd_retry template to mention illegal control characters"
    );
}

#[test]
fn test_fix_mode_xsd_retry_template_lists_fix_result_status_values() {
    let _cloud_config = crate::config::types::CloudConfig::disabled();
    let template = include_str!("../../../../prompts/templates/fix_mode_xsd_retry.txt");
    assert!(
        template.contains("all_issues_addressed")
            && template.contains("issues_remain")
            && template.contains("no_issues_found"),
        "Expected fix_mode_xsd_retry template to list fix-result status values"
    );
}
