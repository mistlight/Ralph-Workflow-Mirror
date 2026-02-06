use super::AtomicWriteEnforcingWorkspace;
use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{ErrorEvent, PipelineEvent, WorkspaceIoErrorKind};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{ContinuationState, PipelineState, PromptMode, SameAgentRetryReason};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
struct ReadFailingWorkspace {
    inner: MemoryWorkspace,
    forbidden_read_path: PathBuf,
    kind: io::ErrorKind,
}

impl ReadFailingWorkspace {
    fn new(inner: MemoryWorkspace, forbidden_read_path: PathBuf, kind: io::ErrorKind) -> Self {
        Self {
            inner,
            forbidden_read_path,
            kind,
        }
    }
}

impl Workspace for ReadFailingWorkspace {
    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn read(&self, relative: &Path) -> io::Result<String> {
        if relative == self.forbidden_read_path.as_path() {
            return Err(io::Error::new(
                self.kind,
                format!("read forbidden for {}", self.forbidden_read_path.display()),
            ));
        }
        self.inner.read(relative)
    }

    fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
        if relative == self.forbidden_read_path.as_path() {
            return Err(io::Error::new(
                self.kind,
                format!("read forbidden for {}", self.forbidden_read_path.display()),
            ));
        }
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

    fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.inner.create_dir_all(relative)
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

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }

    fn read_dir(&self, relative: &Path) -> io::Result<Vec<crate::workspace::DirEntry>> {
        self.inner.read_dir(relative)
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
fn test_prepare_fix_prompt_same_agent_retry_uses_previous_prepared_prompt() {
    let marker = "<<<PREVIOUS_FIX_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "<issues/>\n")
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/fix_prompt.txt", marker);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

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
    };

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Other),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });
    let _ = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_fix_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/fix_prompt.txt"))
        .expect("fix prompt file should be written");

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
fn test_prepare_fix_prompt_same_agent_retry_does_not_stack_retry_notes() {
    let marker = "<<<PREVIOUS_FIX_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "<issues/>\n")
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/fix_prompt.txt", marker);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

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
    };

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Other),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let _ = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_fix_prompt should succeed");

    handler.state.continuation.same_agent_retry_count = 2;
    let _ = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_fix_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/fix_prompt.txt"))
        .expect("fix prompt file should be written");

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

#[test]
fn test_prepare_fix_prompt_allows_literal_placeholders_in_issues() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "{{MISSING}}\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_fix_prompt should succeed");

    assert!(matches!(result.event, PipelineEvent::Review(_)));
}

#[test]
fn test_prepare_fix_prompt_workspace_write_failure_is_non_fatal() {
    // Per acceptance criteria #5: Template rendering errors must never terminate the pipeline.
    // When prompt file write fails, the handler logs a warning and continues successfully.
    let inner = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "<issues/>\n")
        .with_dir(".agent/tmp")
        .with_file(
            ".agent/tmp/fix_prompt.txt",
            "<<<PREVIOUS_FIX_PROMPT_MARKER>>>",
        );
    let workspace =
        AtomicWriteEnforcingWorkspace::new(inner, PathBuf::from(".agent/tmp/fix_prompt.txt"));

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

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
    };

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Other),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    // Per AC #5: Write failure should NOT return an error; it should succeed
    // with a warning logged instead.
    let result = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_fix_prompt should succeed even when write fails (non-fatal)");

    // Verify that the prompt was prepared in memory even though the write failed
    assert!(
        matches!(result.event, PipelineEvent::Review(_)),
        "should emit Review event even when write fails, got: {:?}",
        result.event
    );
}

#[test]
fn test_prepare_fix_prompt_does_not_mask_non_not_found_prompt_backup_read_errors() {
    let inner = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "<issues/>\n")
        .with_dir(".agent/tmp");
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/PROMPT.md.backup"),
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let err = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect_err("prepare_fix_prompt should surface non-NotFound PROMPT backup read failures");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/PROMPT.md.backup"
        ),
        "expected WorkspaceReadFailed for PROMPT backup read, got: {error_event:?}"
    );
}

#[test]
fn test_prepare_fix_prompt_does_not_mask_non_not_found_plan_read_errors() {
    let inner = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/ISSUES.md", "<issues/>\n")
        .with_dir(".agent/tmp");
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/PLAN.md"),
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let err = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect_err("prepare_fix_prompt should surface non-NotFound PLAN read failures");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/PLAN.md"
        ),
        "expected WorkspaceReadFailed for PLAN read, got: {error_event:?}"
    );
}

#[test]
fn test_prepare_fix_prompt_does_not_mask_non_not_found_issues_read_errors() {
    let inner = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_dir(".agent/tmp");
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(".agent/ISSUES.md"),
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let err = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect_err("prepare_fix_prompt should surface non-NotFound ISSUES read failures");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == ".agent/ISSUES.md"
        ),
        "expected WorkspaceReadFailed for ISSUES read, got: {error_event:?}"
    );
}

#[test]
fn test_prepare_fix_prompt_xsd_retry_does_not_mask_non_not_found_last_output_read_errors() {
    use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;

    let inner = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "# Issues\n")
        .with_dir(".agent/tmp");
    let workspace = ReadFailingWorkspace::new(
        inner,
        PathBuf::from(xml_paths::FIX_RESULT_XML),
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    handler.state.continuation = ContinuationState {
        invalid_output_attempts: 1,
        ..ContinuationState::default()
    };

    let err = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect_err("prepare_fix_prompt should surface non-NotFound FIX_RESULT_XML read failures");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceReadFailed {
                path,
                kind: WorkspaceIoErrorKind::PermissionDenied
            } if path == xml_paths::FIX_RESULT_XML
        ),
        "expected WorkspaceReadFailed for FIX_RESULT_XML read, got: {error_event:?}"
    );
}

#[test]
fn test_prepare_fix_prompt_embeds_sentinel_when_prompt_backup_missing() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "<issues/>\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let _ = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_fix_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/fix_prompt.txt"))
        .expect("fix prompt file should be written");
    assert!(
        prompt.contains("[MISSING INPUT: .agent/PROMPT.md.backup]"),
        "expected missing prompt backup sentinel in fix prompt; got: {prompt}"
    );
}

#[test]
fn test_prepare_fix_prompt_embeds_sentinel_when_issues_missing() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let _ = handler
        .prepare_fix_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_fix_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/fix_prompt.txt"))
        .expect("fix prompt file should be written");
    assert!(
        prompt.contains("[MISSING INPUT: .agent/ISSUES.md]"),
        "expected missing issues sentinel in fix prompt; got: {prompt}"
    );
}
