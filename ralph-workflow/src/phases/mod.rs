//! Pipeline Phase Orchestration Module
//!
//! This module contains the execution logic for each phase of the Ralph pipeline:
//! - Development phase: iterative planning and execution cycles
//! - Review phase: code review and fix cycles
//! - Commit phase: automated commit message generation
//!
//! Each phase is encapsulated in its own submodule with a clean interface that
//! takes a shared context and returns results. The phases module coordinates
//! overall flow while keeping phase-specific logic separated.
//!
//! # Module Structure
//!
//! - [`context`] - Shared phase context for passing state between phases
//! - [`development`] - Iterative development cycle execution
//! - [`review`] - Code review and fix cycle execution
//! - [`commit`] - Automated commit message generation with fallback
//!
//! # Note on Re-exports
//!
//! The functions below are public for use by the reducer architecture.
//! They were previously private module internals.

pub mod commit;
pub mod commit_logging;
pub mod context;
pub mod development;
pub mod integrity;
pub mod review;

pub use commit::generate_commit_message;
pub use context::{get_primary_commit_agent, PhaseContext};
