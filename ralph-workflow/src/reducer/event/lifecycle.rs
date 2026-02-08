//! Pipeline lifecycle events (start, stop, resume).
//!
//! These events control the overall pipeline execution lifecycle,
//! distinct from phase-specific transitions.

use serde::{Deserialize, Serialize};

/// Pipeline lifecycle events (start, stop, resume).
///
/// These events control the overall pipeline execution lifecycle,
/// distinct from phase-specific transitions. Use these for:
///
/// - Starting or resuming a pipeline run
/// - Completing a successful pipeline execution
///
/// # When to Use
///
/// - `Started`: When a fresh pipeline run begins
/// - `Resumed`: When resuming from a checkpoint
/// - `Completed`: When all phases complete successfully
///
/// # ⚠️ FROZEN - DO NOT ADD VARIANTS ⚠️
///
/// This enum is **FROZEN**. Adding new variants is **PROHIBITED**.
///
/// ## Why is this frozen?
///
/// Lifecycle events control pipeline flow (start/stop/completion). Allowing effect
/// handlers to emit new lifecycle events would violate the core architectural principle:
/// **handlers describe what happened; reducers decide what happens next.**
///
/// ## What to do instead
///
/// If you need to express new observations or failures:
///
/// 1. **Reuse existing phase/category events** - Use `PlanningEvent`, `DevelopmentEvent`,
///    `ReviewEvent`, `CommitEvent`, etc. to describe what happened within that phase.
///    Example: `PlanningEvent::PlanXmlMissing` instead of creating a generic "Aborted" event.
///
/// 2. **Return errors from the event loop** - For truly unrecoverable failures (permission
///    errors, invariant violations), return `Err` from the effect handler. The outer runner
///    will handle termination, not the reducer.
///
/// 3. **Handle in orchestration** - Some conditions don't need events at all and can be
///    handled in the effect handler or runner logic.
///
/// ## Enforcement
///
/// The freeze policy is enforced by the `lifecycle_event_is_frozen` test in the parent module,
/// which will fail to compile if new variants are added. This is intentional.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum LifecycleEvent {
    /// Pipeline execution started fresh (not from checkpoint).
    Started,
    /// Pipeline execution resumed from a previous state.
    Resumed {
        /// Whether this resume is from a persisted checkpoint.
        from_checkpoint: bool,
    },
    /// Pipeline execution completed successfully.
    Completed,
}
