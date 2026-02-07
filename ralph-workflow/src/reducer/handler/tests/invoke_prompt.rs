use crate::agents::{AgentRegistry, AgentRole};
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::logging::RunLogContext;
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{AgentEvent, PipelineEvent};
use crate::reducer::event::{ErrorEvent, WorkspaceIoErrorKind};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{AgentChainState, CommitState, PipelineState};
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
struct ReadFailingWorkspace {
    inner: MemoryWorkspace,
    forbidden_read_path: PathBuf,
    error_kind: io::ErrorKind,
}

impl ReadFailingWorkspace {
    fn new(
        inner: MemoryWorkspace,
        forbidden_read_path: PathBuf,
        error_kind: io::ErrorKind,
    ) -> Self {
        Self {
            inner,
            forbidden_read_path,
            error_kind,
        }
    }
}

impl crate::workspace::Workspace for ReadFailingWorkspace {
    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn read(&self, relative: &Path) -> io::Result<String> {
        if relative == self.forbidden_read_path.as_path() {
            return Err(io::Error::new(self.error_kind, "read forbidden (test)"));
        }
        self.inner.read(relative)
    }

    fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
        self.inner.read_bytes(relative)
    }

    fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
        self.inner.write(relative, content)
    }

    fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.inner.write_bytes(relative, content)
    }

    fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.inner.append_bytes(relative, content)
    }

    fn exists(&self, relative: &Path) -> bool {
        self.inner.exists(relative)
    }

    fn is_file(&self, relative: &Path) -> bool {
        self.inner.is_file(relative)
    }

    fn is_dir(&self, relative: &Path) -> bool {
        self.inner.is_dir(relative)
    }

    fn remove(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove(relative)
    }

    fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_if_exists(relative)
    }

    fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(relative)
    }

    fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_dir_all_if_exists(relative)
    }

    fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.inner.create_dir_all(relative)
    }

    fn read_dir(&self, relative: &Path) -> io::Result<Vec<crate::workspace::DirEntry>> {
        self.inner.read_dir(relative)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }

    fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
        self.inner.write_atomic(relative, content)
    }

    fn set_readonly(&self, relative: &Path) -> io::Result<()> {
        self.inner.set_readonly(relative)
    }

    fn set_writable(&self, relative: &Path) -> io::Result<()> {
        self.inner.set_writable(relative)
    }
}

#[test]
fn test_invoke_planning_agent_returns_error_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect_err("invoke_planning_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("planning prompt"),
        "Expected error about missing planning prompt, got: {err}"
    );
}

#[test]
fn test_invoke_planning_agent_maps_non_not_found_prompt_read_errors_to_workspace_read_failed() {
    let inner = MemoryWorkspace::new_test();
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/tmp/planning_prompt.txt"),
        io::ErrorKind::PermissionDenied,
    );

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect_err("invoke_planning_agent should error on non-NotFound prompt read");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/tmp/planning_prompt.txt"
        ),
        "expected WorkspaceReadFailed, got: {error_event:?}"
    );
}

#[test]
fn test_invoke_planning_agent_does_not_mark_invoked_on_failure() {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "planning prompt");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new().with_agent_result(
        "claude",
        Ok(crate::executor::AgentCommandResult::failure(1, "boom")),
    ));

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
        run_log_context: &run_log_context,
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
        run_log_context: &run_log_context,
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
    // New per-run log format: .agent/logs-<run_id>/agents/planning_1.log
    // Agent identity is in the log file header, not the filename
    assert!(
        calls[0].logfile.contains("/agents/planning_1.log"),
        "logfile should use per-run format with phase_index naming: {}",
        calls[0].logfile
    );
}

