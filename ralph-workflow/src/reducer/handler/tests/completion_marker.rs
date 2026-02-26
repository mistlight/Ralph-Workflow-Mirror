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
use crate::reducer::state::AgentChainState;
use crate::reducer::state::PipelineState;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
struct StrictTmpWorkspace {
    inner: MemoryWorkspace,
    tmp_created: Arc<AtomicBool>,
}

impl StrictTmpWorkspace {
    fn new(inner: MemoryWorkspace) -> Self {
        Self {
            inner,
            tmp_created: Arc::new(AtomicBool::new(false)),
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

#[derive(Debug, Clone)]
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
        let _cloud = crate::config::types::CloudConfig::disabled();
        self.inner.set_writable(relative)
    }
}

/// Parameters for building a test PhaseContext.
/// Groups related parameters to avoid clippy::too_many_arguments.
struct ContextParams<'a> {
    workspace: &'a dyn Workspace,
    workspace_arc: &'a Arc<dyn Workspace>,
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

fn build_context<'a>(
    params: ContextParams<'a>,
    cloud: &'a crate::config::types::CloudConfig,
) -> PhaseContext<'a> {
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
        workspace_arc: Arc::clone(params.workspace_arc),
        run_log_context: params.run_log_context,
        cloud_reporter: None,
        cloud,
    }
}

#[test]
fn emit_completion_marker_creates_tmp_dir_before_write() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = StrictTmpWorkspace::new(MemoryWorkspace::new(repo_root.clone()));
    let workspace_arc = std::sync::Arc::new(workspace.clone()) as Arc<dyn Workspace>;
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let mut ctx = build_context(
        ContextParams {
            workspace: &workspace,
            workspace_arc: &workspace_arc,
            repo_root: &repo_root,
            executor: &executor,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            template_context: &template_context,
            timer: &mut timer,
            run_log_context: &run_log_context,
        },
        &cloud,
    );

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
    let cloud = crate::config::types::CloudConfig::disabled();
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = FailingMarkerWorkspace::new(MemoryWorkspace::new(repo_root.clone()));
    let workspace_arc = std::sync::Arc::new(workspace.clone()) as Arc<dyn Workspace>;
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let mut ctx = build_context(
        ContextParams {
            workspace: &workspace,
            workspace_arc: &workspace_arc,
            repo_root: &repo_root,
            executor: &executor,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            template_context: &template_context,
            timer: &mut timer,
            run_log_context: &run_log_context,
        },
        &cloud,
    );

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
        PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::CompletionMarkerWriteFailed {
            is_failure: true,
            ..
        })
    ));
}

#[test]
fn trigger_dev_fix_flow_writes_marker_even_when_agent_invocation_fails() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());
    let workspace_arc = std::sync::Arc::new(workspace.clone()) as Arc<dyn Workspace>;
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let mut ctx = build_context(
        ContextParams {
            workspace: &workspace,
            workspace_arc: &workspace_arc,
            repo_root: &repo_root,
            executor: &executor,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            template_context: &template_context,
            timer: &mut timer,
            run_log_context: &run_log_context,
        },
        &cloud,
    );
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

    assert!(
        result.additional_events.iter().any(|event| matches!(
            event,
            PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixCompleted { .. })
        )),
        "DevFixCompleted should be emitted so the recovery loop can advance"
    );

    let marker_path = Path::new(".agent/tmp/completion_marker");
    assert!(
        !workspace.exists(marker_path),
        "Completion marker must not be written by TriggerDevFixFlow; it is written only on termination"
    );
}

