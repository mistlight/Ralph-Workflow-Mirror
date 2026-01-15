//! File protection and integrity for Ralph's agent files.
//!
//! This module handles file protection and integrity verification:
//! - PROMPT.md validation and protection
//! - File integrity verification and checksums
//! - Real-time file system monitoring for PROMPT.md protection
//! - Protection state management
//!
//! # Submodules
//!
//! - [`integrity`](super::integrity) - File integrity and atomic writes
//! - [`monitoring`](super::monitoring) - Real-time PROMPT.md monitoring
//! - [`validation`](super::validation) - PROMPT.md validation

// Core exports (currently used)
pub use super::validation::{restore_prompt_if_needed, validate_prompt_md};
