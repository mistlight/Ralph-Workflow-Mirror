//! CLI argument parsing and command-line interface definitions.
//!
//! This module contains all CLI-related types and functions:
//! - [`Args`] struct with clap configuration for command-line argument parsing
//! - `Preset` enum for preset agent configurations
//! - CLI handler functions for `--list-agents`, `--list-providers`, `--diagnose`
//! - Config initialization handlers for `--init`, `--init-global`, `--init-legacy`
//! - Interactive template selection for PROMPT.md creation
//!
//! # Module Structure
//!
//! - [`args`] - Args struct with clap configuration
//! - [`presets`] - Preset enum and apply_args_to_config
//! - [`providers`] - Provider listing and info display
//! - [`handlers`] - Command handlers (list-agents, diagnose, dry-run, template-selection)
//! - [`init`] - Config initialization handlers (--init, --init-global, --init-legacy)

mod args;
mod handlers;
mod init;
pub mod presets;
mod providers;

// Re-export all public items for backward compatibility
pub use args::Args;
pub use handlers::{
    create_prompt_from_template, handle_diagnose, handle_dry_run, handle_list_agents,
    handle_list_available_agents, prompt_template_selection,
};
pub use init::{handle_init_global, handle_init_legacy, handle_init_prompt, handle_list_templates};
pub use presets::apply_args_to_config;
pub use providers::handle_list_providers;
