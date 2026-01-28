//! Pipeline state types for reducer architecture.
//!
//! Defines immutable state structures that capture complete pipeline execution context.
//! These state structures can be serialized as checkpoints for resume functionality.

use crate::agents::AgentRole;
use crate::checkpoint::execution_history::ExecutionStep;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::event::PipelinePhase;

/// Development status from agent output.
///
/// These values map to the `<ralph-status>` element in development_result.xml.
/// Used to track whether work is complete or needs continuation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevelopmentStatus {
    /// Work completed successfully - no continuation needed.
    Completed,
    /// Work partially done - needs continuation.
    Partial,
    /// Work failed - needs continuation with different approach.
    Failed,
}

impl std::fmt::Display for DevelopmentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Completed => write!(f, "completed"),
            Self::Partial => write!(f, "partial"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Continuation state for development iterations.
///
/// Tracks context from previous attempts within the same iteration to enable
/// continuation-aware prompting when status is "partial" or "failed".
///
/// # When Fields Are Populated
///
/// - `previous_status`: Set when DevelopmentIterationContinuationTriggered event fires
/// - `previous_summary`: Set when DevelopmentIterationContinuationTriggered event fires
/// - `previous_files_changed`: Set when DevelopmentIterationContinuationTriggered event fires
/// - `previous_next_steps`: Set when DevelopmentIterationContinuationTriggered event fires
/// - `continuation_attempt`: Incremented on each continuation within same iteration
///
/// # Reset Triggers
///
/// The continuation state is reset (cleared) when:
/// - A new iteration starts (DevelopmentIterationStarted event)
/// - Status becomes "completed" (ContinuationSucceeded event)
/// - Phase transitions away from Development
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ContinuationState {
    /// Status from the previous attempt ("partial" or "failed").
    pub previous_status: Option<DevelopmentStatus>,
    /// Summary of what was accomplished in the previous attempt.
    pub previous_summary: Option<String>,
    /// Files changed in the previous attempt.
    pub previous_files_changed: Option<Vec<String>>,
    /// Agent's recommended next steps from the previous attempt.
    pub previous_next_steps: Option<String>,
    /// Current continuation attempt number (0 = first attempt, 1+ = continuation).
    pub continuation_attempt: u32,
    /// Count of invalid XML outputs for the current iteration.
    #[serde(default)]
    pub invalid_output_attempts: u32,
}

impl ContinuationState {
    /// Create a new empty continuation state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this is a continuation attempt (not the first attempt).
    pub fn is_continuation(&self) -> bool {
        self.continuation_attempt > 0
    }

    /// Reset the continuation state for a new iteration.
    pub fn reset(&self) -> Self {
        Self::default()
    }

    /// Trigger a continuation with context from the previous attempt.
    pub fn trigger_continuation(
        &self,
        status: DevelopmentStatus,
        summary: String,
        files_changed: Option<Vec<String>>,
        next_steps: Option<String>,
    ) -> Self {
        Self {
            previous_status: Some(status),
            previous_summary: Some(summary),
            previous_files_changed: files_changed,
            previous_next_steps: next_steps,
            continuation_attempt: self.continuation_attempt + 1,
            invalid_output_attempts: 0,
        }
    }
}

/// Immutable pipeline state (this IS the checkpoint).
///
/// Contains all information needed to resume pipeline execution at any point.
/// The reducer updates this state by returning new immutable copies on each event.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PipelineState {
    pub phase: PipelinePhase,
    pub previous_phase: Option<PipelinePhase>,
    pub iteration: u32,
    pub total_iterations: u32,
    pub reviewer_pass: u32,
    pub total_reviewer_passes: u32,
    pub review_issues_found: bool,
    pub context_cleaned: bool,
    pub agent_chain: AgentChainState,
    pub rebase: RebaseState,
    pub commit: CommitState,
    pub execution_history: Vec<ExecutionStep>,
    /// Continuation state for development iterations.
    ///
    /// Tracks context from previous attempts when status is "partial" or "failed"
    /// to enable continuation-aware prompting.
    #[serde(default)]
    pub continuation: ContinuationState,
}

