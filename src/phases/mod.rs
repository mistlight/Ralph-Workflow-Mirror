//! Pipeline phase orchestration module.
//!
//! This module contains the execution logic for each phase of the Ralph pipeline:
//! - Development phase: iterative planning and execution cycles
//! - Review phase: code review and fix cycles
//! - Commit phase: final commit creation
//!
//! Each phase is encapsulated in its own submodule with a clean interface that
//! takes a shared context and returns results. The phases module coordinates
//! the overall flow while keeping phase-specific logic separated.

mod commit;
mod context;
mod development;
mod review;

pub use commit::run_commit_phase;
pub use context::PhaseContext;
pub use development::run_development_phase;
pub use review::{generate_commit_message, run_review_phase};
