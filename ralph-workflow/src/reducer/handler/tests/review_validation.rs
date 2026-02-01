use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::PipelineEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{PipelineState, ReviewValidatedOutcome};
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn test_validate_review_issues_xml_emits_ui_output() {
    let issues_xml =
        "<ralph-issues><ralph-no-issues-found>ok</ralph-no-issues-found></ralph-issues>";
    let workspace = MemoryWorkspace::new_test().with_file(xml_paths::ISSUES_XML, issues_xml);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .validate_review_issues_xml(&mut ctx, 0)
        .expect("validate_review_issues_xml should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::IssuesXmlValidated {
            pass: 0,
            ..
        })
    ));

    assert!(result.ui_events.iter().any(|event| matches!(
        event,
        UIEvent::XmlOutput {
            xml_type: XmlOutputType::ReviewIssues,
            content,
            context: Some(XmlOutputContext {
                pass: Some(0),
                ..
            }),
        } if content == issues_xml
    )));
}

#[test]
fn test_validate_fix_result_xml_emits_ui_output() {
    let fix_xml =
        "<ralph-fix-result><ralph-status>all_issues_addressed</ralph-status></ralph-fix-result>";
    let workspace = MemoryWorkspace::new_test().with_file(xml_paths::FIX_RESULT_XML, fix_xml);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .validate_fix_result_xml(&mut ctx, 0)
        .expect("validate_fix_result_xml should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::FixResultXmlValidated {
            pass: 0,
            ..
        })
    ));

    assert!(result.ui_events.iter().any(|event| matches!(
        event,
        UIEvent::XmlOutput {
            xml_type: XmlOutputType::FixResult,
            content,
            context: Some(XmlOutputContext {
                pass: Some(0),
                ..
            }),
        } if content == fix_xml
    )));
}

#[test]
fn test_write_issues_markdown_uses_validated_markdown() {
    let markdown = "# Issues\n\nNo issues.\n";
    let workspace = MemoryWorkspace::new_test();

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    handler.state.review_validated_outcome = Some(ReviewValidatedOutcome {
        pass: 0,
        issues_found: false,
        clean_no_issues: true,
        markdown: Some(markdown.to_string()),
    });

    let result = handler
        .write_issues_markdown(&mut ctx, 0)
        .expect("write_issues_markdown should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Review(crate::reducer::event::ReviewEvent::IssuesMarkdownWritten {
            pass: 0
        })
    ));

    let content = workspace
        .read(Path::new(".agent/ISSUES.md"))
        .expect("ISSUES.md should be written");
    assert_eq!(content, markdown);
}

#[test]
fn test_validate_review_issues_xml_includes_snippets_for_locations() {
    let issues_xml = "<ralph-issues><ralph-issue>[high] src/lib.rs:2 - adjust logic</ralph-issue></ralph-issues>";
    let workspace = MemoryWorkspace::new_test()
        .with_file(xml_paths::ISSUES_XML, issues_xml)
        .with_file("src/lib.rs", "fn main() {\n    let x = 1;\n}\n");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .validate_review_issues_xml(&mut ctx, 0)
        .expect("validate_review_issues_xml should succeed");

    let snippets = result.ui_events.iter().find_map(|event| {
        if let UIEvent::XmlOutput { context, .. } = event {
            context.as_ref().map(|ctx| ctx.snippets.clone())
        } else {
            None
        }
    });

    let snippets = snippets.expect("expected XmlOutput context with snippets");
    assert_eq!(snippets.len(), 1);
    assert_eq!(snippets[0].file, "src/lib.rs");
    assert_eq!(snippets[0].line_start, 2);
    assert_eq!(snippets[0].line_end, 2);
    assert!(snippets[0].content.contains("2 |"));
    assert!(snippets[0].content.contains("let x = 1;"));
}

#[test]
fn test_validate_review_issues_xml_includes_snippets_for_windows_paths() {
    let issues_xml =
        "<ralph-issues><ralph-issue>[high] C:\\repo\\src\\lib.rs:2 - adjust logic</ralph-issue></ralph-issues>";
    let workspace = MemoryWorkspace::new_test()
        .with_file("C:/repo/src/lib.rs", "fn main() {\n    let y = 2;\n}\n")
        .with_file(xml_paths::ISSUES_XML, issues_xml);

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .validate_review_issues_xml(&mut ctx, 0)
        .expect("validate_review_issues_xml should succeed");

    let snippets = result.ui_events.iter().find_map(|event| {
        if let UIEvent::XmlOutput { context, .. } = event {
            context.as_ref().map(|ctx| ctx.snippets.clone())
        } else {
            None
        }
    });

    let snippets = snippets.expect("expected XmlOutput context with snippets");
    assert_eq!(snippets.len(), 1);
    assert!(snippets[0].content.contains("2 |"));
    assert!(snippets[0].content.contains("let y = 2;"));
}

#[test]
fn test_write_issues_markdown_aborts_when_missing_validated_outcome() {
    let workspace = MemoryWorkspace::new_test();

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();

    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();

    let executor = Arc::new(MockProcessExecutor::new());
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();
    let executor_ref = executor_arc.clone();
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
        executor: executor_ref.as_ref(),
        executor_arc,
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let mut handler = MainEffectHandler::new(PipelineState::initial(0, 1));
    let result = handler
        .write_issues_markdown(&mut ctx, 0)
        .expect("write_issues_markdown should succeed");

    assert!(matches!(result.event, PipelineEvent::Lifecycle(_)));
}