impl PipelineState {
    pub fn initial(developer_iters: u32, reviewer_reviews: u32) -> Self {
        // Determine initial phase based on what work needs to be done
        let initial_phase = if developer_iters == 0 {
            // No development iterations → skip Planning and Development
            if reviewer_reviews == 0 {
                // No review passes either → go straight to commit
                PipelinePhase::CommitMessage
            } else {
                PipelinePhase::Review
            }
        } else {
            PipelinePhase::Planning
        };

        Self {
            phase: initial_phase,
            previous_phase: None,
            iteration: 0,
            total_iterations: developer_iters,
            reviewer_pass: 0,
            total_reviewer_passes: reviewer_reviews,
            review_issues_found: false,
            context_cleaned: false,
            agent_chain: AgentChainState::initial(),
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            execution_history: Vec::new(),
            continuation: ContinuationState::new(),
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(
            self.phase,
            PipelinePhase::Complete | PipelinePhase::Interrupted
        )
    }

    pub fn current_head(&self) -> String {
        self.rebase
            .current_head()
            .unwrap_or_else(|| "HEAD".to_string())
    }
}

/// Agent fallback chain state (explicit, not loop indices).
///
/// Tracks position in the multi-level fallback chain:
/// - Agent level (primary → fallback1 → fallback2)
/// - Model level (within each agent, try different models)
/// - Retry cycle (exhaust all agents, start over with exponential backoff)
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentChainState {
    pub agents: Vec<String>,
    pub current_agent_index: usize,
    pub models_per_agent: Vec<Vec<String>>,
    pub current_model_index: usize,
    pub retry_cycle: u32,
    pub max_cycles: u32,
    pub current_role: AgentRole,
    /// Prompt context preserved from rate-limited agent for continuation.
    ///
    /// When an agent hits 429, we save the prompt here so the next agent
    /// can continue the same work instead of starting from scratch.
    #[serde(default)]
    pub rate_limit_continuation_prompt: Option<String>,
}

impl AgentChainState {
    pub fn initial() -> Self {
        Self {
            agents: Vec::new(),
            current_agent_index: 0,
            models_per_agent: Vec::new(),
            current_model_index: 0,
            retry_cycle: 0,
            max_cycles: 3,
            current_role: AgentRole::Developer,
            rate_limit_continuation_prompt: None,
        }
    }

    pub fn with_agents(
        mut self,
        agents: Vec<String>,
        models_per_agent: Vec<Vec<String>>,
        role: AgentRole,
    ) -> Self {
        self.agents = agents;
        self.models_per_agent = models_per_agent;
        self.current_role = role;
        self
    }

    /// Builder method to set the maximum number of retry cycles.
    ///
    /// A retry cycle is when all agents have been exhausted and we start
    /// over with exponential backoff.
    pub fn with_max_cycles(mut self, max_cycles: u32) -> Self {
        self.max_cycles = max_cycles;
        self
    }

    pub fn current_agent(&self) -> Option<&String> {
        self.agents.get(self.current_agent_index)
    }

    /// Get the currently selected model for the current agent.
    ///
    /// Returns `None` if:
    /// - No models are configured
    /// - The current agent index is out of bounds
    /// - The current model index is out of bounds
    pub fn current_model(&self) -> Option<&String> {
        self.models_per_agent
            .get(self.current_agent_index)
            .and_then(|models| models.get(self.current_model_index))
    }

    pub fn is_exhausted(&self) -> bool {
        self.retry_cycle >= self.max_cycles
            && self.current_agent_index == 0
            && self.current_model_index == 0
    }

    pub fn advance_to_next_model(&self) -> Self {
        let mut new = self.clone();
        if let Some(models) = new.models_per_agent.get(new.current_agent_index) {
            if new.current_model_index + 1 < models.len() {
                new.current_model_index += 1;
            } else {
                new.current_model_index = 0;
            }
        }
        new
    }

    pub fn switch_to_next_agent(&self) -> Self {
        let mut new = self.clone();
        if new.current_agent_index + 1 < new.agents.len() {
            new.current_agent_index += 1;
            new.current_model_index = 0;
        } else {
            new.current_agent_index = 0;
            new.current_model_index = 0;
            new.retry_cycle += 1;
        }
        new
    }

