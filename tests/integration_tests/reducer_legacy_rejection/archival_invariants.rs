//! Integration tests for archival invariants with legacy artifacts.
//!
//! Verifies that legacy artifacts from previous Ralph versions don't affect
//! pipeline execution. The reducer must derive all decisions from events,
//! not from file presence or content.
//!
//! Observable behaviors tested:
//! - Legacy PLAN.md files are ignored during planning
//! - Legacy ISSUES.md files are ignored during review
//! - Pipeline decisions come from events, not file system state
//! - Effect determination is independent of legacy artifacts
//!
//! # Integration Test Compliance
//!
//! These tests follow [../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md):
//! - Test observable behavior: effect determination
//! - Use MemoryWorkspace to simulate legacy files
//! - Verify event-driven architecture

use crate::common::with_locked_prompt_permissions;
use crate::test_timeout::with_default_timeout;
use std::path::Path;

// ============================================================================
// LEGACY ARTIFACT IGNORED DURING EXECUTION TESTS
// ============================================================================

/// Test that legacy artifacts in workspace don't affect effect determination.
///
/// When legacy files (e.g., ISSUES.md, PLAN.md from old versions) exist
/// in the workspace, the pipeline should NOT read them to derive results.
/// All pipeline decisions must come from reducer events/effects, not file presence.
#[test]
fn test_legacy_artifacts_ignored_during_execution() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;

    with_default_timeout(|| {
        // Create state in Development phase with agents initialized
        let mut state = with_locked_prompt_permissions(PipelineState::initial(2, 1));
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Effect determination should NOT depend on workspace file existence
        // (determine_next_effect is a pure function of state)
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::PrepareDevelopmentContext { .. }),
            "Effect should be determined from state alone, got {:?}",
            effect
        );

        // Even with max iterations reached, state-based transition should happen
        let mut state = with_locked_prompt_permissions(PipelineState::initial(0, 1));
        state.phase = PipelinePhase::Review;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Reviewer,
        );

        // Effect determination for review should not check for legacy ISSUES.md
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::PrepareReviewContext { .. }),
            "Review effect should be determined from state alone, got {:?}",
            effect
        );
    });
}

/// Test that legacy artifact files in workspace are completely ignored.
///
/// Even when legacy files exist in the workspace (ISSUES.md, PLAN.md, commit.xml),
/// the pipeline must not read them to derive results. All results must come from
/// the current XML paths. This test explicitly creates these files and verifies
/// determine_next_effect remains unchanged.
#[test]
fn test_legacy_artifact_files_completely_ignored() {
    use ralph_workflow::agents::AgentRole;
    use ralph_workflow::reducer::effect::Effect;
    use ralph_workflow::reducer::event::PipelinePhase;
    use ralph_workflow::reducer::orchestration::determine_next_effect;
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::workspace::MemoryWorkspace;

    with_default_timeout(|| {
        // Create workspace with legacy artifact files that should be ignored
        let _workspace = MemoryWorkspace::new_test()
            .with_file("ISSUES.md", "# Legacy Issues\n- Issue 1\n- Issue 2")
            .with_file("PLAN.md", "# Legacy Plan\n\nDo legacy things")
            .with_file(
                ".agent/tmp/commit.xml",
                "<commit><message>Legacy</message></commit>",
            )
            .with_dir(".agent/logs/planning_1"); // Legacy directory mode

        // Create state in Development phase
        let mut state = with_locked_prompt_permissions(PipelineState::initial(2, 1));
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["test-agent".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // Effect determination must be pure - workspace contents must not affect it
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::PrepareDevelopmentContext { .. }),
            "Effect must be determined from state alone, not workspace files"
        );

        // Verify the workspace has our legacy files (confirming test setup)
        // Note: We don't actually check workspace because determine_next_effect
        // is stateless - it only takes &PipelineState, not &Workspace
        // This demonstrates the architectural invariant that effects are pure.
    });
}

// ============================================================================
// .PROCESSED ARCHIVE TESTS (NO FALLBACK READS)
// ============================================================================

/// Test that `.processed` files are archive-only and never used as fallback reads.
///
/// This applies to all canonical XML outputs. If the primary XML is missing, the
/// pipeline must NOT consult the archived `.processed` file.
#[test]
fn test_processed_files_are_archive_only_for_all_outputs() {
    use ralph_workflow::files::llm_output_extraction::file_based_extraction::try_extract_from_file_with_workspace;
    use ralph_workflow::workspace::MemoryWorkspace;

    with_default_timeout(|| {
        let cases = [
            (".agent/tmp/plan.xml", "<plan>archived</plan>"),
            (".agent/tmp/issues.xml", "<issues>archived</issues>"),
            (
                ".agent/tmp/development_result.xml",
                "<development>archived</development>",
            ),
            (".agent/tmp/fix_result.xml", "<fix>archived</fix>"),
            (
                ".agent/tmp/commit_message.xml",
                "<commit_message>archived</commit_message>",
            ),
        ];

        let mut workspace = MemoryWorkspace::new_test();
        for (primary_path, content) in cases {
            workspace = workspace.with_file(&format!("{primary_path}.processed"), content);
        }

        for (primary_path, _) in cases {
            let result = try_extract_from_file_with_workspace(&workspace, Path::new(primary_path));
            assert!(
                result.is_none(),
                "{primary_path}.processed must not be used as a fallback input"
            );
        }
    });
}

/// Test that legacy `commit.xml` is not used as a fallback when commit message XML is missing.
#[test]
fn test_legacy_commit_xml_is_not_used_for_commit_message_extraction() {
    use ralph_workflow::files::llm_output_extraction::file_based_extraction::try_extract_from_file_with_workspace;
    use ralph_workflow::workspace::MemoryWorkspace;

    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_file(
            ".agent/tmp/commit.xml",
            "<commit><message>legacy</message></commit>",
        );

        let result = try_extract_from_file_with_workspace(
            &workspace,
            Path::new(".agent/tmp/commit_message.xml"),
        );

        assert!(
            result.is_none(),
            "commit_message.xml missing must not fall back to legacy commit.xml"
        );
    });
}

/// Test that archived XML files use .processed suffix consistently.
///
/// All XML archiving must use the `.processed` suffix for consistency.
/// This ensures the fallback pattern in handlers works correctly.
#[test]
fn test_archived_xml_uses_processed_suffix() {
    use ralph_workflow::files::llm_output_extraction::archive_xml_file_with_workspace;
    use ralph_workflow::workspace::{MemoryWorkspace, Workspace};

    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/tmp/plan.xml", "<plan>test</plan>")
            .with_file(".agent/tmp/issues.xml", "<issues>test</issues>")
            .with_file(
                ".agent/tmp/development_result.xml",
                "<development>test</development>",
            )
            .with_file(".agent/tmp/fix_result.xml", "<fix>test</fix>")
            .with_file(".agent/tmp/commit_message.xml", "<commit>test</commit>");

        // Archive each file
        let paths = [
            ".agent/tmp/plan.xml",
            ".agent/tmp/issues.xml",
            ".agent/tmp/development_result.xml",
            ".agent/tmp/fix_result.xml",
            ".agent/tmp/commit_message.xml",
        ];

        for path in paths {
            archive_xml_file_with_workspace(&workspace, Path::new(path));

            // Original should be gone
            assert!(
                !workspace.exists(Path::new(path)),
                "Original file should be removed after archiving: {}",
                path
            );

            // .processed should exist
            let processed_path = format!("{}.processed", path);
            assert!(
                workspace.exists(Path::new(&processed_path)),
                "Archived file should have .processed suffix: {}",
                processed_path
            );
        }
    });
}
