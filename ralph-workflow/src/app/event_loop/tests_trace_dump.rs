#[test]
fn test_dump_event_loop_trace_creates_parent_dir_before_write() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::workspace::{MemoryWorkspace, Workspace};
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[derive(Debug)]
    struct StrictTmpWorkspace {
        inner: MemoryWorkspace,
        tmp_created: AtomicBool,
    }

    impl StrictTmpWorkspace {
        fn new(inner: MemoryWorkspace) -> Self {
            Self {
                inner,
                tmp_created: AtomicBool::new(false),
            }
        }
    }

    impl Workspace for StrictTmpWorkspace {
        fn root(&self) -> &Path {
            self.inner.root()
        }

        fn read(&self, relative: &Path) -> io::Result<String> {
            self.inner.read(relative)
        }

        fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
            self.inner.read_bytes(relative)
        }

        fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
            if relative == Path::new(EVENT_LOOP_TRACE_PATH)
                && !self.tmp_created.load(Ordering::Acquire)
            {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "parent dir missing (strict workspace)",
                ));
            }
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
            if relative == Path::new(".agent/tmp") {
                self.tmp_created.store(true, Ordering::Release);
            }
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

    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let strict_workspace = StrictTmpWorkspace::new(MemoryWorkspace::new(repo_root.clone()));

    let mut ctx = PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: "test-developer",
        reviewer_agent: "test-reviewer",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
        executor: &*executor,
        executor_arc: Arc::clone(&executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: &repo_root,
        workspace: &strict_workspace,
    };

    let mut trace = EventTraceBuffer::new(1);
    let state = PipelineState::initial(1, 0);
    trace.push(build_trace_entry(0, &state, "Effect::None", "Event::None"));

    let dumped = dump_event_loop_trace(&mut ctx, &trace, &state, "test");
    assert!(
        dumped,
        "expected trace dump to succeed even when .agent/tmp is missing"
    );
    assert!(
        strict_workspace
            .inner
            .exists(Path::new(EVENT_LOOP_TRACE_PATH)),
        "expected trace file to be created"
    );
}

#[test]
fn test_extract_error_event_searches_anyhow_error_chain() {
    use crate::reducer::event::ErrorEvent;
    use std::error::Error;
    use std::fmt;

    #[derive(Debug)]
    struct WrapperError {
        source: ErrorEvent,
    }

    impl fmt::Display for WrapperError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "wrapper")
        }
    }

    impl Error for WrapperError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.source)
        }
    }

    let wrapped: anyhow::Error = anyhow::Error::new(WrapperError {
        source: ErrorEvent::FixPromptMissing,
    });

    let extracted = super::extract_error_event(&wrapped)
        .expect("expected ErrorEvent to be found in error chain");
    assert!(matches!(extracted, ErrorEvent::FixPromptMissing));
}

#[test]
fn test_event_loop_dumps_trace_on_unrecoverable_handler_error() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::phases::PhaseContext;
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
    use crate::reducer::state::PipelineState;
    use crate::workspace::{MemoryWorkspace, Workspace};
    use anyhow::Result;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    #[derive(Debug)]
    struct UnrecoverableErrorHandler {
        state: PipelineState,
    }

    impl EffectHandler<'_> for UnrecoverableErrorHandler {
        fn execute(
            &mut self,
            _effect: Effect,
            _ctx: &mut PhaseContext<'_>,
        ) -> Result<EffectResult> {
            Err(anyhow::anyhow!("boom"))
        }
    }

    impl super::StatefulHandler for UnrecoverableErrorHandler {
        fn update_state(&mut self, state: PipelineState) {
            self.state = state;
        }
    }

    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());

    let mut ctx = PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: "test-developer",
        reviewer_agent: "test-reviewer",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
        executor: &*executor,
        executor_arc: Arc::clone(&executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: &repo_root,
        workspace: &workspace,
    };

    let state = PipelineState::initial(1, 0);
    let mut handler = UnrecoverableErrorHandler {
        state: state.clone(),
    };
    let loop_config = super::EventLoopConfig { max_iterations: 10 };

    let result =
        super::run_event_loop_with_handler(&mut ctx, Some(state), loop_config, &mut handler)
            .expect("event loop should return an EventLoopResult even on unrecoverable errors");
    assert!(
        !result.completed,
        "expected unrecoverable handler error to be reported as incomplete"
    );

    assert!(
        workspace.exists(Path::new(super::EVENT_LOOP_TRACE_PATH)),
        "expected trace file to be dumped on unrecoverable handler error"
    );
    assert!(
        workspace.exists(Path::new(".agent/tmp/completion_marker")),
        "expected completion marker to be written on unrecoverable handler error"
    );
}

