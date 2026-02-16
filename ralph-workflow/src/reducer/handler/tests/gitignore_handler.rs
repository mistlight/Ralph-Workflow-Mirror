use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::LifecycleEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Helper struct to group test context parameters
struct TestContextParams<'a> {
    workspace: &'a dyn crate::workspace::Workspace,
    config: &'a Config,
    registry: &'a AgentRegistry,
    logger: &'a Logger,
    colors: &'a Colors,
    timer: &'a mut Timer,
    template_context: &'a TemplateContext,
    executor: &'a dyn crate::executor::ProcessExecutor,
    executor_arc: Arc<dyn crate::executor::ProcessExecutor>,
    repo_root: &'a std::path::Path,
    run_log_context: &'a crate::logging::RunLogContext,
}

fn create_test_context<'a>(
    params: TestContextParams<'a>,
    cloud_config: &'a crate::config::types::CloudConfig,
) -> crate::phases::PhaseContext<'a> {
    crate::phases::PhaseContext {
        config: params.config,
        registry: params.registry,
        logger: params.logger,
        colors: params.colors,
        timer: params.timer,
        developer_agent: "dev",
        reviewer_agent: "rev",
        review_guidelines: None,
        template_context: params.template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: HashMap::new(),
        executor: params.executor,
        executor_arc: params.executor_arc,
        repo_root: params.repo_root,
        workspace: params.workspace,
        run_log_context: params.run_log_context,
        cloud_reporter: None,
        cloud_config,
    }
}

#[test]
fn test_ensure_gitignore_creates_file_when_missing() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test();

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();

    let mut ctx = create_test_context(
        TestContextParams {
            workspace: &workspace,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            template_context: &template_context,
            executor: executor.as_ref(),
            executor_arc: executor.clone(),
            repo_root: repo_root.as_path(),
            run_log_context: &run_log_context,
        },
        &cloud_config,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed");

    // Verify event
    match result.event {
        crate::reducer::event::PipelineEvent::Lifecycle(
            LifecycleEvent::GitignoreEntriesEnsured {
                added,
                existing,
                created,
            },
        ) => {
            assert_eq!(added.len(), 2);
            assert!(added.contains(&"/PROMPT*".to_string()));
            assert!(added.contains(&".agent/".to_string()));
            assert!(existing.is_empty());
            assert!(created);
        }
        _ => panic!("Expected GitignoreEntriesEnsured event"),
    }

    // Verify file was written
    assert!(workspace.exists(std::path::Path::new(".gitignore")));
    let content = workspace.read(std::path::Path::new(".gitignore")).unwrap();
    assert!(content.contains("/PROMPT*"));
    assert!(content.contains(".agent/"));
    assert!(content.contains("# Ralph-workflow artifacts"));
}