    /// Switch to next agent after rate limit, preserving prompt for continuation.
    ///
    /// This is used when an agent hits a 429 rate limit error. Instead of
    /// retrying with the same agent (which would likely hit rate limits again),
    /// we switch to the next agent and preserve the prompt so the new agent
    /// can continue the same work.
    pub fn switch_to_next_agent_with_prompt(&self, prompt: Option<String>) -> Self {
        // Rate-limit fallback is special: it should never retry an agent that has
        // already been rate-limited in the current chain.
        //
        // For single-agent chains (or when switching would wrap around), we
        // treat the chain as exhausted to avoid immediately re-invoking the same
        // rate-limited agent.
        if self.agents.len() <= 1 {
            let mut exhausted = self.clone();
            exhausted.current_agent_index = 0;
            exhausted.current_model_index = 0;
            exhausted.retry_cycle = exhausted.max_cycles;
            exhausted.rate_limit_continuation_prompt = None;
            return exhausted;
        }

        if self.current_agent_index + 1 >= self.agents.len() {
            let mut exhausted = self.clone();
            exhausted.current_agent_index = 0;
            exhausted.current_model_index = 0;
            exhausted.retry_cycle = exhausted.max_cycles;
            exhausted.rate_limit_continuation_prompt = None;
            return exhausted;
        }

        let mut next = self.switch_to_next_agent();
        next.rate_limit_continuation_prompt = prompt;
        next
    }

    /// Clear continuation prompt after successful execution.
    ///
    /// Called when an agent successfully completes its task, clearing any
    /// saved prompt context from previous rate-limited agents.
    pub fn clear_continuation_prompt(&self) -> Self {
        let mut new = self.clone();
        new.rate_limit_continuation_prompt = None;
        new
    }

    pub fn reset_for_role(&self, role: AgentRole) -> Self {
        let mut new = self.clone();
        new.current_role = role;
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new.retry_cycle = 0;
        new.rate_limit_continuation_prompt = None;
        new
    }

    pub fn reset(&self) -> Self {
        let mut new = self.clone();
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new.rate_limit_continuation_prompt = None;
        new
    }

    pub fn start_retry_cycle(&self) -> Self {
        let mut new = self.clone();
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new.retry_cycle += 1;
        new
    }
}

/// Rebase operation state.
///
/// Tracks rebase progress through the state machine:
/// NotStarted → InProgress → Conflicted → Completed/Skipped
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RebaseState {
    NotStarted,
    InProgress {
        original_head: String,
        target_branch: String,
    },
    Conflicted {
        original_head: String,
        target_branch: String,
        files: Vec<PathBuf>,
        resolution_attempts: u32,
    },
    Completed {
        new_head: String,
    },
    Skipped,
}

impl RebaseState {
    #[doc(hidden)]
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_terminal(&self) -> bool {
        matches!(self, RebaseState::Completed { .. } | RebaseState::Skipped)
    }

    pub fn current_head(&self) -> Option<String> {
        match self {
            RebaseState::NotStarted | RebaseState::Skipped => None,
            RebaseState::InProgress { original_head, .. } => Some(original_head.clone()),
            RebaseState::Conflicted { .. } => None,
            RebaseState::Completed { new_head } => Some(new_head.clone()),
        }
    }

    #[doc(hidden)]
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            RebaseState::InProgress { .. } | RebaseState::Conflicted { .. }
        )
    }
}

/// Maximum number of retry attempts when XML/format validation fails.
///
/// This applies across the pipeline for:
/// - Commit message generation validation failures
/// - Plan generation validation failures  
/// - Development output validation failures
/// - Review output validation failures
///
/// When an agent produces output that fails XML parsing or format validation,
/// we retry with corrective prompts up to this many times before giving up.
pub const MAX_VALIDATION_RETRY_ATTEMPTS: u32 = 100;

/// Maximum number of developer validation retry attempts before giving up.
///
/// Specifically for developer iterations - this is for XSD validation failures
/// (malformed XML). After exhausting these retries, the system will fall back
/// to a continuation attempt with a fresh prompt rather than failing entirely.
/// This separates XSD retry (can't parse the response) from continuation
/// (understood the response but work is incomplete).
pub const MAX_DEV_VALIDATION_RETRY_ATTEMPTS: u32 = 10;

/// Maximum number of invalid XML output reruns before aborting the iteration.
pub const MAX_DEV_INVALID_OUTPUT_RERUNS: u32 = 2;

/// Commit generation state.
///
/// Tracks commit message generation progress through retries:
/// NotStarted → Generating → Generated → Committed/Skipped
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum CommitState {
    NotStarted,
    Generating { attempt: u32, max_attempts: u32 },
    Generated { message: String },
    Committed { hash: String },
    Skipped,
}

