//! Pipeline checkpoint system for resume functionality.
//!
//! This module provides checkpoint management for Ralph's pipeline:
//! - Save and load pipeline state
//! - Resume interrupted pipelines
//! - Track pipeline phase progress
//! - Validate checkpoints before resuming
//! - Restore configuration from checkpoints
//!
//! # Example
//!
//! ```ignore
//! use ralph::checkpoint::{
//!     CheckpointBuilder, PipelineCheckpoint, PipelinePhase,
//!     save_checkpoint, load_checkpoint,
//! };
//!
//! // Create a checkpoint using the builder
//! let checkpoint = CheckpointBuilder::new()
//!     .phase(PipelinePhase::Development, 2, 5)
//!     .reviewer_pass(0, 2)
//!     .agents("claude", "codex")
//!     .capture_from_context(&config, &registry, "claude", "codex", &logger)
//!     .build()
//!     .expect("checkpoint should build");
//!
//! save_checkpoint(&checkpoint)?;
//!
//! // Load and resume
//! if let Some(checkpoint) = load_checkpoint()? {
//!     println!("Resuming from: {}", checkpoint.description());
//! }
//! ```

pub mod builder;
pub mod restore;
pub mod state;
pub mod validation;

pub use builder::CheckpointBuilder;
pub use restore::apply_checkpoint_to_config;
pub use state::{
    checkpoint_exists, clear_checkpoint, load_checkpoint, save_checkpoint, timestamp,
    PipelineCheckpoint, PipelinePhase, RebaseState,
};
pub use validation::validate_checkpoint;
