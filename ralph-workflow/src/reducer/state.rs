//! Pipeline state types for reducer architecture.
//!
//! Defines immutable state structures that capture complete pipeline execution context.
//! These state structures can be serialized as checkpoints for resume functionality.

use crate::agents::AgentRole;
use crate::checkpoint::execution_history::ExecutionStep;
use crate::checkpoint::{
    PipelineCheckpoint, PipelinePhase as CheckpointPhase, RebaseState as CheckpointRebaseState,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::event::PipelinePhase;

// State enums and basic types (ArtifactType, PromptMode, DevelopmentStatus, FixStatus, RebaseState, CommitState)
include!("state/enums.rs");

// Continuation state for development and fix iterations
include!("state/continuation.rs");

// Agent fallback chain state and backoff computation
include!("state/agent_chain.rs");

// Pipeline state, validated outcomes, and checkpoint conversion
include!("state/pipeline.rs");

// Tests
include!("state/tests.rs");
