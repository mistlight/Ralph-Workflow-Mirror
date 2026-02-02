use super::*;
use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::{Stats, Timer};
use crate::prompts::template_context::TemplateContext;
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct TestFixture {
    config: Config,
    registry: AgentRegistry,
    colors: Colors,
    logger: Logger,
    timer: Timer,
    stats: Stats,
    template_context: TemplateContext,
    executor_arc: Arc<dyn crate::executor::ProcessExecutor>,
    repo_root: PathBuf,
    workspace: MemoryWorkspace,
}

impl TestFixture {
    fn new(workspace: MemoryWorkspace) -> Self {
        let colors = Colors { enabled: false };
        let executor_arc =
            Arc::new(MockProcessExecutor::new()) as Arc<dyn crate::executor::ProcessExecutor>;
        let repo_root = PathBuf::from("/test/repo");
        Self {
            config: Config::default(),
            registry: AgentRegistry::new().unwrap(),
            colors,
            logger: Logger::new(colors),
            timer: Timer::new(),
            stats: Stats::default(),
            template_context: TemplateContext::default(),
            executor_arc,
            repo_root,
            workspace,
        }
    }

    fn ctx(&mut self) -> super::super::context::PhaseContext<'_> {
        super::super::context::PhaseContext {
            config: &self.config,
            registry: &self.registry,
            logger: &self.logger,
            colors: &self.colors,
            timer: &mut self.timer,
            stats: &mut self.stats,
            developer_agent: "dev",
            reviewer_agent: "review",
            review_guidelines: None,
            template_context: &self.template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: self.executor_arc.as_ref(),
            executor_arc: self.executor_arc.clone(),
            repo_root: self.repo_root.as_path(),
            workspace: &self.workspace,
        }
    }
}

#[test]
fn test_validate_and_process_issues_xml_archives_and_writes_markdown() {
    let xml_content = r#"<ralph-issues>
 <ralph-no-issues-found>No issues were found during review</ralph-no-issues-found>
 </ralph-issues>"#;

    let workspace = MemoryWorkspace::new_test().with_file(
        crate::files::llm_output_extraction::xml_paths::ISSUES_XML,
        xml_content,
    );
    let mut fixture = TestFixture::new(workspace);
    let mut ctx = fixture.ctx();

    let _ = super::xml_processing::validate_and_process_issues_xml(
        &mut ctx,
        xml_content,
        Path::new(".agent/ISSUES.md"),
    )
    .expect("validate_and_process_issues_xml should succeed for valid XML");

    assert!(
        !fixture.workspace.exists(Path::new(
            crate::files::llm_output_extraction::xml_paths::ISSUES_XML
        )),
        "expected {} to be archived after validation",
        crate::files::llm_output_extraction::xml_paths::ISSUES_XML
    );
    assert!(
        fixture
            .workspace
            .exists(Path::new(".agent/tmp/issues.xml.processed")),
        "expected archived issues XML to exist"
    );

    let issues_md = fixture
        .workspace
        .read(Path::new(".agent/ISSUES.md"))
        .expect("ISSUES.md should be written");
    assert!(
        issues_md.contains("No issues"),
        "expected ISSUES.md to contain the no-issues summary"
    );
    assert!(
        !issues_md.contains("<ralph-issues>"),
        "expected ISSUES.md to be markdown, not raw XML"
    );
}

#[test]
fn test_run_review_pass_uses_unique_logfile_with_attempt_suffix() {
    let workspace = MemoryWorkspace::new_test().with_file(".agent/PLAN.md", "# Plan\n");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();

    let repo_root = PathBuf::from("/mock/repo");
    let mut ctx = super::super::context::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: "claude",
        reviewer_agent: "claude",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: HashMap::new(),
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let _ =
        run_review_pass(&mut ctx, 1, "review", "", None).expect("run_review_pass should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].logfile, ".agent/logs/reviewer_review_1_claude_0_a0.log",
        "review logfile should include agent, model index, and attempt suffix"
    );
}

#[test]
fn test_run_fix_pass_uses_unique_logfile_with_attempt_suffix() {
    let workspace = MemoryWorkspace::new_test()
        .with_file("PROMPT.md", "# Prompt\n")
        .with_file(".agent/PROMPT.md.backup", "# Prompt\n")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/ISSUES.md", "# Issues\n");
    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let config = Config::default();
    let registry = AgentRegistry::new().unwrap();
    let template_context = TemplateContext::default();
    let executor = Arc::new(
        MockProcessExecutor::new()
            .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
    );
    let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();

    let repo_root = PathBuf::from("/mock/repo");
    let mut ctx = super::super::context::PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: "claude",
        reviewer_agent: "claude",
        review_guidelines: None,
        template_context: &template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: HashMap::new(),
        executor: executor_arc.as_ref(),
        executor_arc: executor_arc.clone(),
        repo_root: repo_root.as_path(),
        workspace: &workspace,
    };

    let resume_ctx: Option<&crate::checkpoint::restore::ResumeContext> = None;
    let _ = run_fix_pass(
        &mut ctx,
        1,
        crate::prompts::ContextLevel::Normal,
        resume_ctx,
        None,
    )
    .expect("run_fix_pass should succeed");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].logfile, ".agent/logs/reviewer_fix_1_claude_0_a0.log",
        "fix logfile should include agent, model index, and attempt suffix"
    );
}
