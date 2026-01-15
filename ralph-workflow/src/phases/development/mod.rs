//! Development phase execution.
//!
//! This module handles the development phase of the Ralph pipeline, which consists
//! of iterative planning and execution cycles. Each iteration:
//! 1. Creates a PLAN.md from PROMPT.md
//! 2. Executes the plan
//! 3. Deletes PLAN.md
//! 4. Optionally runs fast checks
//!
//! # Module Structure
//!
//! - [`execution`] - Core development iteration execution logic
//! - [`planning`] - Plan generation and extraction
//! - [`util`] - Helper functions for verification and fast checks

mod execution;
mod planning;
mod util;

pub use execution::run_development_phase;
