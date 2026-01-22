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
pub mod orchestration;
pub mod state;
pub mod state_reduction;

pub use effect::{Effect, EffectHandler};
pub use event::{AgentErrorKind, ConflictStrategy, PipelineEvent, RebasePhase};
pub use fault_tolerant_executor::{execute_agent_fault_tolerantly, AgentExecutionConfig};
pub use handler::MainEffectHandler;
pub use orchestration::determine_next_effect;
pub use state::{AgentChainState, CommitState, PipelineState, RebaseState};
pub use state_reduction::reduce;

// Re-export CheckpointTrigger for external use
pub use event::CheckpointTrigger;
