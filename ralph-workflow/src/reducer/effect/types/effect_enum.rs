//! Effect type definitions for the Ralph pipeline.
//!
//! This module defines the [`Effect`] enum, which represents all side-effect operations
//! that can be executed by the pipeline. Effects are determined by the reducer's orchestration
//! logic and executed by effect handlers.
//!
//! ## Why This File Is Large (500 lines)
//!
//! Per CODE_STYLE.md, this file is an acceptable exception to the 300-line guideline because it's
//! a **comprehensive enum** with 60+ variants that must remain together for exhaustiveness checking.
//! Splitting the enum would break pattern matching across the codebase.
//!
//! ## Architecture Note
//!
//! Effects are part of the reducer architecture's event-sourced pipeline:
//! ```text
//! State → Orchestrator → Effect → Handler → Event → Reducer → State
//! ```
//!
//! The Effect enum defines the vocabulary of operations the pipeline can execute. Each variant
//! corresponds to a single, focused side-effect operation (e.g., invoke agent, write file,
//! validate XML).
//!
//! ## See Also
//!
//! - `docs/architecture/effect-system.md` - Effect system design
//! - `reducer::handler` - Effect handler implementations
//! - `reducer::state_reduction` - Orchestration logic that determines effects

use crate::agents::AgentRole;
use serde::{Deserialize, Serialize};

use crate::reducer::event::{CheckpointTrigger, ConflictStrategy, PipelinePhase, RebasePhase};
use crate::reducer::state::{DevelopmentStatus, PromptMode};

/// Data for continuation context writing.
///
/// Groups parameters for [`Effect::WriteContinuationContext`] to avoid
/// exceeding the function argument limit.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ContinuationContextData {
    pub iteration: u32,
    pub attempt: u32,
    pub status: DevelopmentStatus,
    pub summary: String,
    /// Files changed in previous attempt. Box<[String]> saves 8 bytes per instance
    /// vs Vec<String> since this collection is never modified after construction.
    pub files_changed: Option<Box<[String]>>,
    pub next_steps: Option<String>,
}

/// Types of recovery reset operations.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RecoveryResetType {
    /// Reset to the start of a phase (clear phase-specific progress flags).
    PhaseStart,
    /// Reset iteration counter (decrement and restart from Planning).
    IterationReset,
    /// Complete reset (iteration 0, restart from Planning).
    CompleteReset,
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

    /// Check for uncommitted changes before pipeline termination (single-task).
    ///
    /// This effect runs `git status --porcelain` to detect uncommitted work.
    /// If changes exist, routes back to CommitMessage phase to commit them.
    /// If no changes, emits `PreTerminationSafetyCheckPassed` to proceed with termination.
    ///
    /// THE ONLY EXCEPTION: User-initiated Ctrl+C (interrupted_by_user=true) skips this check
    /// and proceeds directly to termination, respecting the user's explicit interrupt choice.
    CheckUncommittedChangesBeforeTermination,

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
        phase: PipelinePhase,
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

    /// Lock PROMPT.md with read-only permissions at pipeline startup.
    ///
    /// This effect is emitted before any phase-specific work to protect
    /// the user prompt from accidental modification during execution.
    /// Best-effort operation; failures emit warnings but don't block pipeline.
    LockPromptPermissions,

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
    /// attempt completion event so the recovery loop can advance. Termination
    /// (and completion marker emission) only occurs via `EmitCompletionMarkerAndTerminate`
    /// after recovery exhaustion.
    TriggerDevFixFlow {
        /// The phase where the failure occurred.
        failed_phase: PipelinePhase,
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

    /// Emit recovery reset events to escalate recovery strategy.
    ///
    /// This effect is derived when dev-fix recovery escalates beyond simple retry.
    /// The handler emits events that reset state appropriately (phase start, iteration
    /// reset, or complete reset) and then the orchestrator derives the next effect.
    EmitRecoveryReset {
        /// Type of reset to perform.
        reset_type: RecoveryResetType,
        /// Target phase to reset to.
        target_phase: PipelinePhase,
    },

    /// Attempt recovery by transitioning back to failed phase.
    ///
    /// This effect is derived when dev-fix completes and recovery should be attempted.
    /// The handler emits RecoveryAttempted event which transitions back to the failed
    /// phase, allowing normal orchestration to derive the recovery effect.
    ///
    /// This is used for level 1 recovery (retry same operation). For level 2+ recovery
    /// (phase reset, iteration reset, complete reset), use EmitRecoveryReset instead.
    AttemptRecovery {
        /// The escalation level being attempted.
        level: u32,
        /// The attempt count.
        attempt_count: u32,
    },

    /// Emit RecoverySucceeded event to clear recovery state after successful work completion.
    ///
    /// This effect is derived when the pipeline successfully completes work after a recovery
    /// attempt (e.g., Planning validates, Development completes). The handler emits
    /// RecoverySucceeded event which clears recovery tracking fields (dev_fix_attempt_count,
    /// recovery_escalation_level, failed_phase_for_recovery) and allows normal operation to resume.
    EmitRecoverySuccess {
        /// The escalation level that succeeded.
        level: u32,
        /// Total attempts before success.
        total_attempts: u32,
    },

    // ========================================================================
    // Cloud Mode Effects (INTERNAL USE ONLY)
    // ========================================================================
    //
    // These effects are only emitted when cloud mode is enabled (RALPH_CLOUD_MODE=true).
    // In CLI mode, these effects are never derived by orchestration.
    //
    // Cloud mode is environment-variable only and not exposed to users.
    // See PROMPT.md for full cloud integration architecture.
    /// Configure git authentication for remote operations (cloud mode only).
    ///
    /// This effect runs once at pipeline start (before first commit) to set up
    /// credentials for all subsequent push operations. It configures git based on
    /// the authentication method specified in cloud configuration.
    ///
    /// Only emitted when cloud_config.enabled is true.
    ConfigureGitAuth {
        /// Serialized authentication method for logging/debugging.
        /// The actual auth config comes from cloud_config in PhaseContext.
        auth_method: String,
    },

    /// Push commits to remote repository (cloud mode only).
    ///
    /// This effect is emitted immediately after every successful CreateCommit effect
    /// when cloud mode is enabled. The orchestrator sequences: CreateCommit -> PushToRemote.
    ///
    /// This ensures incremental progress is visible on the remote and survives pipeline failures.
    ///
    /// Only emitted when cloud_config.enabled is true and a pending push exists in state.
    PushToRemote {
        /// Remote name (e.g., "origin")
        remote: String,
        /// Branch to push
        branch: String,
        /// Whether to force push
        force: bool,
        /// The commit SHA being pushed (for reporting)
        commit_sha: String,
    },

    /// Create a pull request on the remote platform (cloud mode only).
    ///
    /// This effect is emitted during Finalizing phase when create_pr is enabled in
    /// cloud configuration. The PR is created after all commits are pushed, summarizing
    /// the full run.
    ///
    /// Uses platform-specific CLI tools (gh for GitHub, glab for GitLab).
    ///
    /// Only emitted when cloud_config.enabled and cloud_config.git_remote.create_pr are true.
    CreatePullRequest {
        /// Target branch for the PR
        base_branch: String,
        /// Source branch (the pushed branch)
        head_branch: String,
        /// PR title
        title: String,
        /// PR body/description
        body: String,
    },
}
