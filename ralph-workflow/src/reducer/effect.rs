//! Effect types and handlers for side effects.
//!
//! Effects represent impure operations (git, filesystem, agent execution) that
//! handlers execute on behalf of the reducer. The reducer is pure and determines
//! which effect to execute next; handlers execute effects and produce events.
//!
//! # Key Types
//!
//! - [`Effect`] - Enum of all possible side-effect operations
//! - [`EffectHandler`] - Trait for executing effects (impure code lives here)
//! - [`EffectResult`] - Contains both pipeline event and optional UI events
//!
//! # Single-Task Effect Principle
//!
//! Each Effect variant represents exactly **one** logical operation. Effects must NOT:
//! - Perform multiple unrelated file operations
//! - Combine "decide" and "do" in one effect
//! - Bundle agent execution with parsing, retry, or output writing
//!
//! If an effect needs multiple responsibilities, split it into separate effects.
//! The effect handler executes effects atomically; all coordination happens via
//! reducer state and events.
//!
//! This principle is tested in `reducer_legacy_rejection.rs::test_effects_are_single_task`.
//!
//! **This rule is absolute.**
//!
//! If you touch this codebase for any reason and you notice an effect that bundles
//! hidden logic (multiple responsibilities, policy decisions, retries, phase transitions,
//! parsing + writing + archiving, etc.), you must refactor it into a sequence of
//! single-task effects and explicit events. Do not leave effect "shortcuts" behind.
//!
//! # Redux-Style Event Modeling
//!
//! This project intentionally follows the Redux style-guide guidance:
//! - Think of events/actions as "something that happened" (not "setters")
//! - Keep reducer logic pure and deterministic
//! - Keep state serializable
//! - Put side effects in handlers/middleware (effects), not in reducers
//!
//! References (official Redux docs):
//! - Actions are events: https://redux.js.org/tutorials/fundamentals/part-2-concepts-data-flow
//! - Event-based actions vs setters: https://redux.js.org/style-guide/
//!
//! # Design
//!
//! This separation keeps business logic pure (in reducers) while isolating
//! side effects (in handlers). See [`CODE_STYLE.md`](https://codeberg.org/mistlight/RalphWithReviewer/src/branch/main/CODE_STYLE.md)
//! for the full architecture overview.

#[cfg(test)]
#[path = "effect/tests.rs"]
mod tests;
mod types;

pub use types::{ContinuationContextData, Effect, EffectHandler, EffectResult};
