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
//! - [`template_selection`]: Interactive template selection when PROMPT.md is missing
//! - [`advanced_help`]: Comprehensive advanced help display

pub mod advanced_help;
pub mod diagnose;
pub mod dry_run;
pub mod list;
pub mod template_selection;

// Re-export handlers at module level for convenience
pub use advanced_help::handle_help_advanced;
pub use diagnose::handle_diagnose;
pub use dry_run::handle_dry_run;
pub use list::{handle_list_agents, handle_list_available_agents};
pub use template_selection::{create_prompt_from_template, prompt_template_selection};