impl CommitState {
    #[doc(hidden)]
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_terminal(&self) -> bool {
        matches!(self, CommitState::Committed { .. } | CommitState::Skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_state_initial() {
        let state = PipelineState::initial(5, 2);
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.total_iterations, 5);
        assert_eq!(state.total_reviewer_passes, 2);
        assert!(!state.is_complete());
    }

    #[test]
    fn test_agent_chain_initial() {
        let chain = AgentChainState::initial();
        assert!(chain.agents.is_empty());
        assert_eq!(chain.current_agent_index, 0);
        assert_eq!(chain.retry_cycle, 0);
    }

    #[test]
    fn test_agent_chain_with_agents() {
        let chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string(), "codex".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);

        assert_eq!(chain.agents.len(), 2);
        assert_eq!(chain.current_agent(), Some(&"claude".to_string()));
        assert_eq!(chain.max_cycles, 3);
    }

    #[test]
    fn test_agent_chain_advance_to_next_model() {
        let chain = AgentChainState::initial().with_agents(
            vec!["claude".to_string()],
            vec![vec!["model1".to_string(), "model2".to_string()]],
            AgentRole::Developer,
        );

        let new_chain = chain.advance_to_next_model();
        assert_eq!(new_chain.current_model_index, 1);
        assert_eq!(new_chain.current_model(), Some(&"model2".to_string()));
    }

    #[test]
    fn test_agent_chain_switch_to_next_agent() {
        let chain = AgentChainState::initial().with_agents(
            vec!["claude".to_string(), "codex".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        let new_chain = chain.switch_to_next_agent();
        assert_eq!(new_chain.current_agent_index, 1);
        assert_eq!(new_chain.current_agent(), Some(&"codex".to_string()));
        assert_eq!(new_chain.retry_cycle, 0);
    }

    #[test]
    fn test_agent_chain_exhausted() {
        let chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);

        let chain = chain.start_retry_cycle();
        let chain = chain.start_retry_cycle();
        let chain = chain.start_retry_cycle();

        assert!(chain.is_exhausted());
    }

    #[test]
    fn test_rebase_state_not_started() {
        let state = RebaseState::NotStarted;
        assert!(!state.is_terminal());
        assert!(state.current_head().is_none());
        assert!(!state.is_in_progress());
    }

    #[test]
    fn test_rebase_state_in_progress() {
        let state = RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
        };
        assert!(!state.is_terminal());
        assert_eq!(state.current_head(), Some("abc123".to_string()));
        assert!(state.is_in_progress());
    }

    #[test]
    fn test_rebase_state_completed() {
        let state = RebaseState::Completed {
            new_head: "def456".to_string(),
        };
        assert!(state.is_terminal());
        assert_eq!(state.current_head(), Some("def456".to_string()));
        assert!(!state.is_in_progress());
    }

    #[test]
    fn test_commit_state_not_started() {
        let state = CommitState::NotStarted;
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_commit_state_generating() {
        let state = CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        };
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_commit_state_committed() {
        let state = CommitState::Committed {
            hash: "abc123".to_string(),
        };
        assert!(state.is_terminal());
    }

    #[test]
    fn test_is_complete_during_finalizing() {
        // Finalizing phase should NOT be complete - event loop must continue
        // to execute the RestorePromptPermissions effect
        let state = PipelineState {
            phase: PipelinePhase::Finalizing,
            ..PipelineState::initial(5, 2)
        };
        assert!(
            !state.is_complete(),
            "Finalizing phase should not be complete - event loop must continue"
        );
    }

    #[test]
    fn test_is_complete_after_finalization() {
        // Complete phase IS complete
        let state = PipelineState {
            phase: PipelinePhase::Complete,
            ..PipelineState::initial(5, 2)
        };
        assert!(state.is_complete(), "Complete phase should be complete");
    }

    // =========================================================================
    // Continuation state tests
    // =========================================================================

    #[test]
    fn test_continuation_state_initial() {
        let state = ContinuationState::new();
        assert!(!state.is_continuation());
        assert_eq!(state.continuation_attempt, 0);
        assert!(state.previous_status.is_none());
        assert!(state.previous_summary.is_none());
        assert!(state.previous_files_changed.is_none());
        assert!(state.previous_next_steps.is_none());
    }

    #[test]
    fn test_continuation_state_default() {
        let state = ContinuationState::default();
        assert!(!state.is_continuation());
        assert_eq!(state.continuation_attempt, 0);
    }

    #[test]
    fn test_continuation_trigger_partial() {
        let state = ContinuationState::new();
        let new_state = state.trigger_continuation(
            DevelopmentStatus::Partial,
            "Did some work".to_string(),
            Some(vec!["file1.rs".to_string()]),
            Some("Continue with tests".to_string()),
        );

        assert!(new_state.is_continuation());
        assert_eq!(new_state.continuation_attempt, 1);
        assert_eq!(new_state.previous_status, Some(DevelopmentStatus::Partial));
        assert_eq!(
            new_state.previous_summary,
            Some("Did some work".to_string())
        );
        assert_eq!(
            new_state.previous_files_changed,
            Some(vec!["file1.rs".to_string()])
        );
        assert_eq!(
            new_state.previous_next_steps,
            Some("Continue with tests".to_string())
        );
    }

    #[test]
    fn test_continuation_trigger_failed() {
        let state = ContinuationState::new();
        let new_state = state.trigger_continuation(
            DevelopmentStatus::Failed,
            "Build failed".to_string(),
            None,
            Some("Fix errors".to_string()),
        );

        assert!(new_state.is_continuation());
        assert_eq!(new_state.continuation_attempt, 1);
        assert_eq!(new_state.previous_status, Some(DevelopmentStatus::Failed));
        assert_eq!(new_state.previous_summary, Some("Build failed".to_string()));
        assert!(new_state.previous_files_changed.is_none());
        assert_eq!(
            new_state.previous_next_steps,
            Some("Fix errors".to_string())
        );
    }

    #[test]
    fn test_continuation_reset() {
        let state = ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        );

        let reset = state.reset();
        assert!(!reset.is_continuation());
        assert_eq!(reset.continuation_attempt, 0);
        assert!(reset.previous_status.is_none());
        assert!(reset.previous_summary.is_none());
    }