#[test]
fn test_event_loop_config_creation() {
    let config = EventLoopConfig {
        max_iterations: 1000,
    };
    assert_eq!(config.max_iterations, 1000);
}

#[test]
fn test_create_initial_state_with_config_counts_total_attempts() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;
    use std::sync::Arc;

    // Semantics: max_dev_continuations counts *continuations beyond initial*.
    // Total attempts should be 1 + max_dev_continuations.
    let config = Config {
        max_dev_continuations: Some(2),
        max_xsd_retries: Some(10),
        ..Config::default()
    };

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());

    let ctx = PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: "test-developer",
        reviewer_agent: "test-reviewer",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
        executor: &*executor,
        executor_arc: Arc::clone(&executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: &repo_root,
        workspace: &workspace,
    };

    let state = create_initial_state_with_config(&ctx);

    assert_eq!(
        state.continuation.max_continue_count, 3,
        "max_continue_count should be total attempts (1 + max_dev_continuations)"
    );
}

/// Regression test: event loop must apply EffectResult.additional_events.
///
/// Without this, AgentEvent::SessionEstablished is never reduced and same-session
/// XSD retry cannot work.
#[test]
fn test_event_loop_applies_additional_events_in_order() {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::{ExecutionHistory, RunContext};
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::reducer::effect::{Effect, EffectHandler, EffectResult};
    use crate::reducer::PipelineEvent;
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Debug)]
    struct TestHandler {
        state: PipelineState,
    }

    impl TestHandler {
        fn new(state: PipelineState) -> Self {
            Self { state }
        }
    }

    impl<'ctx> EffectHandler<'ctx> for TestHandler {
        fn execute(
            &mut self,
            _effect: Effect,
            _ctx: &mut PhaseContext<'_>,
        ) -> Result<EffectResult> {
            Ok(
                EffectResult::event(PipelineEvent::prompt_permissions_restored())
                    .with_additional_event(PipelineEvent::agent_session_established(
                        crate::agents::AgentRole::Developer,
                        "test-agent".to_string(),
                        "session-123".to_string(),
                    )),
            )
        }
    }

    impl super::StatefulHandler for TestHandler {
        fn update_state(&mut self, state: PipelineState) {
            self.state = state;
        }
    }

    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());

    let mut ctx = PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: "test-developer",
        reviewer_agent: "test-reviewer",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
        executor: &*executor,
        executor_arc: Arc::clone(&executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: &repo_root,
        workspace: &workspace,
    };

    let state = PipelineState::initial(1, 0);
    let mut handler = TestHandler::new(state);
    let loop_config = EventLoopConfig { max_iterations: 10 };

    let result = run_event_loop_with_handler(
        &mut ctx,
        Some(PipelineState::initial(1, 0)),
        loop_config,
        &mut handler,
    )
    .expect("event loop should run");

    assert!(
        result.completed,
        "pipeline should complete (PromptPermissionsRestored)"
    );
    assert_eq!(
        handler.state.agent_chain.last_session_id.as_deref(),
        Some("session-123"),
        "additional SessionEstablished event should be reduced and stored"
    );
}
