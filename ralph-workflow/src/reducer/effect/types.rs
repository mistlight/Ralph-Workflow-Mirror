use crate::agents::AgentRole;
use crate::phases::PhaseContext;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::event::{CheckpointTrigger, ConflictStrategy, PipelineEvent, RebasePhase};
use super::state::PromptMode;
use super::ui_event::UIEvent;

/// Data for continuation context writing.
///
/// Groups parameters for [`Effect::WriteContinuationContext`] to avoid
/// exceeding the function argument limit.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ContinuationContextData {
    pub iteration: u32,
    pub attempt: u32,
    pub status: super::state::DevelopmentStatus,
    pub summary: String,
    pub files_changed: Option<Vec<String>>,
    pub next_steps: Option<String>,
}

/// Effects represent side-effect operations.
///
/// The reducer determines which effect to execute next based on state.
/// Effect handlers execute effects and emit events.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Effect {
    AgentInvocation {
        role: AgentRole,
        agent: String,
        model: Option<String>,
        prompt: String,
    },

    InitializeAgentChain {
        role: AgentRole,
    },

    /// Prepare the planning prompt for an iteration (single-task).
    ///
    /// This effect must only render/write the prompt that will be used for the
    /// subsequent planning agent invocation.
    PreparePlanningPrompt {
        iteration: u32,
        prompt_mode: PromptMode,
    },

    /// Materialize planning inputs (single-task).
    ///
    /// This effect must perform any oversize handling for planning prompt inputs
    /// (inline vs file references) and emit explicit reducer events recording the
    /// final representation. It must not render/write the planning prompt.
    MaterializePlanningInputs {
        iteration: u32,
    },

    /// Clean up stale planning XML before invoking the planning agent (single-task).
    CleanupPlanningXml {
        iteration: u32,
    },

    /// Invoke the planning agent for an iteration (single-task).
    ///
    /// This effect must only perform agent execution using the prepared planning prompt
    /// (written by `PreparePlanningPrompt`) and must not parse/validate outputs.
    InvokePlanningAgent {
        iteration: u32,
    },

    /// Extract the planning XML from the canonical workspace path (single-task).
    ///
    /// This effect must only verify that `.agent/tmp/plan.xml` exists and is readable.
    /// It must not validate XML, write PLAN.md, or change phase.
    ExtractPlanningXml {
        iteration: u32,
    },

    /// Validate the extracted planning XML (single-task).
    ///
    /// This effect must only validate/parse the XML at `.agent/tmp/plan.xml` and
    /// emit a planning validation event. It must not write PLAN.md, archive files,
    /// or transition phases.
    ValidatePlanningXml {
        iteration: u32,
    },

    /// Write `.agent/PLAN.md` from the validated planning XML (single-task).
    ///
    /// This effect must only write markdown. It must not archive XML or transition phases.
    WritePlanningMarkdown {
        iteration: u32,
    },

    /// Archive `.agent/tmp/plan.xml` after PLAN.md is written (single-task).
    ///
    /// This effect must only archive the canonical plan XML (move to `.processed`).
    ArchivePlanningXml {
        iteration: u32,
    },

    /// Apply the already-validated planning outcome to advance the reducer state (single-task).
    ///
    /// This effect must only emit the appropriate planning outcome event.
    ApplyPlanningOutcome {
        iteration: u32,
        valid: bool,
    },

    /// Prepare development context files for an iteration (single-task).
    ///
    /// This effect must only write any context artifacts needed for the
    /// development prompt and must not render prompts or invoke agents.
    PrepareDevelopmentContext {
        iteration: u32,
    },

    /// Materialize development inputs (single-task).
    ///
    /// This effect must perform any oversize handling for development prompt inputs
    /// (inline vs file references) and emit explicit reducer events recording the
    /// final representation. It must not render/write the development prompt.
    MaterializeDevelopmentInputs {
        iteration: u32,
    },

    /// Prepare the development prompt for an iteration (single-task).
    ///
    /// This effect must only render/write the prompt that will be used for
    /// the subsequent developer agent invocation.
    PrepareDevelopmentPrompt {
        iteration: u32,
        prompt_mode: PromptMode,
    },

    /// Clean up stale development XML before invoking the developer agent (single-task).
    CleanupDevelopmentXml {
        iteration: u32,
    },

    /// Invoke the developer agent for an iteration (single-task).
    ///
    /// This effect must only perform agent execution using the prepared prompt
    /// and must not parse/validate outputs.
    InvokeDevelopmentAgent {
        iteration: u32,
    },

    /// Invoke the analysis agent for an iteration (single-task).
    ///
    /// This effect must only perform agent execution to analyze the git diff
    /// against PLAN.md and produce development_result.xml. It must not parse
    /// or validate outputs - those are handled by subsequent ExtractDevelopmentXml
    /// and ValidateDevelopmentXml effects.
    ///
    /// The analysis agent has no context from development execution and produces
    /// an objective assessment based purely on observable code changes.
    InvokeAnalysisAgent {
        iteration: u32,
    },

    /// Extract the development result XML from the canonical workspace path (single-task).
    ///
    /// This effect must only verify that `.agent/tmp/development_result.xml` exists and is readable.
    /// It must not validate XML, apply outcomes, or archive files.
    ExtractDevelopmentXml {
        iteration: u32,
    },

    /// Validate the extracted development result XML (single-task).
    ///
    /// This effect must only validate/parse the XML at `.agent/tmp/development_result.xml` and
    /// emit a validation event. It must not apply outcomes or archive files.
    ValidateDevelopmentXml {
        iteration: u32,
    },

    /// Apply the already-validated development outcome to advance the reducer state (single-task).
    ///
    /// This effect must only emit the appropriate development outcome event.
    ApplyDevelopmentOutcome {
        iteration: u32,
    },

    /// Archive `.agent/tmp/development_result.xml` after validation (single-task).
    ///
    /// This effect must only archive the canonical development result XML.
    ArchiveDevelopmentXml {
        iteration: u32,
    },

    /// Prepare review context files (single-task).
    ///
    /// This effect must only write the review inputs (prompt backups, diffs, etc.)
    /// needed for a subsequent `AgentInvocation` and must not invoke agents.
    PrepareReviewContext {
        pass: u32,
    },

    /// Materialize review inputs (single-task).
    ///
    /// This effect must perform any oversize handling for review prompt inputs
    /// (inline vs file references) and emit explicit reducer events recording the
    /// final representation. It must not render/write the review prompt.
    MaterializeReviewInputs {
        pass: u32,
    },

    /// Prepare the review prompt for a pass (single-task).
    ///
    /// This effect must only render/write the prompt that will be used for the
    /// subsequent reviewer agent invocation.
    PrepareReviewPrompt {
        pass: u32,
        prompt_mode: PromptMode,
    },

    /// Clean up stale review issues XML before invoking the reviewer agent (single-task).
    CleanupReviewIssuesXml {
        pass: u32,
    },

    /// Invoke the reviewer agent for a review pass (single-task).
    ///
    /// This effect must only perform agent execution using the prepared review prompt
    /// (written by `PrepareReviewPrompt`) and must not parse/validate outputs.
    InvokeReviewAgent {
        pass: u32,
    },

    /// Extract the review issues XML from the canonical workspace path (single-task).
    ///
    /// This effect must only verify that `.agent/tmp/issues.xml` exists and is readable.
    /// It must not validate XML, write ISSUES.md, or change phase.
    ExtractReviewIssuesXml {
        pass: u32,
    },

    /// Validate the extracted review issues XML (single-task).
    ///
    /// This effect must only validate/parses the XML at `.agent/tmp/issues.xml` and
    /// emit a review validation event. It must not write ISSUES.md, archive files,
    /// or transition phases.
    ValidateReviewIssuesXml {
        pass: u32,
    },

    /// Write `.agent/ISSUES.md` from the validated issues XML (single-task).
    ///
    /// This effect must only write markdown. It must not archive XML or transition phases.
    WriteIssuesMarkdown {
        pass: u32,
    },

    /// Extract review issue snippets for a pass (single-task).
    ///
    /// This effect must only extract snippets and emit UI output.
    ExtractReviewIssueSnippets {
        pass: u32,
    },

    /// Archive `.agent/tmp/issues.xml` after ISSUES.md is written (single-task).
    ///
    /// This effect must only archive the canonical issues XML (move to `.processed`).
    ArchiveReviewIssuesXml {
        pass: u32,
    },

    /// Apply the already-validated review outcome to advance the reducer state (single-task).
    ///
    /// This effect must only emit the appropriate review outcome event.
    ApplyReviewOutcome {
        pass: u32,
        issues_found: bool,
        clean_no_issues: bool,
    },

    /// Prepare the fix prompt for a review pass (single-task).
    ///
    /// This effect must only render/write the prompt that will be used for the
    /// subsequent fix agent invocation.
    PrepareFixPrompt {
        pass: u32,
        prompt_mode: PromptMode,
    },

    /// Clean up stale fix result XML before invoking the fix agent (single-task).
    CleanupFixResultXml {
        pass: u32,
    },

    /// Invoke the fix agent for a review pass (single-task).
    ///
    /// This effect must only perform agent execution using the prepared fix prompt
    /// (written by `PrepareFixPrompt`) and must not parse/validate outputs.
    InvokeFixAgent {
        pass: u32,
    },

    /// Extract the fix result XML from the canonical workspace path (single-task).
    ///
    /// This effect must only verify that `.agent/tmp/fix_result.xml` exists and is readable.
    /// It must not validate XML, apply outcomes, or archive files.
    ExtractFixResultXml {
        pass: u32,
    },

    /// Validate the extracted fix result XML (single-task).
    ///
    /// This effect must only validate/parses the XML at `.agent/tmp/fix_result.xml` and
    /// emit a fix validation event. It must not apply outcomes or archive files.
    ValidateFixResultXml {
        pass: u32,
    },

    /// Apply the already-validated fix outcome to advance the reducer state (single-task).
    ///
    /// This effect must only emit the appropriate fix outcome event.
    ApplyFixOutcome {
        pass: u32,
    },

    /// Archive `.agent/tmp/fix_result.xml` after validation (single-task).
    ///
    /// This is intentionally sequenced before `ApplyFixOutcome` so the reducer can
    /// archive artifacts while still in the fix chain (before state transitions
    /// reset per-pass tracking).
    ArchiveFixResultXml {
        pass: u32,
    },

    RunRebase {
        phase: RebasePhase,
        target_branch: String,
    },

    ResolveRebaseConflicts {
        strategy: ConflictStrategy,
    },

    /// Compute the commit diff for the current attempt (single-task).
    ///
    /// This effect must only compute/write the diff and emit whether it is empty.
    CheckCommitDiff,

    /// Prepare the commit prompt (single-task).
    ///
    /// This effect must only render/write the commit prompt that will be used for
    /// the subsequent commit agent invocation. It must not invoke agents or
    /// validate outputs.
    PrepareCommitPrompt {
        prompt_mode: PromptMode,
    },

    /// Materialize commit inputs (single-task).
    ///
    /// This effect must perform any model-budget truncation and inline-vs-reference
    /// handling for commit prompt inputs (notably the diff), and emit explicit
    /// reducer events recording the final representation. It must not render/write
    /// the commit prompt.
    MaterializeCommitInputs {
        attempt: u32,
    },

    /// Invoke the commit agent (single-task).
    ///
    /// This effect must only perform agent execution using the prepared commit prompt
    /// and must not parse/validate outputs.
    InvokeCommitAgent,

    /// Clean up stale commit XML before invoking the commit agent (single-task).
    CleanupCommitXml,

    /// Extract the commit XML from the canonical workspace path (single-task).
    ///
    /// This effect must only verify that `.agent/tmp/commit_message.xml` exists and is readable.
    /// It must not validate XML or archive files.
    ExtractCommitXml,

    /// Validate the extracted commit XML (single-task).
    ///
    /// This effect must only validate/parse the XML at `.agent/tmp/commit_message.xml`
    /// and emit a commit validation event. It must not create commits or archive files.
    ValidateCommitXml,

    /// Apply the already-validated commit message outcome (single-task).
    ///
    /// This effect must only emit the appropriate commit outcome event.
    ApplyCommitMessageOutcome,

    /// Archive `.agent/tmp/commit_message.xml` after validation (single-task).
    ///
    /// This effect must only archive the canonical commit XML (move to `.processed`).
    ArchiveCommitXml,

    CreateCommit {
        message: String,
    },

    SkipCommit {
        reason: String,
    },

    /// Wait for a retry-cycle backoff delay.
    ///
    /// This effect is emitted when the reducer determines the agent chain has
    /// entered a new retry cycle and a backoff delay must be applied before
    /// attempting more work.
    BackoffWait {
        role: AgentRole,
        cycle: u32,
        duration_ms: u64,
    },

    /// Report agent chain exhaustion.
    ///
    /// This effect is emitted when the agent chain has exhausted all retry attempts.
    /// The handler converts this to an ErrorEvent::AgentChainExhausted which the
    /// reducer processes to transition to Interrupted phase.
    ReportAgentChainExhausted {
        role: AgentRole,
        phase: super::event::PipelinePhase,
        cycle: u32,
    },

    ValidateFinalState,

    SaveCheckpoint {
        trigger: CheckpointTrigger,
    },

    /// Ensure required gitignore entries exist (single-task).
    ///
    /// This effect checks the repository's .gitignore for required entries
    /// (`/PROMPT*`, `.agent/`) and adds any missing entries. It runs at
    /// pipeline start before phase-specific work begins.
    ///
    /// The effect is idempotent: if entries already exist, no changes are made.
    /// File write errors are logged as warnings but do not fail the pipeline.
    EnsureGitignoreEntries,

    CleanupContext,

    /// Restore PROMPT.md write permissions after pipeline completion.
    ///
    /// This effect is emitted during the Finalizing phase to restore
    /// write permissions on PROMPT.md so users can edit it normally
    /// after Ralph exits.
    RestorePromptPermissions,

    /// Write continuation context file for next development attempt.
    ///
    /// This effect is emitted when a development iteration returns
    /// partial/failed status and needs to continue. The context file
    /// provides the next attempt with information about what was done.
    ///
    /// The effect handler executes this as part of the development iteration
    /// flow when the reducer determines continuation is needed.
    WriteContinuationContext(ContinuationContextData),

    /// Clean up continuation context file.
    ///
    /// Emitted when an iteration completes successfully or when
    /// starting a fresh iteration (to remove stale context).
    ///
    /// The effect handler executes this as part of the development iteration
    /// flow when the reducer determines cleanup is needed.
    CleanupContinuationContext,

    /// Trigger development agent to fix pipeline failure.
    ///
    /// Invoked when the pipeline reaches AwaitingDevFix phase after agent chain
    /// exhaustion. The dev agent is given the full failure context (logs, error
    /// messages, last state) and asked to diagnose and fix the root cause.
    ///
    /// After completion (success or failure), the pipeline emits a completion
    /// marker and transitions to Interrupted.
    TriggerDevFixFlow {
        /// The phase where the failure occurred.
        failed_phase: super::event::PipelinePhase,
        /// The role of the exhausted agent chain.
        failed_role: AgentRole,
        /// Retry cycle count when exhaustion occurred.
        retry_cycle: u32,
    },

    /// Emit completion marker and transition to Interrupted.
    ///
    /// This effect is emitted after dev-fix flow completes (or skips) to ensure
    /// a completion marker is always written before pipeline termination.
    EmitCompletionMarkerAndTerminate {
        /// Whether the pipeline is terminating due to failure.
        is_failure: bool,
        /// Optional reason for termination.
        reason: Option<String>,
    },

    /// Trigger mandatory loop recovery.
    ///
    /// This effect is emitted when the orchestrator detects that the same effect
    /// has been executed too many times consecutively without state progression.
    /// The handler will reset XSD retry state, clear session IDs, and reset loop
    /// detection counters to break the loop.
    TriggerLoopRecovery {
        /// String representation of the detected loop (for diagnostics).
        detected_loop: String,
        /// Number of times the loop was repeated.
        loop_count: u32,
    },
}

