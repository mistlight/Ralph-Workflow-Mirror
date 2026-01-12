//! CLI command handlers.
//!
//! Contains handler functions for CLI commands like --list-agents,
//! --diagnose, and --dry-run.
//!
//! # Module Structure
//!
//! - [`diagnose`]: Comprehensive diagnostic output for troubleshooting
//! - [`dry_run`]: Validation without running agents
//! - [`list`]: Agent listing commands

pub mod diagnose;
pub mod dry_run;
pub mod list;

// Re-export handlers at module level for convenience
pub use diagnose::handle_diagnose;
pub use dry_run::handle_dry_run;
pub use list::{handle_list_agents, handle_list_available_agents};
