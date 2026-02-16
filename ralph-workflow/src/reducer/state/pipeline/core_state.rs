// Core PipelineState struct and initialization.
//
// This is the checkpoint payload - the single source of truth for pipeline progress.
// All state fields are immutable from the reducer's perspective. State transitions
// occur exclusively through the reduce function.

/// Execution step history with bounded insertion.
///
/// This newtype enforces that callers cannot mutate the underlying `VecDeque` directly
/// (e.g., via `push_back`) and must instead use a bounded API.
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(transparent)]
pub struct BoundedExecutionHistory(std::collections::VecDeque<ExecutionStep>);

impl BoundedExecutionHistory {
    pub fn new() -> Self {
        Self(std::collections::VecDeque::new())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, ExecutionStep> {
        self.0.iter()
    }

    pub fn as_deque(&self) -> &std::collections::VecDeque<ExecutionStep> {
        &self.0
    }

    pub(crate) fn push_bounded(&mut self, step: ExecutionStep, limit: usize) {
        self.0.push_back(step);
        while self.0.len() > limit {
            self.0.pop_front();
        }
    }

    pub(crate) fn replace_bounded(
        &mut self,
        history: std::collections::VecDeque<ExecutionStep>,
        limit: usize,
    ) {
        self.0 = history;
        while self.0.len() > limit {
            self.0.pop_front();
        }
    }
}

impl std::ops::Deref for BoundedExecutionHistory {
    type Target = std::collections::VecDeque<ExecutionStep>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for &'a BoundedExecutionHistory {
    type Item = &'a ExecutionStep;
    type IntoIter = std::collections::vec_deque::Iter<'a, ExecutionStep>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Immutable pipeline state - the single source of truth for pipeline progress.
///
/// This struct captures complete execution context and doubles as the checkpoint
/// data structure for resume functionality. Serialize it to JSON to save state;
/// deserialize to resume interrupted runs.
///
/// # Invariants
///
/// - `iteration` is always `<= total_iterations`
/// - `reviewer_pass` is always `<= total_reviewer_passes`
/// - `agent_chain` maintains fallback order and retry counts
/// - State transitions only occur through the [`reduce`](super::reduce) function
///
/// # See Also
///
/// - [`reduce`](super::reduce) for state transitions
/// - [`determine_next_effect`](super::determine_next_effect) for effect derivation
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PipelineState {
    pub phase: PipelinePhase,
    pub previous_phase: Option<PipelinePhase>,
    pub iteration: u32,
    pub total_iterations: u32,
    pub reviewer_pass: u32,
    pub total_reviewer_passes: u32,
    pub review_issues_found: bool,
    /// Tracks whether the planning prompt was prepared for the current iteration.
    #[serde(default)]
    pub planning_prompt_prepared_iteration: Option<u32>,
    /// Tracks whether `.agent/tmp/plan.xml` was cleaned for the current iteration.
    #[serde(default)]
    pub planning_xml_cleaned_iteration: Option<u32>,
    /// Tracks whether the planning agent was invoked for the current iteration.
    #[serde(default)]
    pub planning_agent_invoked_iteration: Option<u32>,
    /// Tracks whether `.agent/tmp/plan.xml` was successfully extracted for the iteration.
    #[serde(default)]
    pub planning_xml_extracted_iteration: Option<u32>,
    /// Stores the validated outcome for the current planning iteration.
    #[serde(default)]
    pub planning_validated_outcome: Option<PlanningValidatedOutcome>,
    /// Tracks whether PLAN.md has been written for the current iteration.
    #[serde(default)]
    pub planning_markdown_written_iteration: Option<u32>,
    /// Tracks whether `.agent/tmp/plan.xml` was archived for the current iteration.
    #[serde(default)]
    pub planning_xml_archived_iteration: Option<u32>,
    /// Tracks whether development context was prepared for the current iteration.
    ///
    /// Used to sequence single-task development effects.
    #[serde(default)]
    pub development_context_prepared_iteration: Option<u32>,
    /// Tracks whether the development prompt was prepared for the current iteration.
    #[serde(default)]
    pub development_prompt_prepared_iteration: Option<u32>,
    /// Tracks whether `.agent/tmp/development_result.xml` was cleaned for the current iteration.
    #[serde(default)]
    pub development_xml_cleaned_iteration: Option<u32>,
    /// Tracks whether the developer agent was invoked for the current iteration.
    #[serde(default)]
    pub development_agent_invoked_iteration: Option<u32>,
    /// Tracks whether the analysis agent was invoked for the current iteration.
    ///
    /// Analysis agent runs after EVERY development iteration to produce
    /// an objective assessment of progress by comparing git diff against PLAN.md.
    /// This ensures continuous verification throughout the development phase.
    #[serde(default)]
    pub analysis_agent_invoked_iteration: Option<u32>,
    /// Tracks whether `.agent/tmp/development_result.xml` was extracted for the current iteration.
    #[serde(default)]
    pub development_xml_extracted_iteration: Option<u32>,
    /// Stores the validated development outcome for the current iteration.
    #[serde(default)]
    pub development_validated_outcome: Option<DevelopmentValidatedOutcome>,
    /// Tracks whether the development XML was archived for the current iteration.
    #[serde(default)]
    pub development_xml_archived_iteration: Option<u32>,
    /// Tracks whether review context was prepared for the current pass.
    ///
    /// Used to sequence single-task review effects (PrepareReviewContext -> ...).
    #[serde(default)]
    pub review_context_prepared_pass: Option<u32>,
    /// Tracks whether the review prompt was prepared for the current pass.
    #[serde(default)]
    pub review_prompt_prepared_pass: Option<u32>,
    /// Tracks whether `.agent/tmp/issues.xml` was cleaned for the current pass.
    #[serde(default)]
    pub review_issues_xml_cleaned_pass: Option<u32>,
    /// Tracks whether the reviewer agent was invoked for the current pass.
    #[serde(default)]
    pub review_agent_invoked_pass: Option<u32>,
    /// Tracks whether `.agent/tmp/issues.xml` was successfully extracted for the current pass.
    #[serde(default)]
    pub review_issues_xml_extracted_pass: Option<u32>,
    /// Stores the validated outcome for the current review pass.
    ///
    /// This is used to sequence post-validation single-task effects (write markdown,
    /// archive XML) before the reducer advances to the next pass/phase.
    #[serde(default)]
    pub review_validated_outcome: Option<ReviewValidatedOutcome>,
    /// Tracks whether ISSUES.md has been written for the current pass.
    #[serde(default)]
    pub review_issues_markdown_written_pass: Option<u32>,
    /// Tracks whether review issue snippets were extracted for the current pass.
    #[serde(default)]
    pub review_issue_snippets_extracted_pass: Option<u32>,
    #[serde(default)]
    pub review_issues_xml_archived_pass: Option<u32>,

    #[serde(default)]
    pub fix_prompt_prepared_pass: Option<u32>,

    #[serde(default)]
    pub fix_result_xml_cleaned_pass: Option<u32>,

    #[serde(default)]
    pub fix_agent_invoked_pass: Option<u32>,

    #[serde(default)]
    pub fix_result_xml_extracted_pass: Option<u32>,

    #[serde(default)]
    pub fix_validated_outcome: Option<FixValidatedOutcome>,

    #[serde(default)]
    pub fix_result_xml_archived_pass: Option<u32>,
    /// Tracks whether the commit prompt was prepared for the current commit attempt.
    #[serde(default)]
    pub commit_prompt_prepared: bool,
    /// Tracks whether the commit diff has been computed for the current attempt.
    #[serde(default)]
    pub commit_diff_prepared: bool,
    /// Tracks whether the computed commit diff was empty.
    #[serde(default)]
    pub commit_diff_empty: bool,
    /// Content identifier (sha256 hex) of the prepared commit diff.
    ///
    /// This is recorded when the diff is prepared and is used by orchestration guards
    /// to avoid reusing stale materialized prompt inputs across checkpoint resumes or
    /// when tmp artifacts change.
    #[serde(default)]
    pub commit_diff_content_id_sha256: Option<String>,
    /// Tracks whether the commit agent was invoked for the current commit attempt.
    #[serde(default)]
    pub commit_agent_invoked: bool,
    /// Tracks whether `.agent/tmp/commit_message.xml` was cleaned for the current attempt.
    #[serde(default)]
    pub commit_xml_cleaned: bool,
    /// Tracks whether `.agent/tmp/commit_message.xml` was extracted for the current attempt.
    #[serde(default)]
    pub commit_xml_extracted: bool,
    /// Stores the validated commit outcome for the current attempt.
    #[serde(default)]
    pub commit_validated_outcome: Option<CommitValidatedOutcome>,
    /// Tracks whether commit XML has been archived for the current attempt.
    #[serde(default)]
    pub commit_xml_archived: bool,
    pub context_cleaned: bool,
    pub agent_chain: AgentChainState,
    pub rebase: RebaseState,
    pub commit: CommitState,
    #[serde(default)]
    pub execution_history: BoundedExecutionHistory,
    /// Count of CheckpointSaved events applied to state.
    ///
    /// This is a reducer-visible record of checkpoint saves, intended for
    /// observability and tests that enforce checkpointing happens via effects.
    #[serde(default)]
    pub checkpoint_saved_count: u32,
    /// Continuation state for development iterations.
    ///
    /// Tracks context from previous attempts when status is "partial" or "failed"
    /// to enable continuation-aware prompting.
    #[serde(default)]
    pub continuation: ContinuationState,

    /// Run-level execution metrics.
    ///
    /// This is the single source of truth for all iteration/attempt/retry/fallback
    /// statistics. Updated deterministically by the reducer based on events.
    #[serde(default)]
    pub metrics: RunMetrics,

    /// Whether TriggerDevFixFlow has been executed in the current AwaitingDevFix phase.
    ///
    /// This flag is set to true when DevFixTriggered event is reduced.
    /// It ensures the event loop allows at least one iteration to execute
    /// TriggerDevFixFlow before checking completion, preventing premature
    /// exit when max iterations is imminent.
    #[serde(default)]
    pub dev_fix_triggered: bool,

    /// Count of dev-fix recovery attempts for current failure.
    ///
    /// Tracks how many times we've attempted to recover from the same failure.
    /// Reset when recovery succeeds or when moving to a different failure context.
    /// Used to determine recovery escalation level:
    /// - Attempts 1-3: Retry same operation (Level 1)
    /// - Attempts 4-6: Reset to phase start (Level 2)
    /// - Attempts 7-9: Reset iteration counter (Level 3)
    /// - Attempts 10+: Reset to iteration 0 (Level 4)
    #[serde(default)]
    pub dev_fix_attempt_count: u32,

    /// Current recovery escalation level.
    ///
    /// Tracks which recovery strategy is being applied:
    /// - 0: No recovery in progress
    /// - 1: Retry same operation (attempts 1-3)
    /// - 2: Reset to phase start (attempts 4-6)
    /// - 3: Reset iteration counter (attempts 7-9)
    /// - 4: Reset to iteration 0 (attempts 10+)
    #[serde(default)]
    pub recovery_escalation_level: u32,

    /// Snapshot of the phase where the current failure occurred.
    ///
    /// Preserved when transitioning to AwaitingDevFix so we know which phase
    /// to return to after dev-fix completes. Set when entering AwaitingDevFix,
    /// cleared when recovery succeeds or when reaching terminal state.
    #[serde(default)]
    pub failed_phase_for_recovery: Option<PipelinePhase>,

    /// Whether the pipeline should (re)attempt emitting a completion marker.
    ///
    /// This is reserved for explicit termination paths (safety valve / catastrophic
    /// external termination), not attempt-count based recovery escalation.
    ///
    /// When true, orchestration must derive `Effect::EmitCompletionMarkerAndTerminate`
    /// until it succeeds (CompletionMarkerEmitted) so external orchestration can
    /// reliably observe termination.
    #[serde(default)]
    pub completion_marker_pending: bool,

    /// Whether the pending completion marker represents a failure.
    #[serde(default)]
    pub completion_marker_is_failure: bool,

    /// Optional reason to include in the completion marker for failures.
    #[serde(default)]
    pub completion_marker_reason: Option<String>,

    /// Whether gitignore entries have been ensured for this pipeline run.
    ///
    /// Set to true after Effect::EnsureGitignoreEntries completes successfully.
    /// This prevents re-running the effect on every orchestration cycle.
    #[serde(default)]
    pub gitignore_entries_ensured: bool,

    /// Canonical, reducer-visible prompt inputs after oversize materialization.
    ///
    /// This is the single source of truth for any inline-vs-reference and
    /// model-budget truncation decisions. Effects must not silently re-truncate
    /// or re-reference content on retries; instead, they should consume these
    /// materialized inputs (or materialize them exactly once per content id).
    #[serde(default)]
    pub prompt_inputs: PromptInputsState,

    /// PROMPT.md permission lifecycle state.
    ///
    /// Tracks best-effort read-only protection during execution and restoration
    /// on all graceful termination paths (success and failure).
    #[serde(default)]
    pub prompt_permissions: PromptPermissionsState,

    /// Last template substitution log for validation and observability.
    ///
    /// Updated when TemplateRendered event is reduced. Used by the reducer
    /// to validate templates based on tracked substitutions rather than
    /// regex scanning rendered output.
    #[serde(default)]
    pub last_substitution_log: Option<crate::prompts::SubstitutionLog>,

    /// Whether the last template validation failed based on the substitution log.
    #[serde(default)]
    pub template_validation_failed: bool,

    /// Unsubstituted placeholders from the last rendered template.
    #[serde(default)]
    pub template_validation_unsubstituted: Vec<String>,

    /// True if pipeline was interrupted by user signal (Ctrl+C).
    /// This is the ONLY case where pre-termination commit safety check is skipped.
    /// All other termination paths (AwaitingDevFix exhaustion, programmatic interrupts, etc.)
    /// must commit before terminating.
    #[serde(default)]
    pub interrupted_by_user: bool,

    /// When set, the pipeline has detected uncommitted changes during the
    /// pre-termination safety check and routed back through the commit phase.
    ///
    /// After the commit is created (or explicitly skipped), the reducer must
    /// return to this phase and allow termination to proceed.
    #[serde(default)]
    pub termination_resume_phase: Option<PipelinePhase>,

    /// True if pre-termination commit safety check has been performed.
    /// Prevents infinite loops when checking for uncommitted changes before Complete/Interrupted.
    #[serde(default)]
    pub pre_termination_commit_checked: bool,

    // ========================================================================
    // Cloud Mode State Fields (INTERNAL USE ONLY)
    // ========================================================================
    //
    // These fields are only populated when cloud mode is enabled (internal env-config).
    // In CLI mode, they remain in their default (None/false) state and are not used.
    //
    // Cloud mode is environment-variable only and not exposed to users.
    /// Cloud configuration (redacted) for pure orchestration.
    ///
    /// This is a checkpoint-safe view (no secrets) derived from runtime cloud config.
    /// When enabled=false, all cloud-specific effects are skipped.
    #[serde(default)]
    pub cloud_config: crate::config::CloudStateConfig,

    /// Commit SHA pending push (cloud mode only, None in CLI mode).
    ///
    /// Set when CommitCreated event is reduced in cloud mode.
    /// Cleared when CommitEvent::PushCompleted is reduced.
    /// Used by orchestration to emit PushToRemote effects.
    #[serde(default)]
    pub pending_push_commit: Option<String>,

    /// Whether git auth has been configured this run (cloud mode only).
    ///
    /// Set to true when CommitEvent::GitAuthConfigured is reduced.
    /// Used to avoid re-configuring authentication on every push.
    #[serde(default)]
    pub git_auth_configured: bool,

    /// Whether PR has been created (cloud mode only).
    ///
    /// Set to true when CommitEvent::PullRequestCreated is reduced.
    /// Prevents duplicate PR creation attempts.
    #[serde(default)]
    pub pr_created: bool,

    /// URL of created PR (cloud mode only).
    ///
    /// Populated when CommitEvent::PullRequestCreated is reduced.
    /// Used for reporting and observability.
    #[serde(default)]
    pub pr_url: Option<String>,

    /// Count of successful push operations (cloud mode only).
    ///
    /// Incremented when CommitEvent::PushCompleted is reduced.
    /// Used for metrics and observability.
    #[serde(default)]
    pub push_count: u32,

    /// Consecutive push failure count for the current pending commit.
    ///
    /// Reset on CommitEvent::PushCompleted or when the pending push is cleared.
    #[serde(default)]
    pub push_retry_count: u32,

    /// Last push error message (cloud mode only).
    ///
    /// Used for completion reporting and observability. Must not contain secrets.
    #[serde(default)]
    pub last_push_error: Option<String>,

    /// Commits that failed to push after exhausting retries.
    ///
    /// This is used for completion reporting so failures are not silent.
    #[serde(default)]
    pub unpushed_commits: Vec<String>,

    /// SHA of the last successfully pushed commit (cloud mode only).
    ///
    /// Updated when CommitEvent::PushCompleted is reduced.
    /// Used for observability and debugging.
    #[serde(default)]
    pub last_pushed_commit: Option<String>,

    /// PR number for the created pull request (cloud mode only).
    ///
    /// Populated when CommitEvent::PullRequestCreated is reduced.
    /// Used for reporting and observability.
    #[serde(default)]
    pub pr_number: Option<u32>,
}

impl PipelineState {
    pub fn execution_history(&self) -> &std::collections::VecDeque<ExecutionStep> {
        self.execution_history.as_deque()
    }

