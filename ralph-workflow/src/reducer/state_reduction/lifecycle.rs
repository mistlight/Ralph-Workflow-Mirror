// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_lifecycle_event(state: PipelineState, event: LifecycleEvent) -> PipelineState {
    match event {
        LifecycleEvent::Started => state,
        LifecycleEvent::Resumed { .. } => PipelineState {
            // A resumed run is active again; do not keep treating it as a user-interrupted
            // termination path. This flag is used ONLY to exempt Ctrl+C termination from the
            // pre-termination commit safety check.
            interrupted_by_user: false,
            ..state
        },
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumed_clears_user_interrupt_flag_for_resumed_run() {
        // A checkpoint may record that the previous process ended via Ctrl+C.
        // After we resume, the pipeline is running again and MUST NOT keep treating
        // itself as a user-interrupted termination.
        let mut state = PipelineState::initial(1, 0);
        state.interrupted_by_user = true;

        let reduced = reduce_lifecycle_event(
            state,
            LifecycleEvent::Resumed {
                from_checkpoint: true,
            },
        );

        assert!(
            !reduced.interrupted_by_user,
            "Resumed runs must clear interrupted_by_user so termination safety checks behave correctly"
        );
    }
}
