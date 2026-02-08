//! Orchestration logic for determining next effect.
//!
//! This module implements the **pure orchestration layer** that derives effects from state.
//! The orchestrator is a critical component of the reducer architecture that bridges
//! state transitions with effect execution.
//!
//! # Pure Function Contract
//!
//! All orchestration functions are **PURE**:
//! - Input: `&PipelineState` (immutable reference to current state)
//! - Output: `Effect` (intention to perform side effects)
//! - No I/O operations (no filesystem, network, environment access)
//! - No side effects (no logging, no mutations, no hidden state)
//! - Deterministic: same state always produces same effect
//!
//! # Architecture Flow
//!
//! ```text
//! State → determine_next_effect() → Effect → Handler → Event → Reducer → State
//!         ^^^^^^^^^^^^^^^^^^^^^^
//!         Pure orchestration (this module)
//! ```
//!
//! The orchestrator examines state and derives the next effect:
//! 1. Check for pending recovery operations (continuation cleanup, loop recovery)
//! 2. Check for retry/fallback conditions (XSD retry, agent retry)
//! 3. Determine normal phase progression effect
//!
//! # Decision Priority
//!
//! Orchestration checks conditions in priority order:
//! 1. **Recovery**: Continuation cleanup, loop recovery
//! 2. **Retry**: XSD retry pending, same-agent retry pending
//! 3. **Continuation**: Agent requested continuation
//! 4. **Normal**: Phase-specific progression
//! 5. **Transition**: Advance to next phase
//!
//! # Testing Strategy
//!
//! Orchestrators are pure functions - test them without mocks:
//!
//! ```ignore
//! #[test]
//! fn test_xsd_retry_pending_derives_cleanup_effect() {
//!     let state = PipelineState {
//!         continuation: ContinuationState {
//!             xsd_retry_pending: true,
//!             ..Default::default()
//!         },
//!         ..test_state()
//!     };
//!
//!     let effect = determine_next_effect(&state);
//!
//!     assert!(matches!(effect, Effect::CleanupDevelopmentXml { .. }));
//! }
//! ```
//!
//! See [`tests`] module for comprehensive orchestration tests.

use super::event::{CheckpointTrigger, PipelinePhase, RebasePhase};
use super::state::{CommitState, PipelineState, PromptMode, RebaseState};

use crate::agents::AgentRole;
use crate::reducer::effect::{ContinuationContextData, Effect};

mod phase_effects;
use phase_effects::determine_next_effect_for_phase;

include!("orchestration/xsd_retry.rs");

#[cfg(test)]
#[path = "orchestration/tests.rs"]
mod tests;
