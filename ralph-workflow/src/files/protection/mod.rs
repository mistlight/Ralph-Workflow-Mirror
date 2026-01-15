//! File protection and integrity for Ralph's agent files.
//!
//! This module handles file protection and integrity verification:
//! - PROMPT.md validation and protection
//! - Real-time file system monitoring for PROMPT.md protection
//! - Protection state management
//!
//! # Submodules
//!
//! - [`monitoring`] - Real-time PROMPT.md monitoring
//! - [`validation`] - PROMPT.md validation

pub mod monitoring;
pub(in crate::files) mod validation;

// Core exports (currently used)
pub use validation::{restore_prompt_if_needed, validate_prompt_md};
