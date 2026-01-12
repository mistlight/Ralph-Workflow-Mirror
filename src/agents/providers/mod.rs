//! OpenCode Provider Types and Authentication Helpers
//!
//! This module handles provider detection from model flags and provides
//! authentication guidance for the 75+ providers supported by OpenCode.
//!
//! # Module Structure
//!
//! - [`types`] - `OpenCodeProviderType` enum definition
//! - [`detection`] - Model flag parsing and provider detection
//! - [`metadata`] - Provider names, prefixes, and auth commands
//! - [`models`] - Example model identifiers per provider
//! - [`validation`] - Model flag validation and auth failure advice

mod detection;
mod metadata;
mod models;
mod types;
mod validation;

#[cfg(test)]
mod tests;

// Re-export public API
pub use detection::strip_model_flag_prefix;
pub use types::OpenCodeProviderType;
pub use validation::{auth_failure_advice, validate_model_flag};