    pub fn execution_history_len(&self) -> usize {
        self.execution_history.len()
    }

    pub fn initial(developer_iters: u32, reviewer_reviews: u32) -> Self {
        Self::initial_with_continuation(developer_iters, reviewer_reviews, ContinuationState::new())
    }

    /// Create initial state with custom continuation limits from config.
    ///
    /// Use this when you need to load XSD retry and continuation limits from unified config.
    /// Example:
    /// ```ignore
    /// // Config semantics: max_dev_continuations counts continuation attempts *beyond*
    /// // the initial attempt. ContinuationState::max_continue_count semantics are
    /// // "maximum total attempts including initial".
    /// let continuation = ContinuationState::with_limits(
    ///     config.general.max_xsd_retries,
    ///     1 + config.general.max_dev_continuations,
    ///     config.general.max_same_agent_retries,
    /// );
    /// let state = PipelineState::initial_with_continuation(dev_iters, reviews, continuation);
    /// ```
    pub fn initial_with_continuation(
        developer_iters: u32,
        reviewer_reviews: u32,
        continuation: ContinuationState,
    ) -> Self {
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
            planning_prompt_prepared_iteration: None,
            planning_xml_cleaned_iteration: None,
            planning_agent_invoked_iteration: None,
            planning_xml_extracted_iteration: None,
            planning_validated_outcome: None,
            planning_markdown_written_iteration: None,
            planning_xml_archived_iteration: None,
            development_context_prepared_iteration: None,
            development_prompt_prepared_iteration: None,
            development_xml_cleaned_iteration: None,
            development_agent_invoked_iteration: None,
            analysis_agent_invoked_iteration: None,
            development_xml_extracted_iteration: None,
            development_validated_outcome: None,
            development_xml_archived_iteration: None,
            review_context_prepared_pass: None,
            review_prompt_prepared_pass: None,
            review_issues_xml_cleaned_pass: None,
            review_agent_invoked_pass: None,
            review_issues_xml_extracted_pass: None,
            review_validated_outcome: None,
            review_issues_markdown_written_pass: None,
            review_issue_snippets_extracted_pass: None,
            review_issues_xml_archived_pass: None,
            fix_prompt_prepared_pass: None,
            fix_result_xml_cleaned_pass: None,
            fix_agent_invoked_pass: None,
            fix_result_xml_extracted_pass: None,
            fix_validated_outcome: None,
            fix_result_xml_archived_pass: None,
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_diff_content_id_sha256: None,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            context_cleaned: false,
            agent_chain: AgentChainState::initial(),
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            execution_history: BoundedExecutionHistory::new(),
            checkpoint_saved_count: 0,
            continuation: continuation.clone(),
            dev_fix_triggered: false,
            dev_fix_attempt_count: 0,
            recovery_escalation_level: 0,
            failed_phase_for_recovery: None,
            completion_marker_pending: false,
            completion_marker_is_failure: false,
            completion_marker_reason: None,
            gitignore_entries_ensured: false,
            prompt_inputs: PromptInputsState::default(),
            prompt_permissions: PromptPermissionsState::default(),
            last_substitution_log: None,
            template_validation_failed: false,
            template_validation_unsubstituted: Vec::new(),
            metrics: RunMetrics::new(developer_iters, reviewer_reviews, &continuation),
            interrupted_by_user: false,
            termination_resume_phase: None,
            pre_termination_commit_checked: false,
            // Cloud mode fields (all default/disabled)
            cloud_config: crate::config::CloudStateConfig::disabled(),
            pending_push_commit: None,
            git_auth_configured: false,
            pr_created: false,
            pr_url: None,
            push_count: 0,
            push_retry_count: 0,
            last_push_error: None,
            unpushed_commits: Vec::new(),
            last_pushed_commit: None,
            pr_number: None,
        }
    }

