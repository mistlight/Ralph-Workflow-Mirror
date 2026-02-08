//! Integration tests for completion marker emission on pipeline failure.
//!
//! Verifies that when the pipeline reaches Status: Failed (AgentChainExhausted),
//! it properly:
//! 1. Transitions to AwaitingDevFix phase
//! 2. Triggers TriggerDevFixFlow effect
//! 3. Emits completion marker to filesystem
//! 4. Transitions to Interrupted phase
//! 5. Saves checkpoint (making is_complete() return true)
//!
//! ## Test Organization
//!
//! - `common` - Shared test fixtures and helpers
//! - `state_transitions` - Reducer state transition tests
//! - `marker_emission` - Completion marker writing tests
//! - `error_handling` - Error recovery and timeout tests
//!
//! ## Key Scenarios Tested
//!
//! ### State Transitions
//! - Planning/Development → AwaitingDevFix on AgentChainExhausted
//! - AwaitingDevFix → Interrupted after marker emission
//! - is_complete() returns true for Interrupted from AwaitingDevFix
//!
//! ### Marker Emission
//! - Completion marker written during TriggerDevFixFlow effect
//! - Marker contains correct failure information
//! - Full event loop properly emits marker and completes
//! - Dev-fix agent dispatched on failure
//!
//! ### Error Handling
//! - Max iterations in AwaitingDevFix triggers defensive completion
//! - Marker write failures don't block completion
//! - SaveCheckpoint panics are caught
//! - SaveCheckpoint errors are reduced as events
//! - Budget exhaustion continues to commit phase

mod common;
mod error_handling;
mod marker_emission;
mod state_transitions;
