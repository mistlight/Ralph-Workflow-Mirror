use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{ErrorEvent, WorkspaceIoErrorKind};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{CommitState, PipelineState};
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_apply_commit_message_outcome_surfaces_missing_validated_outcome_as_error_event() {
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
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
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
        cloud_reporter: None,
        cloud_config: &cloud_config,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));
    handler.state.commit = CommitState::Generating {
        attempt: 2,
        max_attempts: 3,
    };

    let err = handler
        .apply_commit_message_outcome(&mut ctx)
        .expect_err("apply_commit_message_outcome must surface invariant violations as ErrorEvent");

    let error_event = err
        .downcast_ref::<ErrorEvent>()
        .expect("error should preserve ErrorEvent for event-loop recovery");
    assert!(
        matches!(
            error_event,
            ErrorEvent::ValidatedCommitOutcomeMissing { attempt: 2 }
        ),
        "expected ValidatedCommitOutcomeMissing, got: {error_event:?}"
    );

    // Defensive: ensure we did not produce a stringy 'Other' workspace error.
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
