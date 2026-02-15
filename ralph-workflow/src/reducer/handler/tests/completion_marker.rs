use crate::agents::AgentRegistry;
use crate::agents::AgentRole;
use crate::checkpoint::{ExecutionHistory, RunContext};
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::phases::PhaseContext;
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::effect::{Effect, EffectHandler};
use crate::reducer::event::{AwaitingDevFixEvent, PipelineEvent, PipelinePhase};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
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
        if relative == Path::new(".agent/tmp/completion_marker")
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

#[derive(Debug)]
struct FailingMarkerWorkspace {
    inner: MemoryWorkspace,
}

impl FailingMarkerWorkspace {
    fn new(inner: MemoryWorkspace) -> Self {
        Self { inner }
    }
}

impl Workspace for FailingMarkerWorkspace {
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
        if relative == Path::new(".agent/tmp/completion_marker") {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulated marker write failure",
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

/// Parameters for building a test PhaseContext.
/// Groups related parameters to avoid clippy::too_many_arguments.
struct ContextParams<'a> {
    workspace: &'a dyn Workspace,
    repo_root: &'a Path,
    executor: &'a Arc<MockProcessExecutor>,
    config: &'a Config,
    registry: &'a AgentRegistry,
    logger: &'a Logger,
    colors: &'a Colors,
    template_context: &'a TemplateContext,
    timer: &'a mut Timer,
    run_log_context: &'a crate::logging::RunLogContext,
}

fn build_context<'a>(params: ContextParams<'a>) -> PhaseContext<'a> {
    PhaseContext {
        config: params.config,
        registry: params.registry,
        logger: params.logger,
        colors: params.colors,
        timer: params.timer,
        developer_agent: "test-developer",
        reviewer_agent: "test-reviewer",
        review_guidelines: None,
        template_context: params.template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
        executor: &**params.executor,
        executor_arc: Arc::clone(params.executor) as Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: params.repo_root,
        workspace: params.workspace,
        run_log_context: params.run_log_context,
    }
}

#[test]
fn emit_completion_marker_creates_tmp_dir_before_write() {
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = StrictTmpWorkspace::new(MemoryWorkspace::new(repo_root.clone()));
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let mut ctx = build_context(ContextParams {
        workspace: &workspace,
        repo_root: &repo_root,
        executor: &executor,
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        template_context: &template_context,
        timer: &mut timer,
        run_log_context: &run_log_context,
    });

    let state = PipelineState::initial(1, 0);
    let mut handler = MainEffectHandler::new(state);

    let result = handler.execute(
        Effect::EmitCompletionMarkerAndTerminate {
            is_failure: true,
            reason: Some("unit test".to_string()),
        },
        &mut ctx,
    );

    assert!(
        result.is_ok(),
        "EmitCompletionMarkerAndTerminate should succeed"
    );

    let marker_path = Path::new(".agent/tmp/completion_marker");
    assert!(
        workspace.exists(marker_path),
        "Completion marker should exist"
    );
}

#[test]
fn emit_completion_marker_with_write_failure_emits_event() {
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = FailingMarkerWorkspace::new(MemoryWorkspace::new(repo_root.clone()));
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let mut ctx = build_context(ContextParams {
        workspace: &workspace,
        repo_root: &repo_root,
        executor: &executor,
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        template_context: &template_context,
        timer: &mut timer,
        run_log_context: &run_log_context,
    });

    let state = PipelineState::initial(1, 0);
    let mut handler = MainEffectHandler::new(state);

    let result = handler.execute(
        Effect::EmitCompletionMarkerAndTerminate {
            is_failure: true,
            reason: Some("unit test".to_string()),
        },
        &mut ctx,
    );

    assert!(
        result.is_ok(),
        "EmitCompletionMarkerAndTerminate should emit event even if write fails"
    );

    let event = result.expect("Expected effect result").event;
    assert!(matches!(
        event,
        PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
            is_failure: true
        })
    ));
}

#[test]
fn trigger_dev_fix_flow_writes_marker_even_when_agent_invocation_fails() {
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let mut ctx = build_context(ContextParams {
        workspace: &workspace,
        repo_root: &repo_root,
        executor: &executor,
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        template_context: &template_context,
        timer: &mut timer,
        run_log_context: &run_log_context,
    });
    ctx.developer_agent = "missing-agent";

    let state = PipelineState::initial(1, 0);
    let mut handler = MainEffectHandler::new(state);

    let result = handler.execute(
        Effect::TriggerDevFixFlow {
            failed_phase: PipelinePhase::Development,
            failed_role: AgentRole::Developer,
            retry_cycle: 1,
        },
        &mut ctx,
    );

    assert!(
        result.is_ok(),
        "TriggerDevFixFlow should succeed even if dev-fix invocation fails"
    );

    let result = result.expect("Expected effect result");
    // NEW BEHAVIOR: CompletionMarkerEmitted should NOT be emitted immediately.
    // Instead, the pipeline should prepare for recovery after dev-fix completes.
    // The reducer will decide whether to retry, escalate, or terminate based on
    // the attempt count and recovery escalation level.
    assert!(
        !result.additional_events.iter().any(|event| matches!(
            event,
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerEmitted {
                is_failure: true
            })
        )),
        "CompletionMarkerEmitted should NOT be emitted immediately on dev-fix invocation failure. \
         Recovery should be attempted first."
    );

    let marker_path = Path::new(".agent/tmp/completion_marker");
    assert!(
        workspace.exists(marker_path),
        "Completion marker should still be written by TriggerDevFixFlow for external orchestration, \
         but the pipeline should not emit CompletionMarkerEmitted event (recovery comes first)"
    );
}
