//! Checkpoint migration from v3 format to reducer state.
//!
//! Implements migration from existing checkpoint format to new PipelineState.

use crate::checkpoint::PipelineCheckpoint;
use crate::checkpoint::PipelinePhase as CheckpointPhase;
use crate::checkpoint::RebaseState as CheckpointRebaseState;

use super::event::PipelinePhase;
use super::state::{PipelineState, RebaseState};

/// Convert v3 checkpoint to reducer PipelineState.
///
/// Migrates v3 checkpoint structure to reducer state format. Only v3 format
/// is supported; legacy formats (v1, v2) are rejected at the checkpoint
/// loading layer before this conversion is called.
impl From<PipelineCheckpoint> for PipelineState {
    fn from(checkpoint: PipelineCheckpoint) -> Self {
        let rebase_state = migrate_rebase_state(&checkpoint.rebase_state);

        let agent_chain = super::state::AgentChainState::initial();

        PipelineState {
            phase: migrate_phase(checkpoint.phase),
            previous_phase: None, // No previous phase for migrated checkpoints
            iteration: checkpoint.iteration,
            total_iterations: checkpoint.total_iterations,
            reviewer_pass: checkpoint.reviewer_pass,
            total_reviewer_passes: checkpoint.total_reviewer_passes,
            review_issues_found: false, // Default to false for migrated checkpoints
            context_cleaned: false,     // Default to false for migrated checkpoints
            agent_chain,
            rebase: rebase_state,
            commit: super::state::CommitState::NotStarted,
            execution_history: checkpoint
                .execution_history
                .map(|h| h.steps)
                .unwrap_or_default(),
            // Default to no continuation in progress (v3 checkpoints predate continuation)
            continuation: super::state::ContinuationState::new(),
        }
    }
}

/// Migrate checkpoint phase to reducer phase.
///
/// Note: Legacy phases (Fix, ReviewAgain) are rejected at the checkpoint
/// deserialization layer before this function is called.
fn migrate_phase(phase: CheckpointPhase) -> PipelinePhase {
    match phase {
        CheckpointPhase::Rebase => PipelinePhase::Planning,
        CheckpointPhase::Planning => PipelinePhase::Planning,
        CheckpointPhase::Development => PipelinePhase::Development,
        CheckpointPhase::Review => PipelinePhase::Review,
        CheckpointPhase::CommitMessage => PipelinePhase::CommitMessage,
        CheckpointPhase::FinalValidation => PipelinePhase::FinalValidation,
        CheckpointPhase::Complete => PipelinePhase::Complete,
        CheckpointPhase::PreRebase => PipelinePhase::Planning,
        CheckpointPhase::PreRebaseConflict => PipelinePhase::Planning,
        CheckpointPhase::PostRebase => PipelinePhase::CommitMessage,
        CheckpointPhase::PostRebaseConflict => PipelinePhase::CommitMessage,
        CheckpointPhase::Interrupted => PipelinePhase::Interrupted,
    }
}

/// Migrate checkpoint rebase state to reducer rebase state.
fn migrate_rebase_state(rebase_state: &CheckpointRebaseState) -> RebaseState {
    match rebase_state {
        CheckpointRebaseState::NotStarted => RebaseState::NotStarted,
        CheckpointRebaseState::PreRebaseInProgress { upstream_branch } => RebaseState::InProgress {
            original_head: "HEAD".to_string(),
            target_branch: upstream_branch.clone(),
        },
        CheckpointRebaseState::PreRebaseCompleted { commit_oid } => RebaseState::Completed {
            new_head: commit_oid.clone(),
        },
        CheckpointRebaseState::PostRebaseInProgress { upstream_branch } => {
            RebaseState::InProgress {
                original_head: "HEAD".to_string(),
                target_branch: upstream_branch.clone(),
            }
        }
        CheckpointRebaseState::PostRebaseCompleted { commit_oid } => RebaseState::Completed {
            new_head: commit_oid.clone(),
        },
        CheckpointRebaseState::HasConflicts { files } => RebaseState::Conflicted {
            original_head: "HEAD".to_string(),
            target_branch: "main".to_string(),
            files: files
                .iter()
                .map(|s| std::path::PathBuf::from(s.clone()))
                .collect(),
            resolution_attempts: 0,
        },
        CheckpointRebaseState::Failed { .. } => RebaseState::Skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_phase_planning() {
        let phase = migrate_phase(CheckpointPhase::Planning);
        assert_eq!(phase, PipelinePhase::Planning);
    }

    #[test]
    fn test_migrate_phase_development() {
        let phase = migrate_phase(CheckpointPhase::Development);
        assert_eq!(phase, PipelinePhase::Development);
    }

    #[test]
    fn test_migrate_phase_review() {
        let phase = migrate_phase(CheckpointPhase::Review);
        assert_eq!(phase, PipelinePhase::Review);
    }

    #[test]
    fn test_migrate_phase_commit_message() {
        let phase = migrate_phase(CheckpointPhase::CommitMessage);
        assert_eq!(phase, PipelinePhase::CommitMessage);
    }

    #[test]
    fn test_migrate_phase_complete() {
        let phase = migrate_phase(CheckpointPhase::Complete);
        assert_eq!(phase, PipelinePhase::Complete);
    }

    #[test]
    fn test_migrate_rebase_state_not_started() {
        let state = migrate_rebase_state(&CheckpointRebaseState::NotStarted);
        assert!(matches!(state, RebaseState::NotStarted));
    }

    #[test]
    fn test_migrate_rebase_state_pre_in_progress() {
        let state = migrate_rebase_state(&CheckpointRebaseState::PreRebaseInProgress {
            upstream_branch: "main".to_string(),
        });
        assert!(matches!(state, RebaseState::InProgress { .. }));
    }

    #[test]
    fn test_migrate_rebase_state_has_conflicts() {
        let files = vec!["file1.rs".to_string(), "file2.rs".to_string()];
        let state = migrate_rebase_state(&CheckpointRebaseState::HasConflicts {
            files: files.clone(),
        });
        assert!(matches!(state, RebaseState::Conflicted { .. }));
    }

    #[test]
    fn test_migrate_rebase_state_completed() {
        let state = migrate_rebase_state(&CheckpointRebaseState::PreRebaseCompleted {
            commit_oid: "abc123".to_string(),
        });
        assert!(matches!(state, RebaseState::Completed { .. }));
    }
}
