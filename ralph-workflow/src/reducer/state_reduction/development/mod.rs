//! Development phase reducer module
//!
//! This module implements PURE reducer logic for the Development phase of the pipeline.
//!
//! # Purity Guarantees
//!
//! This is a PURE REDUCER - absolutely NO I/O or side effects:
//! - No filesystem access (`std::fs`)
//! - No environment variables (`std::env`)
//! - No logging (`tracing`)
//! - No `println!` or similar output
//! - Deterministic transformations only
//!
//! # Development Phase Flow
//!
//! The Development phase handles the iterative process of generating code changes:
//!
//! 1. **Phase Start**: Initialize development-specific state
//! 2. **Iteration Lifecycle**: Start iteration → prepare context → invoke agents → validate → complete
//! 3. **Continuation Logic**: Handle partial/failed outputs via continuation attempts
//! 4. **Completion**: Transition to `CommitMessage` phase on success or `AwaitingDevFix` on exhaustion
//!
//! # Event Types
//!
//! - **Lifecycle events**: `PhaseStarted`, `IterationStarted`, `IterationCompleted`, `PhaseCompleted`
//! - **Step events**: `ContextPrepared`, `PromptPrepared`, `XmlCleaned`, `AgentInvoked`, etc.
//! - **Continuation events**: `ContinuationTriggered`, `ContinuationSucceeded`, `ContinuationBudgetExhausted`
//! - **Error events**: `OutputValidationFailed`, `XmlMissing`
//!
//! # Module Structure
//!
//! - `iteration_reducer`: Handles iteration lifecycle and step completion events
//! - `continuation_reducer`: Handles continuation and retry logic
//! - `helpers`: Shared pure helper functions

mod continuation_reducer;
mod helpers;
mod iteration_reducer;

use crate::reducer::event::DevelopmentEvent;
use crate::reducer::state::PipelineState;

/// Main entry point for reducing development events.
///
/// This is a PURE function - no I/O, no side effects, deterministic only.
pub(super) fn reduce_development_event(
    state: PipelineState,
    event: DevelopmentEvent,
) -> PipelineState {
    match event {
        // Iteration lifecycle and step events
        DevelopmentEvent::PhaseStarted
        | DevelopmentEvent::IterationStarted { .. }
        | DevelopmentEvent::ContextPrepared { .. }
        | DevelopmentEvent::PromptPrepared { .. }
        | DevelopmentEvent::XmlCleaned { .. }
        | DevelopmentEvent::AgentInvoked { .. }
        | DevelopmentEvent::AnalysisAgentInvoked { .. }
        | DevelopmentEvent::XmlExtracted { .. }
        | DevelopmentEvent::XmlValidated { .. }
        | DevelopmentEvent::XmlArchived { .. }
        | DevelopmentEvent::OutcomeApplied { .. }
        | DevelopmentEvent::IterationCompleted { .. }
        | DevelopmentEvent::PhaseCompleted => {
            iteration_reducer::reduce_iteration_event(state, event)
        }

        // Continuation and retry events
        DevelopmentEvent::ContinuationTriggered { .. }
        | DevelopmentEvent::ContinuationSucceeded { .. }
        | DevelopmentEvent::ContinuationBudgetExhausted { .. }
        | DevelopmentEvent::OutputValidationFailed { .. }
        | DevelopmentEvent::XmlMissing { .. }
        | DevelopmentEvent::ContinuationContextWritten { .. }
        | DevelopmentEvent::ContinuationContextCleaned => {
            continuation_reducer::reduce_continuation_event(state, event)
        }
    }
}
