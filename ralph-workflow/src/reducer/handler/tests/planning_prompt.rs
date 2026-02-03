use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{AgentChainState, PipelineState, PromptMode};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
