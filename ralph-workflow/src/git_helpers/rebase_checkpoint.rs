//! Rebase checkpoint system for fault tolerance.
//!
//! This module provides types and persistence for rebase state,
//! allowing recovery from interrupted or failed rebase operations.
//!
//! # Workspace Support
//!
//! This module provides two sets of functions:
//! - Standard functions using `std::fs` for production use
//! - `_with_workspace` variants for testability with `MemoryWorkspace`

#![deny(unsafe_code)]

use crate::workspace::Workspace;
use std::fs;
use std::io;
use std::path::Path;

/// Default directory for Ralph's internal files.
const AGENT_DIR: &str = ".agent";

/// Rebase checkpoint file name.
const REBASE_CHECKPOINT_FILE: &str = "rebase_checkpoint.json";

/// Get the rebase checkpoint file path.
///
/// The checkpoint is stored in `.agent/rebase_checkpoint.json`
/// relative to the current working directory.
#[must_use]
pub fn rebase_checkpoint_path() -> String {
    format!("{AGENT_DIR}/{REBASE_CHECKPOINT_FILE}")
}

/// Get the rebase checkpoint backup file path.
///
/// The backup is stored in `.agent/rebase_checkpoint.json.bak`
/// and is used for corruption recovery.
#[must_use]
pub fn rebase_checkpoint_backup_path() -> String {
    format!("{AGENT_DIR}/{REBASE_CHECKPOINT_FILE}.bak")
}

include!("rebase_checkpoint/types.rs");
include!("rebase_checkpoint/persistence.rs");
include!("rebase_checkpoint/tests.rs");
