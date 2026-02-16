// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_lifecycle_event(state: PipelineState, event: LifecycleEvent) -> PipelineState {
    const MAX_CONSECUTIVE_PUSH_FAILURES: u32 = 3;

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
            push_retry_count: 0,
            last_push_error: None,
            last_pushed_commit: Some(commit_sha),
            ..state
        },
        LifecycleEvent::PushFailed { error, .. } => {
            // Keep pending push on failure so orchestration can retry, but do not
            // spin forever: after a small failure budget, record the commit as
            // unpushed and allow the pipeline to proceed.
            let mut next = PipelineState {
                push_retry_count: state.push_retry_count.saturating_add(1),
                last_push_error: Some(error),
                ..state
            };

            if next.push_retry_count >= MAX_CONSECUTIVE_PUSH_FAILURES {
                if let Some(commit) = next.pending_push_commit.take() {
                    next.unpushed_commits.push(commit);
                }
                next.push_retry_count = 0;
            }

            next
        }
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
