// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_rebase_event(state: PipelineState, event: RebaseEvent) -> PipelineState {
    match event {
        RebaseEvent::Started {
            target_branch,
            phase: _,
        } => PipelineState {
            rebase: RebaseState::InProgress {
                original_head: state.current_head(),
                target_branch,
            },
            ..state
        },
        RebaseEvent::ConflictDetected { files } => PipelineState {
            rebase: match &state.rebase {
                RebaseState::InProgress {
                    original_head,
                    target_branch,
                } => RebaseState::Conflicted {
                    original_head: original_head.clone(),
                    target_branch: target_branch.clone(),
                    files,
                    resolution_attempts: 0,
                },
                _ => state.rebase.clone(),
            },
            ..state
        },
        RebaseEvent::ConflictResolved { .. } => PipelineState {
            rebase: match &state.rebase {
                RebaseState::Conflicted {
                    original_head,
                    target_branch,
                    ..
                } => RebaseState::InProgress {
                    original_head: original_head.clone(),
                    target_branch: target_branch.clone(),
                },
                _ => state.rebase.clone(),
            },
            ..state
        },
        RebaseEvent::Succeeded { new_head, .. } => PipelineState {
            rebase: RebaseState::Completed { new_head },
            ..state
        },
        RebaseEvent::Failed { .. } => PipelineState {
            rebase: RebaseState::NotStarted,
            ..state
        },
        RebaseEvent::Skipped { .. } => PipelineState {
            rebase: RebaseState::Skipped,
            ..state
        },
        RebaseEvent::Aborted { .. } => state,
    }
}
