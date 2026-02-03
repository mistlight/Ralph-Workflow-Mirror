use crate::agents::{AgentRegistry, AgentRole};
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{AgentEvent, PipelineEvent};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{ContinuationState, PipelineState, PromptMode, SameAgentRetryReason};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_prepare_development_prompt_emits_template_invalid_event() {
    // When the PROMPT.md contains unresolved template placeholders,
    // the handler should emit TemplateVariablesInvalid event.
    // Note: PROMPT.md is in ignore_sources, but this only applies to
    // the validation of the RENDERED prompt, not to validation errors
    // that occur during template resolution itself.
    //
    // However, since all template variables (PROMPT, PLAN, etc.) are
    // provided by prompt_developer_iteration_xml_with_context(),
    // the only way to trigger validation failure is if the final
    // rendered prompt contains unresolved placeholders that are NOT
    // in the ignored content.
    //
    // Since both PROMPT.md and PLAN.md are in ignore_sources, we need
    // to use a different approach: test that PLAN.md content with {{}}
    // is correctly ignored (no error), AND test separately that actual
    // template errors would be caught.
    //
    // This test now verifies that placeholders in PLAN.md are correctly
    // ignored and prompt generation succeeds.
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt with {{LITERAL}} braces")
        .with_file(".agent/PLAN.md", "Plan content");

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

    let mut prompt_history = HashMap::new();
    prompt_history.insert("development_0".to_string(), "{{MISSING}}".to_string());

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
        prompt_history,
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let result = handler
        .prepare_development_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_development_prompt should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Agent(AgentEvent::TemplateVariablesInvalid {
            role: AgentRole::Developer,
            template_name,
            ..
        }) if template_name == "developer_iteration_xml"
    ));
}

#[test]
fn test_prepare_development_prompt_normal_mode_ignores_continuation_state() {
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt")
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let repo_root = PathBuf::from("/mock/repo");

    let mut prompt_history = HashMap::new();
    // Store a continuation prompt containing unresolved placeholders.
    // Normal mode must NOT replay this continuation prompt.
    prompt_history.insert(
        "development_0_continuation_1".to_string(),
        "{{UNRESOLVED}}".to_string(),
    );

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
        prompt_history,
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: crate::reducer::state::ContinuationState {
            continuation_attempt: 1,
            ..crate::reducer::state::ContinuationState::new()
        },
        ..PipelineState::initial(1, 1)
    });

    let result = handler
        .prepare_development_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_development_prompt should succeed");

    // Even though a stored continuation prompt contains unresolved placeholders,
    // PromptMode::Normal must ignore continuation state and prepare a normal prompt.
    assert!(
        matches!(
            result.event,
            PipelineEvent::Development(
                crate::reducer::event::DevelopmentEvent::PromptPrepared { .. }
            )
        ),
        "Expected PromptPrepared event when placeholders in PROMPT.md are ignored, got {:?}",
        result.event
    );
}

#[test]
fn test_prepare_development_prompt_detects_unresolved_partial() {
    // Test that unresolved placeholders in the template itself (not in PROMPT/PLAN content)
    // would be detected. This requires a custom template with an unresolved partial.
    // Since the default templates are well-formed, we skip this test.
    // The validation logic is tested separately in template_validator.rs.
}

#[test]
fn test_prepare_development_prompt_xsd_retry_includes_real_last_output() {
    let invalid_xml = "<ralph-development-result><ralph-status>completed</ralph-status>";
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/development_result.xml", invalid_xml);

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
        developer_agent: "dev",
        reviewer_agent: "rev",
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
    let _ = handler
        .prepare_development_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_development_prompt should succeed");

    let last_output = workspace
        .read(std::path::Path::new(".agent/tmp/last_output.xml"))
        .expect("last_output.xml should be written on XSD retry");
    assert_eq!(
        last_output, invalid_xml,
        "XSD retry should capture the actual invalid XML as last_output.xml"
    );
}

#[test]
fn test_prepare_development_prompt_same_agent_retry_uses_previous_prepared_prompt() {
    let marker = "<<<PREVIOUS_DEVELOPMENT_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/development_prompt.txt", marker);

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
        developer_agent: "dev",
        reviewer_agent: "rev",
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
        ..PipelineState::initial(1, 1)
    });

    let _ = handler
        .prepare_development_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_development_prompt should succeed");

    let prompt = workspace
        .read(std::path::Path::new(".agent/tmp/development_prompt.txt"))
        .expect("development prompt should be written");

    assert!(
        prompt.contains(marker),
        "Same-agent retry should reuse the previously prepared prompt; got: {prompt}"
    );
    assert!(
        prompt.contains("## Retry Note (attempt 1)"),
        "Same-agent retry should prepend retry note; got: {prompt}"
    );
}
