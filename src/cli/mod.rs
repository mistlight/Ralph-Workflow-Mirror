//! CLI argument parsing and command-line interface definitions.
//!
//! This module contains all CLI-related types and functions:
//! - [`Args`] struct with clap configuration for command-line argument parsing
//! - [`Preset`] enum for preset agent configurations
//! - CLI handler functions for `--list-agents`, `--list-providers`, `--diagnose`
//! - Config initialization handlers for `--init`, `--init-global`
//!
//! # Module Structure
//!
//! - [`args`] - Args struct with clap configuration
//! - [`presets`] - Preset enum and apply_args_to_config
//! - [`providers`] - Provider listing and info display
//! - [`handlers`] - Command handlers (list-agents, diagnose, dry-run)
//! - [`init`] - Config initialization handlers (--init, --init-global)

mod args;
mod handlers;
mod init;
pub mod presets;
mod providers;

// Re-export all public items for backward compatibility
pub use args::Args;
pub use handlers::{
    handle_diagnose, handle_dry_run, handle_list_agents, handle_list_available_agents,
};
pub use init::{ensure_config_or_create, handle_init, handle_init_global};
pub use presets::apply_args_to_config;
pub use providers::handle_list_providers;
