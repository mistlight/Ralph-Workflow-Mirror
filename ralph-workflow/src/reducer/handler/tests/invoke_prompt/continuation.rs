//! Rate limit continuation prompt tests
//!
//! Tests for rate limit continuation prompt behavior:
//! - Using rate limit continuation prompt when available
//! - Using fresh prompt when no continuation prompt exists
//! - Analysis agent does not use continuation prompts

use super::*;

#[test]
fn test_invoke_agent_uses_rate_limit_continuation_prompt() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "fresh prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );

    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec!["model-a".to_string()]],
        crate::agents::AgentRole::Developer,
    );
    // Simulate that a previous agent hit rate limit and saved the prompt
    handler.state.agent_chain.rate_limit_continuation_prompt =
        Some(crate::reducer::state::RateLimitContinuationPrompt {
            role: crate::agents::AgentRole::Developer,
            prompt: "saved prompt from rate limit".to_string(),
        });

    let result = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Agent(AgentEvent::InvocationStarted { .. })
    ));
    assert!(result.additional_events.iter().any(|e| {
        matches!(
            e,
            PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
        )
    }));

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    // The saved prompt should have been used instead of "fresh prompt"
    assert_eq!(
        calls[0].prompt, "saved prompt from rate limit",
        "Agent should use rate_limit_continuation_prompt when available"
    );
}

/// Test that when `rate_limit_continuation_prompt` is None, the fresh prompt is used.
#[test]
fn test_invoke_agent_uses_fresh_prompt_when_no_continuation_prompt() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "fresh prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );

    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud: &cloud,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec!["model-a".to_string()]],
        crate::agents::AgentRole::Developer,
    );
    // No rate_limit_continuation_prompt set
    assert!(handler
        .state
        .agent_chain
        .rate_limit_continuation_prompt
        .is_none());

    let result = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Agent(AgentEvent::InvocationStarted { .. })
    ));
    assert!(result.additional_events.iter().any(|e| {
        matches!(
            e,
            PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
        )
    }));

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    // The fresh prompt should have been used
    assert_eq!(
        calls[0].prompt, "fresh prompt",
        "Agent should use fresh prompt when no rate_limit_continuation_prompt"
    );
}
