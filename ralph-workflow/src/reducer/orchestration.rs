//! Orchestration logic for determining next effect.
//!
//! Contains `determine_next_effect()` which decides which effect to execute
//! based on current pipeline state.

use super::event::{CheckpointTrigger, PipelinePhase, RebasePhase};
use super::state::{CommitState, PipelineState, PromptMode, RebaseState};

use crate::agents::AgentRole;
use crate::reducer::effect::{ContinuationContextData, Effect};

include!("orchestration/xsd_retry.rs");
include!("orchestration/phase_effects.rs");

#[cfg(test)]
#[path = "orchestration/tests.rs"]
mod tests;
