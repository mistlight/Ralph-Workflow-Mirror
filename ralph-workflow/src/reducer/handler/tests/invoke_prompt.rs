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
use crate::reducer::state::{AgentChainState, CommitState, PipelineState};
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_invoke_planning_agent_aborts_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    let result = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    assert!(matches!(result.event, PipelineEvent::Lifecycle(_)));
}

#[test]
fn test_invoke_planning_agent_does_not_mark_invoked_on_failure() {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "planning prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new().with_agent_result(
        "claude",
        Ok(crate::executor::AgentCommandResult::failure(1, "boom")),
    ));

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );

    let result = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Agent(AgentEvent::InvocationStarted { .. })
    ));
    assert!(
        result.additional_events.iter().any(|e| {
            matches!(
                e,
                PipelineEvent::Agent(AgentEvent::InvocationFailed { .. })
                    | PipelineEvent::Agent(AgentEvent::RateLimited { .. })
                    | PipelineEvent::Agent(AgentEvent::AuthFailed { .. })
                    | PipelineEvent::Agent(AgentEvent::TimedOut { .. })
            )
        }),
        "invoke_agent should emit a failure fact event after InvocationStarted"
    );
    assert!(
        !result
            .additional_events
            .iter()
            .any(|e| matches!(e, PipelineEvent::Lifecycle(_))),
        "planning agent invoked should not be emitted on failure"
    );
}

#[test]
fn test_invoke_planning_agent_uses_unique_logfile_path_with_attempt() {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "planning prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec!["model-a".to_string()]],
        crate::agents::AgentRole::Developer,
    );

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
    assert_eq!(
        calls[0].logfile, ".agent/logs/planning_1_claude_0_a0.log",
        "logfile should include phase, model index, and attempt suffix"
    );
}

#[test]
fn test_invoke_planning_agent_logfile_attempt_is_collision_free_and_does_not_depend_on_counter_magnitude(
) {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "planning prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec!["model-a".to_string()]],
        crate::agents::AgentRole::Developer,
    );

    // This should not affect logfile attempt selection.
    handler.state.agent_chain.retry_cycle = u32::MAX;
    handler.state.continuation.continuation_attempt = 1;
    handler.state.continuation.xsd_retry_count = 1;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        handler.invoke_planning_agent(&mut ctx, 0)
    }));
    assert!(
        result.is_ok(),
        "invoke_planning_agent should not panic when attempt counter would overflow"
    );

    let effect_result = result
        .unwrap()
        .expect("invoke_planning_agent should succeed");

    assert!(matches!(
        effect_result.event,
        PipelineEvent::Agent(AgentEvent::InvocationStarted { .. })
    ));
    assert!(effect_result.additional_events.iter().any(|e| {
        matches!(
            e,
            PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
        )
    }));

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].logfile, ".agent/logs/planning_1_claude_0_a0.log",
        "logfile attempt should start at a0 for first invocation"
    );
}

#[test]
fn test_invoke_planning_agent_logfile_attempt_does_not_collide_across_distinct_attempt_context() {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "planning prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec!["model-a".to_string()]],
        crate::agents::AgentRole::Developer,
    );

    // First invocation context: retry_cycle=0, continuation_attempt=100, xsd_retry_count=0
    handler.state.agent_chain.retry_cycle = 0;
    handler.state.continuation.continuation_attempt = 100;
    handler.state.continuation.xsd_retry_count = 0;
    let _ = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("first invoke_planning_agent should succeed");

    // Second invocation context: retry_cycle=1, continuation_attempt=0, xsd_retry_count=0
    // Under the old packed arithmetic scheme, both contexts can map to the same attempt value
    // and therefore collide on logfile paths.
    handler.state.agent_chain.retry_cycle = 1;
    handler.state.continuation.continuation_attempt = 0;
    handler.state.continuation.xsd_retry_count = 0;
    let _ = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("second invoke_planning_agent should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 2);
    assert_ne!(
        calls[0].logfile, calls[1].logfile,
        "logfile path must not collide across distinct attempt contexts"
    );
}

#[test]
fn test_invoke_development_agent_aborts_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    let result = handler
        .invoke_development_agent(&mut ctx, 0)
        .expect("invoke_development_agent should succeed");

    assert!(matches!(result.event, PipelineEvent::Lifecycle(_)));
}

#[test]
fn test_invoke_review_agent_aborts_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    let result = handler
        .invoke_review_agent(&mut ctx, 0)
        .expect("invoke_review_agent should succeed");

    assert!(matches!(result.event, PipelineEvent::Lifecycle(_)));
}

#[test]
fn test_invoke_fix_agent_aborts_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    let result = handler
        .invoke_fix_agent(&mut ctx, 0)
        .expect("invoke_fix_agent should succeed");

    assert!(matches!(result.event, PipelineEvent::Lifecycle(_)));
}

#[test]
fn test_invoke_commit_agent_aborts_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
        AgentRole::Commit,
    );

    let result = handler
        .invoke_commit_agent(&mut ctx)
        .expect("invoke_commit_agent should succeed");

    assert!(matches!(result.event, PipelineEvent::Lifecycle(_)));
}

/// Test that rate_limit_continuation_prompt is used when available.
///
/// When an agent hits a rate limit (429), the prompt is saved in
/// rate_limit_continuation_prompt. The next agent invocation should use
/// this saved prompt instead of any newly generated prompt.
#[test]
fn test_invoke_agent_uses_rate_limit_continuation_prompt() {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "fresh prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec!["model-a".to_string()]],
        crate::agents::AgentRole::Developer,
    );
    // Simulate that a previous agent hit rate limit and saved the prompt
    handler.state.agent_chain.rate_limit_continuation_prompt =
        Some("saved prompt from rate limit".to_string());

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

/// Test that when rate_limit_continuation_prompt is None, the fresh prompt is used.
#[test]
fn test_invoke_agent_uses_fresh_prompt_when_no_continuation_prompt() {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "fresh prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );

    let repo_root = PathBuf::from("/mock/repo");
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
