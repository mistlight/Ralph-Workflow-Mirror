use super::common::TestFixture;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::AgentChainState;
use crate::reducer::state::PipelineState;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::path::Path;

#[test]
fn test_invoke_planning_agent_does_not_clear_stale_plan_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/planning_prompt.txt", "prompt")
        .with_file(xml_paths::PLAN_XML, "<ralph-plan>old</ralph-plan>");
    let mut fixture = TestFixture::with_workspace(workspace);
    let mut ctx = fixture.ctx();
    ctx.developer_agent = "claude";
    ctx.reviewer_agent = "codex";
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_planning_agent(&mut ctx, 0)
        .expect("invoke_planning_agent should succeed");

    drop(ctx);
    assert!(fixture.workspace.exists(Path::new(xml_paths::PLAN_XML)));
}

#[test]
fn test_cleanup_planning_xml_clears_stale_plan_xml() {
    let workspace =
        MemoryWorkspace::new_test().with_file(xml_paths::PLAN_XML, "<ralph-plan>old</ralph-plan>");
    let mut fixture = TestFixture::with_workspace(workspace);
    let ctx = fixture.ctx();

    MainEffectHandler::cleanup_planning_xml(&ctx, 0);

    drop(ctx);
    assert!(!fixture.workspace.exists(Path::new(xml_paths::PLAN_XML)));
}

#[test]
fn test_invoke_development_agent_does_not_clear_stale_dev_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/development_prompt.txt", "prompt")
        .with_file(
            xml_paths::DEVELOPMENT_RESULT_XML,
            "<ralph-development>old</ralph-development>",
        );
    let mut fixture = TestFixture::with_workspace(workspace);
    let mut ctx = fixture.ctx();
    ctx.developer_agent = "claude";
    ctx.reviewer_agent = "codex";
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_development_agent(&mut ctx, 0)
        .expect("invoke_development_agent should succeed");

    drop(ctx);
    assert!(fixture
        .workspace
        .exists(Path::new(xml_paths::DEVELOPMENT_RESULT_XML)));
}

#[test]
fn test_cleanup_development_xml_clears_stale_dev_xml() {
    let workspace = MemoryWorkspace::new_test().with_file(
        xml_paths::DEVELOPMENT_RESULT_XML,
        "<ralph-development>old</ralph-development>",
    );
    let mut fixture = TestFixture::with_workspace(workspace);
    let ctx = fixture.ctx();

    MainEffectHandler::cleanup_development_xml(&ctx, 0);

    drop(ctx);
    assert!(!fixture
        .workspace
        .exists(Path::new(xml_paths::DEVELOPMENT_RESULT_XML)));
}

#[test]
fn test_invoke_review_agent_does_not_clear_stale_issues_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/review_prompt.txt", "prompt")
        .with_file(xml_paths::ISSUES_XML, "<ralph-issues>old</ralph-issues>");
    let mut fixture = TestFixture::with_workspace(workspace);
    let mut ctx = fixture.ctx();
    ctx.developer_agent = "claude";
    ctx.reviewer_agent = "codex";
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_review_agent(&mut ctx, 0)
        .expect("invoke_review_agent should succeed");

    drop(ctx);
    assert!(fixture.workspace.exists(Path::new(xml_paths::ISSUES_XML)));
}

#[test]
fn test_cleanup_review_issues_xml_clears_stale_issues_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(xml_paths::ISSUES_XML, "<ralph-issues>old</ralph-issues>");
    let mut fixture = TestFixture::with_workspace(workspace);
    let ctx = fixture.ctx();

    MainEffectHandler::cleanup_review_issues_xml(&ctx, 0);

    drop(ctx);
    assert!(!fixture.workspace.exists(Path::new(xml_paths::ISSUES_XML)));
}

#[test]
fn test_invoke_fix_agent_does_not_clear_stale_fix_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/fix_prompt.txt", "prompt")
        .with_file(
            xml_paths::FIX_RESULT_XML,
            "<ralph-fix-result>old</ralph-fix-result>",
        );
    let mut fixture = TestFixture::with_workspace(workspace);
    let mut ctx = fixture.ctx();
    ctx.developer_agent = "claude";
    ctx.reviewer_agent = "codex";
    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    handler
        .invoke_fix_agent(&mut ctx, 0)
        .expect("invoke_fix_agent should succeed");

    drop(ctx);
    assert!(fixture
        .workspace
        .exists(Path::new(xml_paths::FIX_RESULT_XML)));
}

#[test]
fn test_cleanup_fix_result_xml_clears_stale_fix_xml() {
    let workspace = MemoryWorkspace::new_test().with_file(
        xml_paths::FIX_RESULT_XML,
        "<ralph-fix-result>old</ralph-fix-result>",
    );
    let mut fixture = TestFixture::with_workspace(workspace);
    let ctx = fixture.ctx();

    MainEffectHandler::cleanup_fix_result_xml(&ctx, 0);

    drop(ctx);
    assert!(!fixture
        .workspace
        .exists(Path::new(xml_paths::FIX_RESULT_XML)));
}

#[test]
fn test_invoke_commit_agent_does_not_clear_stale_commit_xml() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_prompt.txt", "prompt")
        .with_file(
            xml_paths::COMMIT_MESSAGE_XML,
            "<ralph-commit>old</ralph-commit>",
        );
    let mut fixture = TestFixture::with_workspace(workspace);
    let mut ctx = fixture.ctx();
    ctx.developer_agent = "claude";
    ctx.reviewer_agent = "codex";
    let mut handler = MainEffectHandler::new(PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..PipelineState::initial(1, 1)
    });

    handler
        .invoke_commit_agent(&mut ctx)
        .expect("invoke_commit_agent should succeed");

    drop(ctx);
    assert!(fixture
        .workspace
        .exists(Path::new(xml_paths::COMMIT_MESSAGE_XML)));
}

#[test]
fn test_cleanup_commit_xml_clears_stale_commit_xml() {
    let workspace = MemoryWorkspace::new_test().with_file(
        xml_paths::COMMIT_MESSAGE_XML,
        "<ralph-commit>old</ralph-commit>",
    );
    let mut fixture = TestFixture::with_workspace(workspace);
    let ctx = fixture.ctx();
    let handler = MainEffectHandler::new(PipelineState::initial(1, 1));

    let _ = handler.cleanup_commit_xml(&ctx);

    drop(ctx);
    assert!(!fixture
        .workspace
        .exists(Path::new(xml_paths::COMMIT_MESSAGE_XML)));
}
