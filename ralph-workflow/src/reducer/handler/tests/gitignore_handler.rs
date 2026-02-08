use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::PlanningEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn create_test_context<'a>(
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
) -> crate::phases::PhaseContext<'a> {
    crate::phases::PhaseContext {
        config,
        registry,
        logger,
        colors,
        timer,
        developer_agent: "dev",
        reviewer_agent: "rev",
        review_guidelines: None,
        template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: HashMap::new(),
        executor,
        executor_arc,
        repo_root,
        workspace,
        run_log_context,
    }
}

#[test]
fn test_ensure_gitignore_creates_file_when_missing() {
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
        &workspace,
        &config,
        &registry,
        &logger,
        &colors,
        &mut timer,
        &template_context,
        executor.as_ref(),
        executor.clone(),
        repo_root.as_path(),
        &run_log_context,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed");

    // Verify event
    match result.event {
        crate::reducer::event::PipelineEvent::Planning(
            PlanningEvent::GitignoreEntriesEnsured {
                entries_added,
                already_present,
                file_created,
            },
        ) => {
            assert_eq!(entries_added.len(), 2);
            assert!(entries_added.contains(&"/PROMPT*".to_string()));
            assert!(entries_added.contains(&".agent/".to_string()));
            assert!(already_present.is_empty());
            assert!(file_created);
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
        &workspace,
        &config,
        &registry,
        &logger,
        &colors,
        &mut timer,
        &template_context,
        executor.as_ref(),
        executor.clone(),
        repo_root.as_path(),
        &run_log_context,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed");

    // Verify event
    match result.event {
        crate::reducer::event::PipelineEvent::Planning(
            PlanningEvent::GitignoreEntriesEnsured {
                entries_added,
                already_present,
                file_created,
            },
        ) => {
            assert_eq!(entries_added.len(), 2);
            assert!(already_present.is_empty());
            assert!(!file_created);
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
        &workspace,
        &config,
        &registry,
        &logger,
        &colors,
        &mut timer,
        &template_context,
        executor.as_ref(),
        executor.clone(),
        repo_root.as_path(),
        &run_log_context,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed");

    // Verify event
    match result.event {
        crate::reducer::event::PipelineEvent::Planning(
            PlanningEvent::GitignoreEntriesEnsured {
                entries_added,
                already_present,
                file_created,
            },
        ) => {
            assert!(entries_added.is_empty());
            assert_eq!(already_present.len(), 2);
            assert!(!file_created);
        }
        _ => panic!("Expected GitignoreEntriesEnsured event"),
    }

    // Verify content unchanged
    let content = workspace.read(std::path::Path::new(".gitignore")).unwrap();
    assert_eq!(content, existing);
}

#[test]
fn test_ensure_gitignore_partial_entries() {
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
        &workspace,
        &config,
        &registry,
        &logger,
        &colors,
        &mut timer,
        &template_context,
        executor.as_ref(),
        executor.clone(),
        repo_root.as_path(),
        &run_log_context,
    );

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 0));
    let result = handler
        .ensure_gitignore_entries(&mut ctx)
        .expect("handler should succeed");

    // Verify event
    match result.event {
        crate::reducer::event::PipelineEvent::Planning(
            PlanningEvent::GitignoreEntriesEnsured {
                entries_added,
                already_present,
                file_created,
            },
        ) => {
            assert_eq!(entries_added.len(), 1);
            assert!(entries_added.contains(&".agent/".to_string()));
            assert_eq!(already_present.len(), 1);
            assert!(already_present.contains(&"/PROMPT*".to_string()));
            assert!(!file_created);
        }
        _ => panic!("Expected GitignoreEntriesEnsured event"),
    }
}
