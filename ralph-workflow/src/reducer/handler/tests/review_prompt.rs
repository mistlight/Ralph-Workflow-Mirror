use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{AgentEvent, PipelineEvent};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{ContinuationState, PipelineState, PromptMode};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn test_materialize_review_inputs_aborts_when_plan_missing() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should return an EffectResult");

    assert!(
        matches!(
            result.event,
            PipelineEvent::Lifecycle(crate::reducer::event::LifecycleEvent::Aborted { .. })
        ),
        "Expected pipeline_aborted when .agent/PLAN.md is missing, got {:?}",
        result.event
    );
}

#[test]
fn test_materialize_review_inputs_aborts_when_diff_missing() {
    let workspace = MemoryWorkspace::new_test()
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
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should return an EffectResult");

    assert!(
        matches!(
            result.event,
            PipelineEvent::Lifecycle(crate::reducer::event::LifecycleEvent::Aborted { .. })
        ),
        "Expected pipeline_aborted when .agent/DIFF.backup is missing, got {:?}",
        result.event
    );
}

#[derive(Debug)]
struct AtomicWriteEnforcingWorkspace {
    inner: MemoryWorkspace,
    forbidden_write_path: std::path::PathBuf,
}

impl AtomicWriteEnforcingWorkspace {
    fn new(inner: MemoryWorkspace, forbidden_write_path: std::path::PathBuf) -> Self {
        Self {
            inner,
            forbidden_write_path,
        }
    }
}

impl Workspace for AtomicWriteEnforcingWorkspace {
    fn root(&self) -> &std::path::Path {
        self.inner.root()
    }

    fn read(&self, relative: &std::path::Path) -> io::Result<String> {
        self.inner.read(relative)
    }

    fn read_bytes(&self, relative: &std::path::Path) -> io::Result<Vec<u8>> {
        self.inner.read_bytes(relative)
    }

    fn write(&self, relative: &std::path::Path, content: &str) -> io::Result<()> {
        if relative == self.forbidden_write_path.as_path() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "non-atomic write forbidden for {}",
                    self.forbidden_write_path.display()
                ),
            ));
        }
        self.inner.write(relative, content)
    }

    fn write_bytes(&self, relative: &std::path::Path, content: &[u8]) -> io::Result<()> {
        self.inner.write_bytes(relative, content)
    }

    fn append_bytes(&self, relative: &std::path::Path, content: &[u8]) -> io::Result<()> {
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

    fn remove(&self, relative: &std::path::Path) -> io::Result<()> {
        self.inner.remove(relative)
    }

    fn remove_if_exists(&self, relative: &std::path::Path) -> io::Result<()> {
        self.inner.remove_if_exists(relative)
    }

    fn remove_dir_all(&self, relative: &std::path::Path) -> io::Result<()> {
        self.inner.remove_dir_all(relative)
    }

    fn remove_dir_all_if_exists(&self, relative: &std::path::Path) -> io::Result<()> {
        self.inner.remove_dir_all_if_exists(relative)
    }

    fn create_dir_all(&self, relative: &std::path::Path) -> io::Result<()> {
        self.inner.create_dir_all(relative)
    }

    fn read_dir(&self, relative: &std::path::Path) -> io::Result<Vec<crate::workspace::DirEntry>> {
        self.inner.read_dir(relative)
    }

    fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }

    fn write_atomic(&self, relative: &std::path::Path, content: &str) -> io::Result<()> {
        self.inner.write_atomic(relative, content)
    }

    fn set_readonly(&self, relative: &std::path::Path) -> io::Result<()> {
        self.inner.set_readonly(relative)
    }

    fn set_writable(&self, relative: &std::path::Path) -> io::Result<()> {
        self.inner.set_writable(relative)
    }
}

#[test]
fn test_materialize_review_inputs_writes_oversize_diff_with_atomic_write() {
    let large_diff = "d".repeat(crate::prompts::MAX_INLINE_CONTENT_SIZE + 1);
    let inner = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/DIFF.backup", &large_diff)
        .with_dir(".agent/tmp");
    let workspace =
        AtomicWriteEnforcingWorkspace::new(inner, std::path::PathBuf::from(".agent/tmp/diff.txt"));

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should return an EffectResult");

    assert!(
        matches!(
            result.event,
            PipelineEvent::PromptInput(
                crate::reducer::event::PromptInputEvent::ReviewInputsMaterialized { .. }
            )
        ),
        "Expected ReviewInputsMaterialized event, got {:?}",
        result.event
    );

    let written = workspace
        .read(std::path::Path::new(".agent/tmp/diff.txt"))
        .expect("materialized diff file should be written");
    assert_eq!(written, large_diff);
}

