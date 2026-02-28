use super::common::TestFixture;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::workspace::{MemoryWorkspace, Workspace};

#[test]
fn test_invoke_analysis_agent_gracefully_handles_missing_plan_and_diff() {
    // Regression: analysis should still run even when PLAN.md is missing or git diff cannot
    // be generated. These inputs should be substituted with placeholders.
    let workspace = MemoryWorkspace::new_test().with_dir(".agent/tmp");
    let mut fixture = TestFixture::with_workspace(workspace);
    let mut ctx = fixture.ctx();
    ctx.developer_agent = "claude";

    let mut handler = MainEffectHandler::new(PipelineState {
        phase: crate::reducer::event::PipelinePhase::Development,
        iteration: 0,
        ..PipelineState::initial(1, 0)
    });

    handler
        .invoke_analysis_agent(&mut ctx, 0)
        .expect("invoke_analysis_agent should not fail when PLAN/DIFF inputs are missing");

    // Validate that the agent was invoked and the prompt has the analysis task structure.
    //
    // This test is intentionally resilient to environments where a real git repository is
    // discoverable from the process CWD (e.g., when running unit tests from a checkout).
    // In those cases, diff generation can succeed even if the in-memory workspace is missing
    // `.agent/start_commit`, so the prompt will contain an actual diff instead of a
    // "[DIFF unavailable" placeholder.
    let calls = fixture.executor.agent_calls();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.contains("Your task is to VERIFY whether the code changes satisfy the PLAN"),
        "expected analysis prompt header in prompt, got: {prompt}"
    );
    // Validate that the DIFF section contains either a placeholder or an actual diff.
    assert!(
        prompt.contains("[DIFF unavailable") || prompt.contains("diff --git"),
        "expected diff placeholder or an actual git diff in prompt, got: {prompt}"
    );
}

#[test]
fn test_invoke_analysis_agent_writes_diff_backup_when_git_diff_succeeds() {
    // When git diff generation succeeds, the handler should still write/update
    // `.agent/DIFF.backup` as a best-effort fallback for prompt materialization.
    let workspace = MemoryWorkspace::new_test()
        .with_dir(".agent/tmp")
        .with_file(".agent/PLAN.md", "# Plan\n")
        .with_file(".agent/DIFF.backup", "DIFF_BACKUP_MARKER");

    let mut fixture = TestFixture::with_workspace(workspace);
    let mut ctx = fixture.ctx();
    ctx.developer_agent = "claude";

    let mut handler = MainEffectHandler::new(PipelineState {
        phase: crate::reducer::event::PipelinePhase::Development,
        iteration: 0,
        ..PipelineState::initial(1, 0)
    });

    handler
        .invoke_analysis_agent(&mut ctx, 0)
        .expect("invoke_analysis_agent should succeed");

    let calls = fixture.executor.agent_calls();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.contains("diff --git") || prompt.contains("[DIFF unavailable"),
        "expected a git diff or a diff-unavailable placeholder in prompt"
    );

    let backup = fixture
        .workspace
        .read(std::path::Path::new(".agent/DIFF.backup"))
        .expect("expected .agent/DIFF.backup to exist");
    assert!(
        backup.contains("diff --git") || backup.contains("[DIFF unavailable"),
        "expected .agent/DIFF.backup to contain a git diff or placeholder"
    );
    assert_ne!(
        backup, "DIFF_BACKUP_MARKER",
        "expected .agent/DIFF.backup to be refreshed"
    );
}
