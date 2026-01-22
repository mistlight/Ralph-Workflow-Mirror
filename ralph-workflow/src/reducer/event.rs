//! Pipeline event types for reducer architecture.
//!
//! Defines all possible events that can occur during pipeline execution.
//! Each event represents a state transition that the reducer handles.

use crate::agents::AgentRole;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Pipeline phases for checkpoint tracking.
///
/// These phases represent the major stages of the Ralph pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelinePhase {
    Planning,
    Development,
    Review,
    CommitMessage,
    FinalValidation,
    Complete,
    Interrupted,
}

impl std::fmt::Display for PipelinePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning => write!(f, "Planning"),
            Self::Development => write!(f, "Development"),
            Self::Review => write!(f, "Review"),
            Self::CommitMessage => write!(f, "Commit Message"),
            Self::FinalValidation => write!(f, "Final Validation"),
            Self::Complete => write!(f, "Complete"),
            Self::Interrupted => write!(f, "Interrupted"),
        }
    }
}

/// Pipeline events representing all state transitions.
///
/// Each event captures an observable transition in pipeline execution.
/// The reducer handles these events to compute new state.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PipelineEvent {
    PipelineStarted,
    PipelineResumed {
        from_checkpoint: bool,
    },
    PipelineCompleted,
    PipelineAborted {
        reason: String,
    },

    PlanningPhaseStarted,
    PlanningPhaseCompleted,

    DevelopmentPhaseStarted,
    DevelopmentIterationStarted {
        iteration: u32,
    },
    PlanGenerationStarted {
        iteration: u32,
    },
    PlanGenerationCompleted {
        iteration: u32,
        valid: bool,
    },
    DevelopmentIterationCompleted {
        iteration: u32,
        output_valid: bool,
    },
    DevelopmentPhaseCompleted,

    ReviewPhaseStarted,
    ReviewPassStarted {
        pass: u32,
    },
    ReviewCompleted {
        pass: u32,
        issues_found: bool,
    },
    FixAttemptStarted {
        pass: u32,
    },
    FixAttemptCompleted {
        pass: u32,
        changes_made: bool,
    },
    ReviewPhaseCompleted {
        early_exit: bool,
    },

    AgentInvocationStarted {
        role: AgentRole,
        agent: String,
        model: Option<String>,
    },
    AgentInvocationSucceeded {
        role: AgentRole,
        agent: String,
    },
    AgentInvocationFailed {
        role: AgentRole,
        agent: String,
        exit_code: i32,
        error_kind: AgentErrorKind,
        retriable: bool,
    },
    AgentFallbackTriggered {
        role: AgentRole,
        from_agent: String,
        to_agent: String,
    },
    AgentModelFallbackTriggered {
        role: AgentRole,
        agent: String,
        from_model: String,
        to_model: String,
    },
    AgentRetryCycleStarted {
        role: AgentRole,
        cycle: u32,
    },
    AgentChainExhausted {
        role: AgentRole,
    },
    AgentChainInitialized {
        role: AgentRole,
        agents: Vec<String>,
    },

    RebaseStarted {
        phase: RebasePhase,
        target_branch: String,
    },
    RebaseConflictDetected {
        files: Vec<PathBuf>,
    },
    RebaseConflictResolved {
        files: Vec<PathBuf>,
    },
    RebaseSucceeded {
        phase: RebasePhase,
        new_head: String,
    },
    RebaseFailed {
        phase: RebasePhase,
        reason: String,
    },
    RebaseAborted {
        phase: RebasePhase,
        restored_to: String,
    },
    RebaseSkipped {
        phase: RebasePhase,
        reason: String,
    },

    CommitGenerationStarted,
    CommitMessageGenerated {
        message: String,
        attempt: u32,
    },
    CommitMessageValidationFailed {
        reason: String,
        attempt: u32,
    },
    CommitCreated {
        hash: String,
        message: String,
    },
    CommitGenerationFailed {
        reason: String,
    },
    CommitSkipped {
        reason: String,
    },

    CheckpointSaved {
        trigger: CheckpointTrigger,
    },
}

/// Rebase phase (initial or post-review).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebasePhase {
    Initial,
    PostReview,
}

/// Error kind for agent failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentErrorKind {
    Network,
    Authentication,
    RateLimit,
    Timeout,
    InternalError,
    ModelUnavailable,
    ParsingError,
    FileSystem,
}

/// Conflict resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    Abort,
    Continue,
    Skip,
}

/// Checkpoint save trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointTrigger {
    PhaseTransition,
    IterationComplete,
    BeforeRebase,
    Interrupt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_phase_display() {
        assert_eq!(format!("{}", PipelinePhase::Planning), "Planning");
        assert_eq!(format!("{}", PipelinePhase::Development), "Development");
        assert_eq!(format!("{}", PipelinePhase::Review), "Review");
        assert_eq!(
            format!("{}", PipelinePhase::CommitMessage),
            "Commit Message"
        );
        assert_eq!(
            format!("{}", PipelinePhase::FinalValidation),
            "Final Validation"
        );
        assert_eq!(format!("{}", PipelinePhase::Complete), "Complete");
        assert_eq!(format!("{}", PipelinePhase::Interrupted), "Interrupted");
    }
}