    #[test]
    fn test_multiple_continuations() {
        let state = ContinuationState::new()
            .trigger_continuation(
                DevelopmentStatus::Partial,
                "First".to_string(),
                Some(vec!["a.rs".to_string()]),
                None,
            )
            .trigger_continuation(
                DevelopmentStatus::Partial,
                "Second".to_string(),
                Some(vec!["b.rs".to_string()]),
                Some("Do more".to_string()),
            );

        assert_eq!(state.continuation_attempt, 2);
        assert_eq!(state.previous_summary, Some("Second".to_string()));
        assert_eq!(state.previous_files_changed, Some(vec!["b.rs".to_string()]));
        assert_eq!(state.previous_next_steps, Some("Do more".to_string()));
    }

    #[test]
    fn test_development_status_display() {
        assert_eq!(format!("{}", DevelopmentStatus::Completed), "completed");
        assert_eq!(format!("{}", DevelopmentStatus::Partial), "partial");
        assert_eq!(format!("{}", DevelopmentStatus::Failed), "failed");
    }

    #[test]
    fn test_pipeline_state_initial_has_empty_continuation() {
        let state = PipelineState::initial(5, 2);
        assert!(!state.continuation.is_continuation());
        assert_eq!(state.continuation.continuation_attempt, 0);
    }

    #[test]
    fn test_agent_chain_reset_clears_rate_limit_continuation_prompt() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        chain.rate_limit_continuation_prompt = Some("saved".to_string());

        let reset = chain.reset();
        assert!(
            reset.rate_limit_continuation_prompt.is_none(),
            "reset() should clear rate_limit_continuation_prompt"
        );
    }

    #[test]
    fn test_agent_chain_reset_for_role_clears_rate_limit_continuation_prompt() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        chain.rate_limit_continuation_prompt = Some("saved".to_string());

        let reset = chain.reset_for_role(AgentRole::Reviewer);
        assert!(
            reset.rate_limit_continuation_prompt.is_none(),
            "reset_for_role() should clear rate_limit_continuation_prompt"
        );
    }

    #[test]
    fn test_switch_to_next_agent_with_prompt_exhausts_when_single_agent() {
        let chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        let next = chain.switch_to_next_agent_with_prompt(Some("prompt".to_string()));
        assert!(
            next.is_exhausted(),
            "single-agent rate limit fallback should exhaust the chain"
        );
    }

    #[test]
    fn test_switch_to_next_agent_with_prompt_exhausts_on_wraparound() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        chain.current_agent_index = 1;

        let next = chain.switch_to_next_agent_with_prompt(Some("prompt".to_string()));
        assert!(
            next.is_exhausted(),
            "rate limit fallback should not wrap and retry a previously rate-limited agent"
        );
    }
}
