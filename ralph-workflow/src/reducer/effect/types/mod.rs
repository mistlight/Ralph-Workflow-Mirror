//! Pipeline effect system types.
//!
//! Defines the effect types used in the pipeline reducer architecture.
//! Effects represent intentions to perform side effects (I/O, git operations, etc.).
//!
//! ## Architecture
//!
//! Effects are intentionally single-task and granular:
//! - `PrepareDevelopmentPrompt`: Prepare prompt for development
//! - `InvokeAgent`: Execute agent CLI
//! - `ExtractXml`: Parse and extract XML from output
//! - `ValidateXml`: Validate XML against XSD schema
//!
//! Handlers execute effects and report outcomes as events.
//! Reducers process events to update state.
//! Orchestration derives next effect from current state.
//!
//! ## Module Organization
//!
//! - `effect_enum`: The `Effect` enum with all variants
//! - `effect_result`: `EffectResult` struct and helper methods
//! - `handler_trait`: `EffectHandler` trait definition
//!
//! See `docs/architecture/effect-system.md` for details.

mod effect_enum;
mod effect_result;
mod handler_trait;

pub use effect_enum::{ContinuationContextData, Effect};
pub use effect_result::EffectResult;
pub use handler_trait::EffectHandler;