#[test]
fn test_ensure_gitignore_appends_when_file_exists() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test().with_file(".gitignore", "node_modules/\n*.log\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();

    let mut ctx = create_test_context(
        TestContextParams {
            workspace: &workspace,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            template_context: &template_context,
            executor: executor.as_ref(),
            executor_arc: executor.clone(),
            repo_root: repo_root.as_path(),
            run_log_context: &run_log_context,
        },
        &cloud_config,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed");

    // Verify event
    match result.event {
        crate::reducer::event::PipelineEvent::Lifecycle(
            LifecycleEvent::GitignoreEntriesEnsured {
                added,
                existing,
                created,
            },
        ) => {
            assert_eq!(added.len(), 2);
            assert!(existing.is_empty());
            assert!(!created);
        }
        _ => panic!("Expected GitignoreEntriesEnsured event"),
    }

    // Verify existing content preserved
    let content = workspace.read(std::path::Path::new(".gitignore")).unwrap();
    assert!(content.contains("node_modules/"));
    assert!(content.contains("*.log"));
    assert!(content.contains("/PROMPT*"));
    assert!(content.contains(".agent/"));
}

#[test]
fn test_ensure_gitignore_idempotent_when_entries_exist() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let existing = "# Ralph-workflow artifacts (auto-generated)\n/PROMPT*\n.agent/\n";
    let workspace = MemoryWorkspace::new_test().with_file(".gitignore", existing);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();

    let mut ctx = create_test_context(
        TestContextParams {
            workspace: &workspace,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            template_context: &template_context,
            executor: executor.as_ref(),
            executor_arc: executor.clone(),
            repo_root: repo_root.as_path(),
            run_log_context: &run_log_context,
        },
        &cloud_config,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed");

    // Verify event
    match result.event {
        crate::reducer::event::PipelineEvent::Lifecycle(
            LifecycleEvent::GitignoreEntriesEnsured {
                added,
                existing,
                created,
            },
        ) => {
            assert!(added.is_empty());
            assert_eq!(existing.len(), 2);
            assert!(!created);
        }
        _ => panic!("Expected GitignoreEntriesEnsured event"),
    }

    // Verify content unchanged
    let content = workspace.read(std::path::Path::new(".gitignore")).unwrap();
    assert_eq!(content, existing);
}

#[test]
fn test_ensure_gitignore_partial_entries() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    let workspace = MemoryWorkspace::new_test().with_file(".gitignore", "/PROMPT*\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();

    let mut ctx = create_test_context(
        TestContextParams {
            workspace: &workspace,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            template_context: &template_context,
            executor: executor.as_ref(),
            executor_arc: executor.clone(),
            repo_root: repo_root.as_path(),
            run_log_context: &run_log_context,
        },
        &cloud_config,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed");

    // Verify event
    match result.event {
        crate::reducer::event::PipelineEvent::Lifecycle(
            LifecycleEvent::GitignoreEntriesEnsured {
                added,
                existing,
                created,
            },
        ) => {
            assert_eq!(added.len(), 1);
            assert!(added.contains(&".agent/".to_string()));
            assert_eq!(existing.len(), 1);
            assert!(existing.contains(&"/PROMPT*".to_string()));
            assert!(!created);
        }
        _ => panic!("Expected GitignoreEntriesEnsured event"),
    }
}

/// Workspace wrapper that simulates write failures for testing error handling.
struct FailingWriteWorkspace<'a> {
    inner: &'a dyn Workspace,
}

impl<'a> FailingWriteWorkspace<'a> {
    fn new(inner: &'a dyn Workspace) -> Self {
        Self { inner }
    }
}

impl<'a> Workspace for FailingWriteWorkspace<'a> {
    fn root(&self) -> &std::path::Path {
        self.inner.root()
    }

    fn read(&self, relative: &std::path::Path) -> std::io::Result<String> {
        self.inner.read(relative)
    }

    fn read_bytes(&self, relative: &std::path::Path) -> std::io::Result<Vec<u8>> {
        self.inner.read_bytes(relative)
    }

    fn write(&self, _relative: &std::path::Path, _content: &str) -> std::io::Result<()> {
        // Simulate write failure
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "simulated permission denied",
        ))
    }

    fn write_bytes(&self, _relative: &std::path::Path, _content: &[u8]) -> std::io::Result<()> {
        // Simulate write failure
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "simulated permission denied",
        ))
    }

    fn append_bytes(&self, relative: &std::path::Path, content: &[u8]) -> std::io::Result<()> {
        self.inner.append_bytes(relative, content)
    }

    fn exists(&self, relative: &std::path::Path) -> bool {
        self.inner.exists(relative)
    }

    fn is_file(&self, relative: &std::path::Path) -> bool {
        self.inner.is_file(relative)
    }

    fn is_dir(&self, relative: &std::path::Path) -> bool {
        self.inner.is_dir(relative)
    }

    fn remove(&self, relative: &std::path::Path) -> std::io::Result<()> {
        self.inner.remove(relative)
    }

    fn remove_if_exists(&self, relative: &std::path::Path) -> std::io::Result<()> {
        self.inner.remove_if_exists(relative)
    }

    fn remove_dir_all(&self, relative: &std::path::Path) -> std::io::Result<()> {
        self.inner.remove_dir_all(relative)
    }

    fn remove_dir_all_if_exists(&self, relative: &std::path::Path) -> std::io::Result<()> {
        self.inner.remove_dir_all_if_exists(relative)
    }

    fn create_dir_all(&self, relative: &std::path::Path) -> std::io::Result<()> {
        self.inner.create_dir_all(relative)
    }

    fn read_dir(
        &self,
        relative: &std::path::Path,
    ) -> std::io::Result<Vec<crate::workspace::DirEntry>> {
        self.inner.read_dir(relative)
    }

    fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        self.inner.rename(from, to)
    }

    fn set_readonly(&self, relative: &std::path::Path) -> std::io::Result<()> {
        self.inner.set_readonly(relative)
    }

    fn set_writable(&self, relative: &std::path::Path) -> std::io::Result<()> {
        self.inner.set_writable(relative)
    }

    fn write_atomic(&self, relative: &std::path::Path, content: &str) -> std::io::Result<()> {
        self.inner.write_atomic(relative, content)
    }
}