/// Result of executing an effect.
///
/// Contains both the PipelineEvent (for reducer) and optional UIEvents (for display).
/// This separation keeps UI concerns out of the reducer while allowing handlers
/// to emit rich feedback during execution.
///
/// # Multiple Events
///
/// Some effects produce multiple reducer events. For example, agent invocation
/// may produce:
/// 1. `InvocationSucceeded` - the primary event
/// 2. `SessionEstablished` - additional event when session ID is extracted
///
/// The `additional_events` field holds events that should be processed after
/// the primary event. The reducer loop processes all events in order.
#[derive(Clone, Debug)]
pub struct EffectResult {
    /// Primary event for reducer (affects state).
    pub event: PipelineEvent,
    /// Additional events to process after the primary event.
    ///
    /// Used for cases where an effect produces multiple events, such as
    /// agent invocation followed by session establishment. Each event is
    /// processed by the reducer in order.
    pub additional_events: Vec<PipelineEvent>,
    /// UI events for display (do not affect state).
    pub ui_events: Vec<UIEvent>,
}

impl EffectResult {
    /// Create result with just a pipeline event (no UI events).
    pub fn event(event: PipelineEvent) -> Self {
        Self {
            event,
            additional_events: Vec::new(),
            ui_events: Vec::new(),
        }
    }

    /// Create result with pipeline event and UI events.
    pub fn with_ui(event: PipelineEvent, ui_events: Vec<UIEvent>) -> Self {
        Self {
            event,
            additional_events: Vec::new(),
            ui_events,
        }
    }

    /// Add an additional event to process after the primary event.
    ///
    /// Used for emitting separate events like SessionEstablished after
    /// agent invocation completes. Each additional event is processed
    /// by the reducer in order.
    pub fn with_additional_event(mut self, event: PipelineEvent) -> Self {
        self.additional_events.push(event);
        self
    }

    /// Add a UI event to the result.
    pub fn with_ui_event(mut self, ui_event: UIEvent) -> Self {
        self.ui_events.push(ui_event);
        self
    }
}

/// Trait for executing effects.
///
/// Returns EffectResult containing both PipelineEvent (for state) and
/// UIEvents (for display). This allows mocking in tests.
pub trait EffectHandler<'ctx> {
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext<'_>) -> Result<EffectResult>;
}
