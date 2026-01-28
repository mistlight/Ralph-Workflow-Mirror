//! Reducer-based pipeline architecture.
//!
//! This module implements the event-sourced reducer architecture from RFC-004.
//! It provides:
//! - Pure state reduction with explicit event transitions
//! - Immutable pipeline state that doubles as checkpoint
//! - Event log for debugging and replay
//! - Effect handlers for side effects (git operations, agent execution)
//!
//! # Architecture
//!
//! ```
//! ┌──────────────────────────────────────────────────┐
//! │                     Pipeline State                        │
//! │  (immutable: phase, iteration, agent_chain, history)     │
//! └──────────────────────────────────────────────────┘
//!                           │
//!                           ▼
//! ┌──────────────────────────────────────────────────┐
//! │                        Reducer                            │
//! │       fn reduce(state: State, event: Event) -> State     │
//! │                   [Pure, no side effects]                │
//! └──────────────────────────────────────────────────┘
//!                           ▲
//!                           │
//! ┌──────────────────────────────────────────────────┐
//! │                        Events                             │
//! │  DevelopmentIterationCompleted | AgentFailed |           │
//! │  ReviewPassCompleted | RebaseSucceeded | ...             │
//! └──────────────────────────────────────────────────┘
//!                           ▲
//!                           │
//! ┌──────────────────────────────────────────────────┐
//! │                   Effect Handlers                         │
//! │  (Agent execution, file I/O, git operations)             │
//! │       [Side effects isolated here]                       │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! # Quick Start
//!
//! Run pipeline with reducer:
//!
//! ```ignore
//! use ralph_workflow::reducer::{run_event_loop, PipelineState};
//!
//! let state = PipelineState::initial(developer_iters, reviewer_reviews);
//! let result = run_event_loop(&mut phase_ctx, Some(state), Default::default())?;
//! ```
//!
//! # State Inspection
//!
//! Inspect pipeline state at any point:
//!
//! ```ignore
//! println!("Current phase: {}", state.phase);
//! println!("Iteration: {}/{}", state.iteration, state.total_iterations);
//! println!("Current agent: {:?}", state.agent_chain.current_agent());
//! ```
//!
//! # Event Replay
//!
//! Replay events from log:
//!
//! ```ignore
//! let final_state = events.into_iter()
//!     .fold(initial_state, |s, e| reduce(s, e));
//! ```
//!
//! # Testing Strategy
//!
//! The reducer architecture is designed for extensive testability:
//!
//! ## Unit Tests
//!
//! - **Pure reducer**: `reduce()` function has no side effects, 100% testable
//! - **State transitions**: Each event → state transition tested in state_reduction.rs tests
//! - **Agent chain**: Fallback logic tested via AgentChainState methods
//! - **Error classification**: All error kinds tested in fault_tolerant_executor.rs
//!
//! ## Integration Tests
//!
//! - **State machine**: Real pipeline execution verifies correct phase transitions
//! - **Event replay**: Event logs can reproduce final state deterministically
//!
//! # Testing Reducer Purity
//!
//! Reducer is easy to test - pure function with no side effects:
//!
//! ```ignore
//! #[test]
//! fn test_agent_fallback() {
//!     let state = create_test_state();
//!     let event = PipelineEvent::AgentInvocationFailed { ... };
//!     let new_state = reduce(state, event);
//!     assert_eq!(new_state.agent_chain.current_agent_index, 1);
//! }
//! ```
//!
//! # Running Tests
//!
//! ```bash
//! # Unit tests only
//! cargo test -p ralph-workflow --lib --all-features
//!
//! # Integration tests
//! cargo test -p ralph-workflow-tests --all-targets
//!
//! # With coverage
//! cargo test -p ralph-workflow --lib --all-features -- --nocapture
//! ```

pub mod effect;
pub mod event;
pub mod fault_tolerant_executor;
pub mod handler;
pub mod migration;
#[cfg(any(test, feature = "test-utils"))]
pub mod mock_effect_handler;
pub mod orchestration;
pub mod state;
pub mod state_reduction;
pub mod ui_event;
pub mod xml_renderer;

#[cfg(test)]
mod orchestration_tests;

#[cfg(test)]
mod tests;

pub use effect::{EffectHandler, EffectResult};
pub use event::PipelineEvent;
pub use handler::MainEffectHandler;
pub use orchestration::determine_next_effect;
pub use state::PipelineState;
pub use state_reduction::reduce;
pub use ui_event::UIEvent;

// Re-export CheckpointTrigger for external use
pub use event::CheckpointTrigger;
