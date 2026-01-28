//! Pipeline Phase Orchestration Module
//!
//! This module contains the execution logic for each phase of the Ralph pipeline.
//! Phases are invoked by the reducer architecture via effects, keeping business
//! logic (when to transition) separate from execution logic (how to execute).
//!
//! # Pipeline Phases
//!
//! Ralph runs four sequential phases:
//!
//! ```text
//! ┌──────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
//! │   Planning   │ ──▶ │ Development  │ ──▶ │    Review    │ ──▶ │    Commit    │
//! │              │     │              │     │              │     │              │
//! │ Creates PLAN │     │ Implements   │     │ Reviews code │     │ Generates    │
//! │ from PROMPT  │     │ iterations   │     │ and fixes    │     │ commit msg   │
//! └──────────────┘     └──────────────┘     └──────────────┘     └──────────────┘
//! ```
//!
//! ## Planning Phase
//!
//! Reads `PROMPT.md` and generates `.agent/PLAN.md` via AI agent.
//! The plan guides subsequent development iterations.
//!
//! ## Development Phase
//!
//! Executes N iterations (configurable via `-D`) where the agent:
//! 1. Reads `PROMPT.md` and `PLAN.md`
//! 2. Implements changes
//! 3. Commits after each iteration (if changes detected)
//!
//! ## Review Phase
//!
//! Runs M review cycles (configurable via `-R`) where:
//! 1. Reviewer agent analyzes cumulative diff since pipeline start
//! 2. Creates `.agent/ISSUES.md` with findings
//! 3. Developer agent fixes issues
//! 4. Repeats until no issues or max cycles reached
//!
//! ## Commit Phase
//!
//! Generates a final commit message via AI, analyzing the full diff.
//! Falls back to intelligent heuristic-based message if AI fails.
//!
//! # Module Structure
//!
//! - [`context`] - Shared [`PhaseContext`] for passing state between phases
//! - [`development`] - Iterative development cycle execution
//! - [`review`] - Code review and fix cycle execution
//! - [`commit`] - Automated commit message generation
//! - [`integrity`] - File integrity verification
//!
//! # Integration with Reducer
//!
//! The reducer (see [`crate::reducer`]) determines which phase to execute via
//! [`crate::reducer::determine_next_effect`]. Phase modules are invoked by
//! effect handlers, returning events that update pipeline state.
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
