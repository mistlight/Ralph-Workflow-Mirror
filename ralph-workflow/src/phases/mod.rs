//! Pipeline Phase Orchestration Module
//!
//! This module contains the execution logic for each phase of the Ralph pipeline:
//! - Development phase: iterative planning and execution cycles
//! - Review phase: code review and fix cycles
//! - Commit phase: automated commit message generation
//!
//! Each phase is encapsulated in its own submodule with a clean interface that
//! takes a shared context and returns results. The phases module coordinates
//! the overall flow while keeping phase-specific logic separated.
//!
//! # Module Structure
//!
//! - [`context`] - Shared phase context for passing state between phases
//! - [`development`] - Iterative development cycle execution
//! - [`review`] - Code review and fix cycle execution
//! - [`commit`] - Automated commit message generation with fallback

pub mod commit;
mod context;
mod development;
mod review;

pub use commit::generate_commit_message;
pub use context::PhaseContext;
pub use development::run_development_phase;
pub use review::run_review_phase;
