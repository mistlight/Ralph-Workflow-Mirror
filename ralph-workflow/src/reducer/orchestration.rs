//! Orchestration logic for determining next effect.
//!
//! Contains `determine_next_effect()` which decides which effect to execute
//! based on current pipeline state.

use super::event::{CheckpointTrigger, PipelinePhase, RebasePhase};
use super::state::{CommitState, PipelineState, PromptMode, RebaseState};

use crate::agents::AgentRole;
use crate::reducer::effect::{ContinuationContextData, Effect};

include!("orchestration/part1.rs");
include!("orchestration/part2.rs");

#[cfg(test)]
#[path = "orchestration/tests.rs"]
mod tests;