#[test]
fn test_invoke_agent_prefers_same_agent_retry_prompt_over_rate_limit_continuation_prompt() {
    let workspace = MemoryWorkspace::new_test();
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );
    let saved_prompt = "CONTINUATION PROMPT (stale)".to_string();
    handler.state.agent_chain.rate_limit_continuation_prompt =
        Some(crate::reducer::state::RateLimitContinuationPrompt {
            role: crate::agents::AgentRole::Developer,
            prompt: saved_prompt.clone(),
        });
    handler.state.continuation.same_agent_retry_count = 1;
    handler.state.continuation.same_agent_retry_reason =
        Some(crate::reducer::state::SameAgentRetryReason::Timeout);

    let retry_preamble = crate::reducer::handler::retry_guidance::same_agent_retry_preamble(
        &handler.state.continuation,
    );
    let retry_prompt = format!(
        "{retry_preamble}\n\
ORIGINAL PROMPT BODY\n\
RETRY PROMPT MARKER"
    );

    let _ = handler
        .invoke_agent(
            &mut ctx,
            AgentRole::Developer,
            "claude".to_string(),
            None,
            retry_prompt.clone(),
        )
        .expect("invoke_agent should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].prompt.contains("RETRY PROMPT MARKER"),
        "retry prompt marker should be present in effective prompt"
    );
    assert!(
        calls[0].prompt.starts_with("## Retry Note"),
        "effective prompt should preserve same-agent retry preamble"
    );
    assert!(
        !calls[0].prompt.contains("CONTINUATION PROMPT (stale)"),
        "effective prompt should not be overwritten by stale continuation prompt"
    );
}

#[test]
fn test_invoke_agent_prefers_xsd_retry_prompt_over_rate_limit_continuation_prompt() {
    let workspace = MemoryWorkspace::new_test();
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );
    handler.state.agent_chain.rate_limit_continuation_prompt =
        Some(crate::reducer::state::RateLimitContinuationPrompt {
            role: crate::agents::AgentRole::Developer,
            prompt: "CONTINUATION PROMPT (stale)".to_string(),
        });
    handler.state.continuation.xsd_retry_session_reuse_pending = true;

    let xsd_retry_prompt = "XSD RETRY PROMPT MARKER".to_string();
    let _ = handler
        .invoke_agent(
            &mut ctx,
            AgentRole::Developer,
            "claude".to_string(),
            None,
            xsd_retry_prompt.clone(),
        )
        .expect("invoke_agent should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].prompt, xsd_retry_prompt,
        "effective prompt should use the XSD retry prompt, not stale continuation prompt"
    );
}

#[test]
fn test_invoke_analysis_agent_does_not_use_rate_limit_continuation_prompt() {
    use crate::agents::AgentRole;
    use crate::executor::AgentCommandResult;

    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/PLAN.md", "# Plan\n\n- Do the thing\n");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new().with_agent_result("claude", Ok(AgentCommandResult::success())),
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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.phase = crate::reducer::event::PipelinePhase::Development;
    handler.state.iteration = 0;
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        AgentRole::Developer,
    );
    let saved_prompt = "CONTINUATION PROMPT (stale)".to_string();
    handler.state.agent_chain.rate_limit_continuation_prompt =
        Some(crate::reducer::state::RateLimitContinuationPrompt {
            role: crate::agents::AgentRole::Developer,
            prompt: saved_prompt.clone(),
        });

    let _ = handler
        .invoke_analysis_agent(&mut ctx, 0)
        .expect("invoke_analysis_agent should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0]
            .prompt
            .contains("independent, objective code analysis agent"),
        "analysis invocation should use analysis prompt, not a stale continuation prompt"
    );
    assert_ne!(
        calls[0].prompt, saved_prompt,
        "analysis invocation must not be overridden by a role-mismatched continuation prompt"
    );
}

#[test]
fn test_xsd_retry_reuses_session_id_even_after_prompt_prepared_clears_pending() {
    use crate::reducer::state_reduction::reduce;

    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "planning prompt");
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
        run_log_context: &run_log_context,
    };

    let session_id = "session-123".to_string();

    // Simulate an XSD retry: XsdValidationFailed sets xsd_retry_pending=true, then the
    // pipeline prepares an XSD retry prompt and (currently) clears xsd_retry_pending.
    let mut state = PipelineState::initial(1, 0);
    state.agent_chain = AgentChainState::initial()
        .with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            crate::agents::AgentRole::Developer,
        )
        .with_session_id(Some(session_id.clone()));
    state.continuation.xsd_retry_pending = true;
    state.continuation.xsd_retry_session_reuse_pending = true;

    state = reduce(state, PipelineEvent::planning_prompt_prepared(0));
    assert!(
        !state.continuation.xsd_retry_pending,
        "prompt preparation clears xsd_retry_pending before invocation"
    );

    let mut handler = MainEffectHandler::new(state);
    let _ = handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].args.iter().any(|a| a == "--resume"),
        "agent command should include session continuation flag for XSD retry"
    );
    assert!(
        calls[0].args.iter().any(|a| a == &session_id),
        "agent command should include session id value for XSD retry"
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
        run_log_context: &run_log_context,
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
    // New per-run log format: .agent/logs-<run_id>/agents/planning_1.log
    // Agent identity is in the log file header, not the filename
    assert!(
        calls[0].logfile.contains("/agents/planning_1.log"),
        "logfile should use per-run format with phase_index naming: {}",
        calls[0].logfile
    );
}

