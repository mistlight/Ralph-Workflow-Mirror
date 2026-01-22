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
//! ┌──────────────────────────────────────────────────────────┐
//! │                     Pipeline State                        │
//! │  (immutable: phase, iteration, agent_chain, history)     │
//! └──────────────────────────────────────────────────────────┘
//!                           │
//!                           ▼
//! ┌──────────────────────────────────────────────────────────┐
//! │                        Reducer                            │
//! │       fn reduce(state: State, event: Event) -> State     │
//! │                   [Pure, no side effects]                │
//! └──────────────────────────────────────────────────────────┘
//!                           ▲
//!                           │
//! ┌──────────────────────────────────────────────────────────┐
//! │                        Events                             │
//! │  DevelopmentIterationCompleted | AgentFailed |           │
//! │  ReviewPassCompleted | RebaseSucceeded | ...             │
//! └──────────────────────────────────────────────────────────┘
//!                           ▲
//!                           │
//! ┌──────────────────────────────────────────────────────────┐
//! │                   Effect Handlers                         │
//! │  (Agent execution, file I/O, git operations)             │
//! │       [Side effects isolated here]                       │
//! └──────────────────────────────────────────────────────────┘
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