#[test]
fn test_ensure_gitignore_handles_write_failure_gracefully() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    // Setup: workspace with existing file that will fail to write
    let workspace = MemoryWorkspace::new_test().with_file(".gitignore", "node_modules/\n*.log\n");
    let failing_workspace = FailingWriteWorkspace::new(&workspace);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();

    let mut ctx = create_test_context(
        TestContextParams {
            workspace: &failing_workspace,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            template_context: &template_context,
            executor: executor.as_ref(),
            executor_arc: executor.clone(),
            repo_root: repo_root.as_path(),
            run_log_context: &run_log_context,
        },
        &cloud_config,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed even on write failure");

    // Verify event - should have empty entries_added because write failed
    match result.event {
        crate::reducer::event::PipelineEvent::Lifecycle(
            LifecycleEvent::GitignoreEntriesEnsured {
                added,
                existing,
                created,
            },
        ) => {
            // Write failed, so no entries were added
            assert!(
                added.is_empty(),
                "entries_added should be empty when write fails"
            );
            // Already present list should still be correct (checked before write)
            assert!(existing.is_empty());
            // created should be false (file existed before)
            assert!(!created);
        }
        _ => panic!("Expected GitignoreEntriesEnsured event"),
    }

    // Verify file was NOT modified (write failed)
    let content = workspace.read(std::path::Path::new(".gitignore")).unwrap();
    assert_eq!(content, "node_modules/\n*.log\n");
    assert!(!content.contains("/PROMPT*"));
    assert!(!content.contains(".agent/"));
}

#[test]
fn test_ensure_gitignore_handles_write_failure_on_missing_file() {
    let cloud_config = crate::config::types::CloudConfig::disabled();
    // Setup: no existing .gitignore, write will fail
    let workspace = MemoryWorkspace::new_test();
    let failing_workspace = FailingWriteWorkspace::new(&workspace);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();

    let mut ctx = create_test_context(
        TestContextParams {
            workspace: &failing_workspace,
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            template_context: &template_context,
            executor: executor.as_ref(),
            executor_arc: executor.clone(),
            repo_root: repo_root.as_path(),
            run_log_context: &run_log_context,
        },
        &cloud_config,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed even on write failure");

    // Verify event - should have empty entries_added because write failed
    match result.event {
        crate::reducer::event::PipelineEvent::Lifecycle(
            LifecycleEvent::GitignoreEntriesEnsured {
                added,
                existing,
                created,
            },
        ) => {
            // Write failed, so no entries were added
            assert!(
                added.is_empty(),
                "entries_added should be empty when write fails"
            );
            // Already present list should be empty (no file existed)
            assert!(existing.is_empty());
            // created should be true (file didn't exist before, even though write failed)
            assert!(created);
        }
        _ => panic!("Expected GitignoreEntriesEnsured event"),
    }

    // Verify file was NOT created (write failed)
    assert!(!workspace.exists(std::path::Path::new(".gitignore")));
}
