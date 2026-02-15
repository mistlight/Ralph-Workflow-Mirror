// Pure helper methods for PipelineState.
//
// These methods provide state queries and derived values. They contain
// no side effects and operate solely on the immutable state struct.

impl PipelineState {
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
    /// When the pipeline terminates due to exhausted recovery attempts, it transitions
    /// through AwaitingDevFix where:
    /// 1. Orchestration derives `EmitCompletionMarkerAndTerminate`
    /// 2. The handler writes the completion marker to filesystem
    /// 3. CompletionMarkerEmitted transitions the reducer state to Interrupted
    ///
    /// At this point, the completion marker has been written, signaling external
    /// orchestration that the pipeline has terminated. The SaveCheckpoint effect
    /// will execute next, but the phase is already considered terminal because
    /// the failure has been properly signaled.
    ///
    /// # Edge Cases
    ///
    /// An Interrupted phase without a checkpoint and without previous_phase context
    /// is NOT considered terminal. This can occur when resuming from a checkpoint
    /// that was interrupted mid-execution.
    ///
    /// # Non-Terminating Pipeline Architecture
    ///
    /// Internal failures are handled via the AwaitingDevFix recovery loop.
    /// Completion markers are emitted only when the pipeline is actually terminating
    /// (after recovery exhaustion or defensive max-iteration recovery).
    ///
    /// Terminal states:
    /// - `Complete`: Normal successful completion
    /// - `Interrupted` with checkpoint saved: Resumable state
    /// - `Interrupted` from `AwaitingDevFix`: Completion marker written, failure signaled
    pub fn is_complete(&self) -> bool {
        matches!(self.phase, PipelinePhase::Complete)
            || (matches!(self.phase, PipelinePhase::Interrupted)
                && (self.checkpoint_saved_count > 0
                    // CRITICAL: AwaitingDevFix→Interrupted transition means completion marker
                    // was written during EmitCompletionMarkerAndTerminate. This is terminal even without
                    // checkpoint because the failure has been properly signaled to orchestration.
                    // This prevents "Pipeline exited without completion marker" bug.
                    || matches!(self.previous_phase, Some(PipelinePhase::AwaitingDevFix))))
    }

    pub fn current_head(&self) -> String {
        self.rebase
            .current_head()
            .unwrap_or_else(|| "HEAD".to_string())
    }

    /// Clear phase-specific progress flags for the given phase.
    ///
    /// Used by Level 2 recovery (PhaseStart) to restart a phase from scratch
    /// while preserving iteration counters and other global state.
    pub(crate) fn clear_phase_flags(&self, phase: PipelinePhase) -> Self {
        match phase {
            PipelinePhase::Planning => self.clear_planning_flags(),
            PipelinePhase::Development => self.clear_development_flags(),
            PipelinePhase::Review => self.clear_review_flags(),
            PipelinePhase::CommitMessage => self.clear_commit_flags(),
            _ => self.clone(),
        }
    }

    /// Clear all Planning phase progress flags.
    fn clear_planning_flags(&self) -> Self {
        Self {
            planning_prompt_prepared_iteration: None,
            planning_xml_cleaned_iteration: None,
            planning_agent_invoked_iteration: None,
            planning_xml_extracted_iteration: None,
            planning_validated_outcome: None,
            planning_markdown_written_iteration: None,
            planning_xml_archived_iteration: None,
            ..self.clone()
        }
    }

    /// Clear all Development phase progress flags.
    fn clear_development_flags(&self) -> Self {
        Self {
            development_context_prepared_iteration: None,
            development_prompt_prepared_iteration: None,
            development_xml_cleaned_iteration: None,
            development_agent_invoked_iteration: None,
            analysis_agent_invoked_iteration: None,
            development_xml_extracted_iteration: None,
            development_validated_outcome: None,
            development_xml_archived_iteration: None,
            ..self.clone()
        }
    }

    /// Clear all Review phase progress flags.
    fn clear_review_flags(&self) -> Self {
        Self {
            review_context_prepared_pass: None,
            review_prompt_prepared_pass: None,
            review_issues_xml_cleaned_pass: None,
            review_agent_invoked_pass: None,
            review_issues_xml_extracted_pass: None,
            review_validated_outcome: None,
            review_issues_markdown_written_pass: None,
            review_issue_snippets_extracted_pass: None,
            review_issues_xml_archived_pass: None,
            review_issues_found: false,
            fix_prompt_prepared_pass: None,
            fix_result_xml_cleaned_pass: None,
            fix_agent_invoked_pass: None,
            fix_result_xml_extracted_pass: None,
            fix_validated_outcome: None,
            fix_result_xml_archived_pass: None,
            ..self.clone()
        }
    }