#[test]
fn test_prepare_review_prompt_aborts_when_inputs_not_materialized() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
        .prepare_review_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_review_prompt should return an EffectResult");

    assert!(
        matches!(
            result.event,
            PipelineEvent::Lifecycle(crate::reducer::event::LifecycleEvent::Aborted { .. })
        ),
        "Expected pipeline_aborted when review inputs are missing, got {:?}",
        result.event
    );
}

#[test]
fn test_prepare_review_prompt_writes_prompt_file_with_required_markers() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
    let materialize = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), materialize.event);
    for ev in materialize.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }
    let _ = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_review_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/review_prompt.txt"))
        .expect("review prompt file should be written");

    assert!(
        prompt.contains("PROMPT.md.backup"),
        "review prompt should instruct reading PROMPT.md.backup"
    );
    assert!(
        prompt.contains("<ralph-issues>"),
        "review prompt should include XML output instructions"
    );
}

#[test]
fn test_prepare_review_prompt_uses_diff_baseline_for_oversize_diff() {
    let large_diff = "d".repeat(crate::prompts::MAX_INLINE_CONTENT_SIZE + 1);
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", &large_diff)
        .with_file(".agent/DIFF.base", "abc123")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
    let materialize = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), materialize.event);
    for ev in materialize.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }
    let _ = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_review_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/review_prompt.txt"))
        .expect("review prompt file should be written");

    assert!(
        prompt.contains("git diff abc123"),
        "review prompt should include baseline git diff command"
    );
    assert!(
        prompt.contains("git diff --cached abc123"),
        "review prompt should include baseline cached diff command"
    );
}

#[test]
fn test_prepare_review_prompt_allows_literal_placeholders_in_plan() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "{{MISSING}}\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
    let materialize = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), materialize.event);
    for ev in materialize.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }
    let result = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_review_prompt should succeed");

    assert!(matches!(result.event, PipelineEvent::Review(_)));
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
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
fn test_prepare_review_prompt_uses_xsd_retry_prompt_key() {
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
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
}

#[test]
fn test_review_xsd_retry_oversize_detected_is_deduped_across_retries() {
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
    let mut stats = Stats::default();

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
        stats: &mut stats,
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
fn test_prepare_review_prompt_normal_mode_ignores_retry_state() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut prompt_history = HashMap::new();
    prompt_history.insert("review_0".to_string(), "{{UNRESOLVED}}".to_string());

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
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            invalid_output_attempts: 1,
            ..ContinuationState::new()
        },
        ..PipelineState::initial(0, 1)
    });

    let materialize = handler
        .materialize_review_inputs(&mut ctx, 0)
        .expect("materialize_review_inputs should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), materialize.event);
    for ev in materialize.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }

    let result = handler
        .prepare_review_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_review_prompt should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Agent(AgentEvent::TemplateVariablesInvalid { template_name, .. })
            if template_name == "review_xml"
    ));
}

#[test]
fn test_prepare_review_prompt_xsd_retry_ignores_last_output_placeholders() {
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
    let mut stats = Stats::default();

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
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
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
fn test_prepare_review_prompt_uses_xsd_retry_template_name() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/DIFF.backup", "diff --git a/a b/a\n+change\n")
        .with_file(".agent/tmp/issues.xml", "<ralph-issues>bad</ralph-issues>")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
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
fn test_prepare_fix_prompt_uses_xsd_retry_template_name() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "Issue\n")
        .with_dir(".agent/tmp");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

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
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
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

    assert!(matches!(
        result.event,
        PipelineEvent::Agent(AgentEvent::TemplateVariablesInvalid { template_name, .. })
            if template_name == "fix_mode_xsd_retry"
    ));
}

#[test]
fn test_prepare_fix_prompt_uses_prompt_history_replay() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PROMPT.md.backup", "# Prompt backup\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "Issue\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let repo_root = PathBuf::from("/mock/repo");

    let mut prompt_history = HashMap::new();
    prompt_history.insert("fix_0".to_string(), "REPLAYED PROMPT".to_string());

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
        prompt_history,
        executor: executor.as_ref(),
        executor_arc: executor.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
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
