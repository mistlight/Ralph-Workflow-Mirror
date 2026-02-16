// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_lifecycle_event(state: PipelineState, event: LifecycleEvent) -> PipelineState {
    match event {
        LifecycleEvent::Started => state,
        LifecycleEvent::Resumed { .. } => state,
        LifecycleEvent::Completed => PipelineState {
            phase: crate::reducer::event::PipelinePhase::Complete,
            ..state
        },
        LifecycleEvent::GitignoreEntriesEnsured { .. } => {
            // Set flag to prevent re-running effect
            PipelineState {
                gitignore_entries_ensured: true,
                ..state
            }
        }
        LifecycleEvent::GitAuthConfigured => PipelineState {
            git_auth_configured: true,
            ..state
        },
        LifecycleEvent::PushCompleted { commit_sha, .. } => PipelineState {
            pending_push_commit: None,
            push_count: state.push_count + 1,
            last_pushed_commit: Some(commit_sha),
            ..state
        },
        LifecycleEvent::PushFailed { .. } => PipelineState {
            // Clear pending push on failure (graceful degradation)
            pending_push_commit: None,
            ..state
        },
        LifecycleEvent::PullRequestCreated { url, number } => PipelineState {
            pr_created: true,
            pr_url: Some(url),
            pr_number: Some(number),
            ..state
        },
        LifecycleEvent::PullRequestFailed { .. } => state, // Log but don't change state
        LifecycleEvent::CloudProgressReported => state,    // No state change
        LifecycleEvent::CloudProgressFailed { .. } => state, // Log but don't change state
    }
}