    /// Clear all CommitMessage phase progress flags.
    fn clear_commit_flags(&self) -> Self {
        Self {
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_diff_content_id_sha256: None,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            ..self.clone()
        }
    }

    /// Reset iteration counter (Level 3 recovery).
    ///
    /// Decrements the iteration counter (with floor at 0) and clears all
    /// phase flags to restart the current iteration from Planning phase.
    pub(crate) fn reset_iteration(&self) -> Self {
        let new_iteration = self.iteration.saturating_sub(1);
        let prompt_inputs = self
            .prompt_inputs
            .clone()
            .with_planning_cleared()
            .with_development_cleared()
            .with_commit_cleared()
            .with_review_cleared()
            .with_xsd_retry_cleared();
        Self {
            iteration: new_iteration,
            phase: PipelinePhase::Planning,
            context_cleaned: false,
            gitignore_entries_ensured: false,
            prompt_inputs,
            continuation: self.continuation.clone().reset(),
            ..self.clone()
        }
        .clear_planning_flags()
        .clear_development_flags()
        .clear_commit_flags()
    }

    /// Reset to iteration 0 (Level 4 recovery).
    ///
    /// Resets iteration counter to 0 and clears all phase flags for a
    /// complete restart from the beginning of the pipeline.
    pub(crate) fn reset_to_iteration_zero(&self) -> Self {
        let prompt_inputs = self
            .prompt_inputs
            .clone()
            .with_planning_cleared()
            .with_development_cleared()
            .with_commit_cleared()
            .with_review_cleared()
            .with_xsd_retry_cleared();
        Self {
            iteration: 0,
            phase: PipelinePhase::Planning,
            context_cleaned: false,
            gitignore_entries_ensured: false,
            prompt_inputs,
            continuation: self.continuation.clone().reset(),
            ..self.clone()
        }
        .clear_planning_flags()
        .clear_development_flags()
        .clear_commit_flags()
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;
    use crate::reducer::state::{
        MaterializedCommitInputs, MaterializedDevelopmentInputs, MaterializedPlanningInputs,
        MaterializedPromptInput, PromptInputKind, PromptInputRepresentation, PromptInputsState,
        PromptMaterializationReason,
    };

    fn mp(kind: PromptInputKind) -> MaterializedPromptInput {
        MaterializedPromptInput {
            kind,
            content_id_sha256: "id".to_string(),
            consumer_signature_sha256: "sig".to_string(),
            original_bytes: 1,
            final_bytes: 1,
            model_budget_bytes: None,
            inline_budget_bytes: None,
            representation: PromptInputRepresentation::Inline,
            reason: PromptMaterializationReason::WithinBudgets,
        }
    }

    #[test]
    fn reset_iteration_resets_global_phase_start_prereqs() {
        let mut state = PipelineState::initial(2, 0);
        state.iteration = 1;
        state.context_cleaned = true;
        state.gitignore_entries_ensured = true;
        state.continuation.same_agent_retry_pending = true;
        state.continuation.same_agent_retry_count = 2;

        state.prompt_inputs = PromptInputsState {
            planning: Some(MaterializedPlanningInputs {
                iteration: 1,
                prompt: mp(PromptInputKind::Prompt),
            }),
            development: Some(MaterializedDevelopmentInputs {
                iteration: 1,
                prompt: mp(PromptInputKind::Prompt),
                plan: mp(PromptInputKind::Plan),
            }),
            commit: Some(MaterializedCommitInputs {
                attempt: 1,
                diff: mp(PromptInputKind::Diff),
            }),
            review: None,
            xsd_retry_last_output: None,
        };

        let reset = state.reset_iteration();

        assert!(!reset.context_cleaned);
        assert!(!reset.gitignore_entries_ensured);
        assert!(!reset.continuation.same_agent_retry_pending);
        assert_eq!(reset.continuation.same_agent_retry_count, 0);
        assert!(reset.prompt_inputs.planning.is_none());
        assert!(reset.prompt_inputs.development.is_none());
        assert!(reset.prompt_inputs.commit.is_none());
    }

    #[test]
    fn reset_to_iteration_zero_resets_global_phase_start_prereqs() {
        let mut state = PipelineState::initial(2, 0);
        state.iteration = 2;
        state.context_cleaned = true;
        state.gitignore_entries_ensured = true;
        state.prompt_inputs.planning = Some(MaterializedPlanningInputs {
            iteration: 2,
            prompt: mp(PromptInputKind::Prompt),
        });

        let reset = state.reset_to_iteration_zero();

        assert_eq!(reset.iteration, 0);
        assert!(!reset.context_cleaned);
        assert!(!reset.gitignore_entries_ensured);
        assert!(reset.prompt_inputs.planning.is_none());
    }
}
