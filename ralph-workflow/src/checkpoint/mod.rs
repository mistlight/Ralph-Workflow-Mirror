//! Pipeline checkpoint system for resume functionality.
//!
//! This module provides checkpoint management for Ralph's pipeline:
//! - Save and load pipeline state
//! - Resume interrupted pipelines
//! - Track pipeline phase progress
//!
//! # Example
//!
//! ```ignore
//! use ralph::checkpoint::{PipelineCheckpoint, PipelinePhase, save_checkpoint, load_checkpoint};
//!
//! // Save a checkpoint
//! let checkpoint = PipelineCheckpoint::new(
//!     PipelinePhase::Development,
//!     2,  // current iteration
//!     5,  // total iterations
//!     0,  // reviewer pass
//!     2,  // total reviewer passes
//!     "claude",
//!     "codex",
//! );
//! save_checkpoint(&checkpoint)?;
//!
//! // Load and resume
//! if let Some(checkpoint) = load_checkpoint()? {
//!     println!("Resuming from: {}", checkpoint.description());
//! }
//! ```

pub mod state;

pub use state::{
    checkpoint_exists, clear_checkpoint, load_checkpoint, save_checkpoint, timestamp,
    PipelineCheckpoint, PipelinePhase,
};