#[test]
fn test_invoke_planning_agent_logfile_attempt_does_not_collide_across_distinct_attempt_context() {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/tmp/planning_prompt.txt", "planning prompt");
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
        run_log_context: &run_log_context,
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
fn test_invoke_development_agent_returns_error_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_development_agent(&mut ctx, 0)
        .expect_err("invoke_development_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("development prompt"),
        "Expected error about missing development prompt, got: {err}"
    );
}

#[test]
fn test_invoke_review_agent_returns_error_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_review_agent(&mut ctx, 0)
        .expect_err("invoke_review_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("review prompt"),
        "Expected error about missing review prompt, got: {err}"
    );
}

#[test]
fn test_invoke_review_agent_maps_non_not_found_prompt_read_errors_to_workspace_read_failed() {
    let inner = MemoryWorkspace::new_test();
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/tmp/review_prompt.txt"),
        io::ErrorKind::PermissionDenied,
    );

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_review_agent(&mut ctx, 0)
        .expect_err("invoke_review_agent should error on non-NotFound prompt read");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/tmp/review_prompt.txt"
        ),
        "expected WorkspaceReadFailed, got: {error_event:?}"
    );
}

#[test]
fn test_invoke_fix_agent_returns_error_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_fix_agent(&mut ctx, 0)
        .expect_err("invoke_fix_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("fix prompt"),
        "Expected error about missing fix prompt, got: {err}"
    );
}

#[test]
fn test_invoke_fix_agent_maps_non_not_found_prompt_read_errors_to_workspace_read_failed() {
    let inner = MemoryWorkspace::new_test();
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/tmp/fix_prompt.txt"),
        io::ErrorKind::PermissionDenied,
    );

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    let err = handler
        .invoke_fix_agent(&mut ctx, 0)
        .expect_err("invoke_fix_agent should error on non-NotFound prompt read");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/tmp/fix_prompt.txt"
        ),
        "expected WorkspaceReadFailed, got: {error_event:?}"
    );
}

#[test]
fn test_invoke_commit_agent_returns_error_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test();
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
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

    let err = handler
        .invoke_commit_agent(&mut ctx)
        .expect_err("invoke_commit_agent should return error when prompt missing");

    assert!(
        err.to_string().contains("commit prompt"),
        "Expected error about missing commit prompt, got: {err}"
    );
}

#[test]
fn test_invoke_commit_agent_maps_non_not_found_prompt_read_errors_to_workspace_read_failed() {
    let inner = MemoryWorkspace::new_test();
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/tmp/commit_prompt.txt"),
        io::ErrorKind::PermissionDenied,
    );

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
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

    let err = handler
        .invoke_commit_agent(&mut ctx)
        .expect_err("invoke_commit_agent should error on non-NotFound prompt read");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/tmp/commit_prompt.txt"
        ),
        "expected WorkspaceReadFailed, got: {error_event:?}"
    );
}

#[test]
fn test_invoke_commit_agent_surfaces_uninitialized_agent_chain_as_error_event() {
    // When the agent chain is empty/uninitialized, invoke_commit_agent must not panic.
    // It must surface a typed ErrorEvent so the reducer can decide interruption policy.
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_prompt.txt", "commit prompt content");
    let _run_log_context = RunLogContext::new(&workspace).unwrap();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());

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
        run_log_context: &run_log_context,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    // Intentionally leave the agent chain uninitialized/empty.
    handler.state.agent_chain = AgentChainState::initial();

    let err = handler
        .invoke_commit_agent(&mut ctx)
        .expect_err("invoke_commit_agent should return typed error when agent chain is empty");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::CommitAgentNotInitialized { attempt: 1 }
        ),
        "expected CommitAgentNotInitialized, got: {error_event:?}"
    );

    // Defensive: ensure the error type is not a string-based anyhow error.
    assert!(
        !matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                kind: WorkspaceIoErrorKind::Other,
                ..
            }
        ),
        "expected a specific invariant error, not a generic workspace error"
    );
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
        run_log_context: &run_log_context,
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

/// Test that when rate_limit_continuation_prompt is None, the fresh prompt is used.
#[test]
fn test_invoke_agent_uses_fresh_prompt_when_no_continuation_prompt() {
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
        run_log_context: &run_log_context,
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
