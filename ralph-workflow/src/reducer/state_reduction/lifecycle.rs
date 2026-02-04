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
    }
}