    /// Returns true if the pipeline is in a terminal state for event loop purposes.
    ///
    /// # Terminal States
    ///
    /// - **Complete phase**: Always terminal (successful completion)
    /// - **Interrupted phase**: Terminal under these conditions:
    ///   1. A checkpoint has been saved (normal Ctrl+C interruption path)
    ///   2. Transitioning from AwaitingDevFix phase (failure handling completed)
    ///
    /// # AwaitingDevFix → Interrupted Path
    ///
    /// When the pipeline terminates via completion marker emission, it transitions
    /// through AwaitingDevFix where:
    /// 1. Orchestration derives `EmitCompletionMarkerAndTerminate`
    /// 2. The handler writes the completion marker to filesystem
    /// 3. CompletionMarkerEmitted transitions the reducer state to Interrupted
    ///
    /// At this point, the completion marker has been written, signaling external
    /// orchestration that the pipeline has terminated. The SaveCheckpoint effect
    /// will execute next, but the phase is already considered terminal because
    /// the failure has been properly signaled.
    pub fn is_terminal(&self) -> bool {
        use crate::reducer::event::PipelinePhase;
        match self.phase {
            PipelinePhase::Complete => true,
            PipelinePhase::Interrupted => {
                self.checkpoint_saved_count > 0
                    || matches!(self.previous_phase, Some(PipelinePhase::AwaitingDevFix))
            }
            _ => false,
        }
    }

    /// Add an execution step to the history with automatic bounding.
    ///
    /// This method implements a ring buffer strategy: when the history exceeds
    /// the configured limit, the oldest entries are dropped to maintain a bounded
    /// memory footprint. This prevents unbounded memory growth during long-running
    /// pipelines while preserving recent execution context for debugging.
    ///
    /// # Arguments
    ///
    /// * `step` - The execution step to add
    /// * `limit` - Maximum number of entries to keep (from config)
    ///
    /// # Memory Behavior
    ///
    /// With default limit of 1000 entries:
    /// - Memory usage: ~51 KB heap (based on recorded baseline measurements)
    /// - Checkpoint size: ~375 KB serialized
    /// - Growth: Bounded (oldest entries dropped when limit reached)
    pub fn add_execution_step(&mut self, step: ExecutionStep, limit: usize) {
        self.execution_history.push_bounded(step, limit);
    }

    pub(crate) fn replace_execution_history_bounded(
        &mut self,
        history: std::collections::VecDeque<ExecutionStep>,
        limit: usize,
    ) {
        self.execution_history.replace_bounded(history, limit);
    }
}
