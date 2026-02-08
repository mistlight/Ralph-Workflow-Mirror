// Checkpoint conversion logic.
//
// Implements conversion from checkpoint format to runtime PipelineState.
// This is pure state transformation with no I/O operations.

impl From<PipelineCheckpoint> for PipelineState {
    fn from(checkpoint: PipelineCheckpoint) -> Self {
        let rebase_state = map_checkpoint_rebase_state(&checkpoint.rebase_state);
        let agent_chain = AgentChainState::initial();

        PipelineState {
            phase: map_checkpoint_phase(checkpoint.phase),
            previous_phase: None,
            // Restore iteration/pass counters from checkpoint.
            // Note: All progress flags are reset to None below.
            // Orchestration uses inclusive boundary checks:
            // `iteration < total || (iteration == total && total > 0)`
            // to ensure work is re-run at boundaries when flags are None.
            // See phase_effects.rs for the boundary logic.
            iteration: checkpoint.iteration,
            total_iterations: checkpoint.total_iterations,
            reviewer_pass: checkpoint.reviewer_pass,
            total_reviewer_passes: checkpoint.total_reviewer_passes,
            review_issues_found: false,
            // All progress flags reset to None to allow re-running current work.
            // The orchestration layer determines which step to execute based on
            // these flags combined with the iteration/pass counters.
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
            agent_chain,
            rebase: rebase_state,
            commit: CommitState::NotStarted,
            execution_history: checkpoint
                .execution_history
                .map(|h| h.steps)
                .unwrap_or_default(),
            checkpoint_saved_count: 0,
            continuation: ContinuationState::new(),
            dev_fix_triggered: false,
            gitignore_entries_ensured: false,
            prompt_inputs: checkpoint.prompt_inputs.unwrap_or_default(),
            metrics: {
                let continuation = ContinuationState::new();
                RunMetrics {
                    dev_iterations_completed: checkpoint.actual_developer_runs,
                    review_passes_completed: checkpoint.actual_reviewer_runs,
                    max_dev_iterations: checkpoint.total_iterations,
                    max_review_passes: checkpoint.total_reviewer_passes,
                    max_xsd_retry_count: continuation.max_xsd_retry_count,
                    max_dev_continuation_count: continuation.max_continue_count,
                    max_fix_continuation_count: continuation.max_fix_continue_count,
                    max_same_agent_retry_count: continuation.max_same_agent_retry_count,
                    ..RunMetrics::default()
                }
            },
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
        CheckpointPhase::AwaitingDevFix => PipelinePhase::AwaitingDevFix,
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
