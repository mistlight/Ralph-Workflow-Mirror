//! Pipeline checkpoint state and persistence.
//!
//! This module contains the checkpoint data structures and file operations
//! for saving and loading pipeline state.
//!
//! # Checkpoint Format
//!
//! Only version 3 checkpoints are supported. Earlier versions (v1, v2) are explicitly
//! rejected during loading. The following legacy phases are also rejected:
//! - "Fix" (use "Development" instead)
//! - "ReviewAgain" (use "Review" instead)
//!
//! # Backwards Compatibility
//!
//! Legacy checkpoint formats are not supported. Users must delete old
//! checkpoints and start a fresh pipeline run.

use chrono::Local;
use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::workspace::Workspace;

include!("state/types/snapshots_and_phases.rs");
include!("state/types/checkpoint.rs");
include!("state/serialization.rs");

#[cfg(test)]
mod tests {
    use super::*;

    include!("state/tests.rs");
}
