// Pipeline state and outcome types.
//
// Contains PipelineState, validated outcomes, and checkpoint conversion.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewValidatedOutcome {
    pub pass: u32,
    pub issues_found: bool,
    pub clean_no_issues: bool,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default)]
    pub no_issues_found: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanningValidatedOutcome {
    pub iteration: u32,
    pub valid: bool,
    #[serde(default)]
    pub markdown: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevelopmentValidatedOutcome {
    pub iteration: u32,
    pub status: DevelopmentStatus,
    pub summary: String,
    pub files_changed: Option<Vec<String>>,
    pub next_steps: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FixValidatedOutcome {
    pub pass: u32,
    pub status: FixStatus,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitValidatedOutcome {
    pub attempt: u32,
    pub message: Option<String>,
    pub reason: Option<String>,
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
    pub execution_history: Vec<ExecutionStep>,
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
}

impl PipelineState {
    pub fn initial(developer_iters: u32, reviewer_reviews: u32) -> Self {
        Self::initial_with_continuation(developer_iters, reviewer_reviews, ContinuationState::new())
    }

    /// Create initial state with custom continuation limits from config.
    ///
    /// Use this when you need to load XSD retry and continuation limits from unified config.
    /// Example:
    /// ```ignore
    /// let continuation = ContinuationState::with_limits(
    ///     config.general.max_xsd_retries,
    ///     config.general.max_dev_continuations,
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
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            context_cleaned: false,
            agent_chain: AgentChainState::initial(),
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            execution_history: Vec::new(),
            checkpoint_saved_count: 0,
            continuation,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.phase, PipelinePhase::Complete)
            || (matches!(self.phase, PipelinePhase::Interrupted) && self.checkpoint_saved_count > 0)
    }

    pub fn current_head(&self) -> String {
        self.rebase
            .current_head()
            .unwrap_or_else(|| "HEAD".to_string())
    }
}

impl From<PipelineCheckpoint> for PipelineState {
    fn from(checkpoint: PipelineCheckpoint) -> Self {
        let rebase_state = map_checkpoint_rebase_state(&checkpoint.rebase_state);
        let agent_chain = AgentChainState::initial();

        PipelineState {
            phase: map_checkpoint_phase(checkpoint.phase),
            previous_phase: None,
            iteration: checkpoint.iteration,
            total_iterations: checkpoint.total_iterations,
            reviewer_pass: checkpoint.reviewer_pass,
            total_reviewer_passes: checkpoint.total_reviewer_passes,
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
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            context_cleaned: false,
            agent_chain,
            rebase: rebase_state,
            commit: CommitState::NotStarted,
            execution_history: checkpoint
                .execution_history
                .map(|h| h.steps)
                .unwrap_or_default(),
            checkpoint_saved_count: 0,
            continuation: ContinuationState::new(),
        }
    }
}

fn map_checkpoint_phase(phase: CheckpointPhase) -> PipelinePhase {
    match phase {
        CheckpointPhase::Rebase => PipelinePhase::Planning,
        CheckpointPhase::Planning => PipelinePhase::Planning,
        CheckpointPhase::Development => PipelinePhase::Development,
        CheckpointPhase::Review => PipelinePhase::Review,
        CheckpointPhase::CommitMessage => PipelinePhase::CommitMessage,
        CheckpointPhase::FinalValidation => PipelinePhase::FinalValidation,
        CheckpointPhase::Complete => PipelinePhase::Complete,
        CheckpointPhase::PreRebase => PipelinePhase::Planning,
        CheckpointPhase::PreRebaseConflict => PipelinePhase::Planning,
        CheckpointPhase::PostRebase => PipelinePhase::CommitMessage,
        CheckpointPhase::PostRebaseConflict => PipelinePhase::CommitMessage,
        CheckpointPhase::Interrupted => PipelinePhase::Interrupted,
    }
}

fn map_checkpoint_rebase_state(rebase_state: &CheckpointRebaseState) -> RebaseState {
    match rebase_state {
        CheckpointRebaseState::NotStarted => RebaseState::NotStarted,
        CheckpointRebaseState::PreRebaseInProgress { upstream_branch } => RebaseState::InProgress {
            original_head: "HEAD".to_string(),
            target_branch: upstream_branch.clone(),
        },
        CheckpointRebaseState::PreRebaseCompleted { commit_oid } => RebaseState::Completed {
            new_head: commit_oid.clone(),
        },
        CheckpointRebaseState::PostRebaseInProgress { upstream_branch } => {
            RebaseState::InProgress {
                original_head: "HEAD".to_string(),
                target_branch: upstream_branch.clone(),
            }
        }
        CheckpointRebaseState::PostRebaseCompleted { commit_oid } => RebaseState::Completed {
            new_head: commit_oid.clone(),
        },
        CheckpointRebaseState::HasConflicts { files } => RebaseState::Conflicted {
            original_head: "HEAD".to_string(),
            target_branch: "main".to_string(),
            files: files.iter().map(PathBuf::from).collect(),
            resolution_attempts: 0,
        },
        CheckpointRebaseState::Failed { .. } => RebaseState::Skipped,
    }
}
