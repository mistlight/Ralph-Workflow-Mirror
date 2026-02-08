//! Configuration file generation and validation.
//!
//! This module handles creating Ralph config files and PROMPT.md templates:
//! - Global config creation (`--init-global`)
//! - Local config creation (`--init-local-config`)
//! - Config validation (`--check-config`)
//! - PROMPT.md generation (`--init`)
//!
//! # Module Organization
//!
//! - [`global`] - Global config file creation
//! - [`local`] - Local config file creation  
//! - [`validation`] - Config validation and error display
//! - [`prompt`] - PROMPT.md creation from templates
//!
//! All handlers accept a [`ConfigEnvironment`](crate::config::ConfigEnvironment) for
//! dependency injection, enabling tests to use in-memory storage instead of real filesystem.

mod global;
mod local;
mod prompt;
mod validation;

// Re-export public API
// Re-export public API
pub use global::{handle_init_global, handle_init_global_with, handle_init_none_exist_with_env};
pub use local::{handle_init_local_config, handle_init_local_config_with};
pub use prompt::{
    handle_init_only_config_exists_with_env, handle_init_only_prompt_exists_with_env,
    handle_init_state_inference_with_env, handle_init_template_arg_at_path_with_env,
};
pub use validation::{handle_check_config, handle_check_config_with};
