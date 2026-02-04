use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{ErrorEvent, PipelineEvent, WorkspaceIoErrorKind};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{
    AgentChainState, ContinuationState, PipelineState, PromptMode, SameAgentRetryReason,
};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
struct WriteFailingWorkspace {
    inner: MemoryWorkspace,
    forbidden_write_path: PathBuf,
}

impl WriteFailingWorkspace {
    fn new(inner: MemoryWorkspace, forbidden_write_path: PathBuf) -> Self {
        Self {
            inner,
            forbidden_write_path,
        }
    }
}

impl Workspace for WriteFailingWorkspace {
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
                    "write forbidden for {}",
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
fn test_prepare_planning_prompt_same_agent_retry_uses_previous_prepared_prompt() {
    let marker = "<<<PREVIOUS_PLANNING_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt")
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/planning_prompt.txt", marker);

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

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Timeout),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(1, 0)
    });

    handler
        .prepare_planning_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_planning_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/planning_prompt.txt"))
        .expect("planning prompt should be written");

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
fn test_prepare_planning_prompt_maps_workspace_write_failure_to_error_event() {
    let inner = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt")
        .with_dir(".agent/tmp")
        .with_file(
            ".agent/tmp/planning_prompt.txt",
            "<<<PREVIOUS_PLANNING_PROMPT_MARKER>>>",
        );
    let workspace =
        WriteFailingWorkspace::new(inner, PathBuf::from(".agent/tmp/planning_prompt.txt"));

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

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Timeout),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(1, 0)
    });

    let err = handler
        .prepare_planning_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect_err("prepare_planning_prompt should return a typed error event on write failure");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::WorkspaceWriteFailed { path, kind: WorkspaceIoErrorKind::Other }
                if path == ".agent/tmp/planning_prompt.txt"
        ),
        "expected WorkspaceWriteFailed for planning prompt write, got: {error_event:?}"
    );
}

#[test]
fn test_prepare_planning_prompt_same_agent_retry_does_not_stack_retry_notes() {
    let marker = "<<<PREVIOUS_PLANNING_PROMPT_MARKER>>>";
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "Prompt")
        .with_dir(".agent/tmp")
        .with_file(".agent/tmp/planning_prompt.txt", marker);

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

    let mut handler = MainEffectHandler::new(PipelineState {
        continuation: ContinuationState {
            same_agent_retry_count: 1,
            same_agent_retry_reason: Some(SameAgentRetryReason::Timeout),
            ..ContinuationState::new()
        },
        ..PipelineState::initial(1, 0)
    });

    handler
        .prepare_planning_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_planning_prompt should succeed");

    handler.state.continuation.same_agent_retry_count = 2;
    handler
        .prepare_planning_prompt(&mut ctx, 0, PromptMode::SameAgentRetry)
        .expect("prepare_planning_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/planning_prompt.txt"))
        .expect("planning prompt should be written");

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
fn test_prepare_planning_prompt_uses_references_for_oversize_prompt() {
    let large_prompt = "x".repeat(crate::prompts::MAX_INLINE_CONTENT_SIZE + 10);
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", &large_prompt)
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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["dev-agent".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );

    let materialize = handler
        .materialize_planning_inputs(&mut ctx, 0)
        .expect("materialize_planning_inputs should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), materialize.event);
    for ev in materialize.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }

    handler
        .prepare_planning_prompt(&mut ctx, 0, PromptMode::Normal)
        .expect("prepare_planning_prompt should succeed");

    let prompt = workspace
        .read(Path::new(".agent/tmp/planning_prompt.txt"))
        .expect("planning prompt file should be written");

    assert!(
        prompt.contains("PROMPT.md.backup"),
        "planning prompt should reference PROMPT.md.backup when prompt is oversize"
    );
    assert!(
        !prompt.contains(&large_prompt[..100]),
        "planning prompt should not inline the large prompt content"
    );
}

#[test]
fn test_materialize_planning_inputs_errors_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test().with_dir(".agent/tmp");

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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["dev-agent".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );

    let result = handler.materialize_planning_inputs(&mut ctx, 0);
    assert!(
        result.is_err(),
        "Expected Err when PROMPT.md is missing, got {result:?}",
    );
}

#[test]
fn test_prepare_planning_prompt_errors_when_prompt_missing() {
    let workspace = MemoryWorkspace::new_test().with_dir(".agent/tmp");

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

    // Seed reducer state with materialized planning inputs so prepare_planning_prompt can run.
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["dev-agent".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );
    handler.state.prompt_inputs.planning =
        Some(crate::reducer::state::MaterializedPlanningInputs {
            iteration: 0,
            prompt: crate::reducer::state::MaterializedPromptInput {
                kind: crate::reducer::state::PromptInputKind::Prompt,
                content_id_sha256: "id".to_string(),
                consumer_signature_sha256: handler.state.agent_chain.consumer_signature_sha256(),
                original_bytes: 0,
                final_bytes: 0,
                model_budget_bytes: None,
                inline_budget_bytes: Some(crate::prompts::MAX_INLINE_CONTENT_SIZE as u64),
                representation: crate::reducer::state::PromptInputRepresentation::Inline,
                reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
            },
        });

    let result = handler.prepare_planning_prompt(&mut ctx, 0, PromptMode::Normal);
    assert!(
        result.is_err(),
        "Expected Err when PROMPT.md is missing, got {result:?}",
    );
}

#[test]
fn test_prepare_planning_prompt_errors_when_inputs_not_materialized() {
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "# Prompt\n")
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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    let result = handler.prepare_planning_prompt(&mut ctx, 0, PromptMode::Normal);
    assert!(
        result.is_err(),
        "Expected Err when planning inputs are missing, got {result:?}",
    );
}

#[test]
fn test_prepare_planning_prompt_xsd_retry_emits_oversize_detected_for_last_output() {
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::state::PromptInputKind;

    let large_last_output = "x".repeat(crate::prompts::MAX_INLINE_CONTENT_SIZE + 10);
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/plan.xml", &large_last_output)
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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["dev-agent".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );

    let result = handler
        .prepare_planning_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_planning_prompt should succeed");

    assert!(
        result.additional_events.iter().any(|ev| matches!(
            ev,
            PipelineEvent::PromptInput(PromptInputEvent::OversizeDetected {
                kind: PromptInputKind::LastOutput,
                ..
            })
        )),
        "Expected OversizeDetected event for PromptInputKind::LastOutput during planning XSD retry"
    );
}

#[test]
fn test_planning_xsd_retry_oversize_detected_is_deduped_across_retries() {
    use crate::reducer::event::PromptInputEvent;
    use crate::reducer::state::PromptInputKind;

    let large_last_output = "x".repeat(crate::prompts::MAX_INLINE_CONTENT_SIZE + 10);
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/plan.xml", &large_last_output)
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

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["dev-agent".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Developer,
    );

    let first = handler
        .prepare_planning_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_planning_prompt should succeed");
    handler.state = crate::reducer::reduce(handler.state.clone(), first.event);
    for ev in first.additional_events {
        handler.state = crate::reducer::reduce(handler.state.clone(), ev);
    }

    let second = handler
        .prepare_planning_prompt(&mut ctx, 0, PromptMode::XsdRetry)
        .expect("prepare_planning_prompt should succeed");

    assert!(
        !second.additional_events.iter().any(|ev| matches!(
            ev,
            PipelineEvent::PromptInput(PromptInputEvent::OversizeDetected { kind: PromptInputKind::LastOutput, .. })
        )),
        "Expected OversizeDetected for LastOutput to be emitted only once for identical planning XSD retry context"
    );
}
