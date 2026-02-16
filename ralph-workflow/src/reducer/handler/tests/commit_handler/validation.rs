use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{CommitEvent, PipelineEvent};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::reducer::ui_event::{UIEvent, XmlOutputType};
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn validate_commit_xml_emits_ui_xml_output_even_when_xml_file_missing() {
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
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 0));

    let result = handler
        .validate_commit_xml(&mut ctx)
        .expect("validate_commit_xml should not error for missing file");

    assert!(
        matches!(
            result.event,
            PipelineEvent::Commit(CommitEvent::CommitXmlValidationFailed { attempt: 1, .. })
        ),
        "expected CommitXmlValidationFailed event when xml is missing, got: {:?}",
        result.event
    );

    assert!(
        result.ui_events.iter().any(|e| matches!(
            e,
            UIEvent::XmlOutput {
                xml_type: XmlOutputType::CommitMessage,
                ..
            }
        )),
        "expected UIEvent::XmlOutput(CommitMessage) even when xml missing"
    );
}