#[test]
fn trigger_dev_fix_flow_invokes_configured_developer_agent_not_current_chain_agent() {
    let cloud = crate::config::types::CloudConfig::disabled();
    let config = Config::default();
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );
    let repo_root = PathBuf::from("/test/repo");
    let workspace = MemoryWorkspace::new(repo_root.clone());
    let workspace_arc = std::sync::Arc::new(workspace.clone()) as Arc<dyn Workspace>;
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
    let mut timer = Timer::new();

    let mut ctx = build_context(
        ContextParams {
            workspace: &workspace,
            workspace_arc: &workspace_arc,
            repo_root: &repo_root,
            executor: &executor,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            template_context: &template_context,
            timer: &mut timer,
            run_log_context: &run_log_context,
        },
        &cloud,
    );
    ctx.developer_agent = "claude";

    let mut state = PipelineState::initial(1, 0);
    state.phase = PipelinePhase::AwaitingDevFix;

    // Simulate failure outside Development: chain currently points at a different role/agent.
    state.agent_chain = AgentChainState::initial().with_agents(
        vec!["codex".to_string()],
        vec![vec![]],
        AgentRole::Commit,
    );

    let mut handler = MainEffectHandler::new(state);
    handler
        .execute(
            Effect::TriggerDevFixFlow {
                failed_phase: PipelinePhase::CommitMessage,
                failed_role: AgentRole::Commit,
                retry_cycle: 1,
            },
            &mut ctx,
        )
        .expect("TriggerDevFixFlow should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].command, "claude");
}

#[test]
fn dev_fix_agent_unavailable_log_does_not_claim_termination() {
    let config = Config::default();
    let colors = Colors { enabled: false };
    let template_context = TemplateContext::default();
    let registry = AgentRegistry::new().unwrap();
    let repo_root = PathBuf::from("/test/repo");

    #[derive(Debug)]
    struct FailingAgentLogWorkspace {
        inner: MemoryWorkspace,
    }

    impl Workspace for FailingAgentLogWorkspace {
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
            self.inner.write(relative, content)
        }

        fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
            self.inner.write_bytes(relative, content)
        }

        fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
            if relative.to_string_lossy().starts_with(".agent/logs-") {
                return Err(io::Error::other("usage limit exceeded"));
            }
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

    let workspace = std::sync::Arc::new(FailingAgentLogWorkspace {
        inner: MemoryWorkspace::new(repo_root.clone()),
    });
    let run_log_context = crate::logging::RunLogContext::new(workspace.as_ref()).unwrap();
    let mut timer = Timer::new();

    let logger = Logger::new(colors).with_workspace_log(
        std::sync::Arc::clone(&workspace) as std::sync::Arc<dyn Workspace>,
        ".agent/tmp/test_logger.log",
    );

    let executor = std::sync::Arc::new(MockProcessExecutor::new());
    let executor_arc: std::sync::Arc<dyn crate::executor::ProcessExecutor> = executor.clone();

    let cloud = crate::config::types::CloudConfig::disabled();
    let workspace_arc = std::sync::Arc::clone(&workspace) as std::sync::Arc<dyn Workspace>;
    let mut ctx = PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        developer_agent: "claude",
        reviewer_agent: "test-reviewer",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
        executor: executor_arc.as_ref(),
        executor_arc: std::sync::Arc::clone(&executor_arc),
        repo_root: repo_root.as_path(),
        workspace: workspace.as_ref(),
        workspace_arc,
        run_log_context: &run_log_context,
        cloud_reporter: None,
        cloud: &cloud,
    };

    let mut state = PipelineState::initial(1, 0);
    state.phase = PipelinePhase::AwaitingDevFix;
    state.agent_chain = crate::reducer::state::AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        AgentRole::Developer,
    );
    let mut handler = MainEffectHandler::new(state);

    let result = handler
        .execute(
            Effect::TriggerDevFixFlow {
                failed_phase: PipelinePhase::Development,
                failed_role: AgentRole::Developer,
                retry_cycle: 1,
            },
            &mut ctx,
        )
        .expect("TriggerDevFixFlow should handle agent unavailability");

    assert!(result.additional_events.iter().any(|e| matches!(
        e,
        PipelineEvent::AwaitingDevFix(AwaitingDevFixEvent::DevFixAgentUnavailable { .. })
    )));

    let log_contents = workspace
        .read(Path::new(".agent/tmp/test_logger.log"))
        .expect("expected logger to write to workspace log");
    assert!(
        log_contents.contains("Continuing unattended recovery loop without dev-fix agent"),
        "expected updated log message, got:\n{log_contents}"
    );
    assert!(
        !log_contents.contains("terminate with failure marker"),
        "log must not claim termination, got:\n{log_contents}"
    );
}
