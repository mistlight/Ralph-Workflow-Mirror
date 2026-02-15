// Checkpoint conversion logic.
//
// Implements conversion from checkpoint format to runtime PipelineState.
// This is pure state transformation with no I/O operations.

use std::collections::VecDeque;

fn bound_execution_history_steps(
    steps: VecDeque<ExecutionStep>,
    limit: usize,
) -> VecDeque<ExecutionStep> {
    if limit == 0 {
        return VecDeque::new();
    }
    let len = steps.len();
    if len <= limit {
        return steps;
    }

    // Keep only the most recent `limit` entries while dropping the oversized
    // allocation from legacy checkpoints.
    let keep_from = len - limit;
    steps.into_iter().skip(keep_from).collect()
}

impl PipelineState {
    pub(crate) fn from_checkpoint_with_execution_history_limit(
        mut checkpoint: PipelineCheckpoint,
        execution_history_limit: usize,
    ) -> Self {
        let rebase_state = map_checkpoint_rebase_state(&checkpoint.rebase_state);
        let agent_chain = AgentChainState::initial();
        let last_substitution_log = checkpoint.last_substitution_log.clone();
        let (template_validation_failed, template_validation_unsubstituted) = last_substitution_log
            .as_ref()
            .map_or((false, Vec::new()), |log| {
                (!log.is_complete(), log.unsubstituted.clone())
            });

        let execution_history_steps = checkpoint
            .execution_history
            .take()
            .map(|h| h.steps)
            .unwrap_or_default();

        let mut state = PipelineState {
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
            execution_history: BoundedExecutionHistory::new(),
            checkpoint_saved_count: 0,
            continuation: ContinuationState::new(),
            dev_fix_triggered: false,
            dev_fix_attempt_count: checkpoint.dev_fix_attempt_count,
            recovery_escalation_level: checkpoint.recovery_escalation_level,
            failed_phase_for_recovery: checkpoint.failed_phase_for_recovery,
            completion_marker_pending: false,
            completion_marker_is_failure: false,
            completion_marker_reason: None,
            gitignore_entries_ensured: false,
            prompt_inputs: checkpoint.prompt_inputs.unwrap_or_default(),
            prompt_permissions: checkpoint.prompt_permissions,
            last_substitution_log,
            template_validation_failed,
            template_validation_unsubstituted,
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
        };

        let bounded_steps =
            bound_execution_history_steps(execution_history_steps, execution_history_limit);
        if !bounded_steps.is_empty() {
            state
                .execution_history
                .replace_bounded(bounded_steps, execution_history_limit);
        }

        state
    }
}

impl From<PipelineCheckpoint> for PipelineState {
    fn from(checkpoint: PipelineCheckpoint) -> Self {
        // `From` cannot accept configuration. Apply a conservative hard cap so
        // legacy checkpoints cannot load arbitrarily large execution history into memory.
        let limit = crate::config::Config::default().execution_history_limit;
        PipelineState::from_checkpoint_with_execution_history_limit(checkpoint, limit)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::event::PipelinePhase;
    use serde_json::Value;

    #[test]
    fn test_clear_planning_flags() {
        let mut state = PipelineState::initial(1, 0);
        state.planning_prompt_prepared_iteration = Some(1);
        state.planning_agent_invoked_iteration = Some(1);
        state.planning_xml_extracted_iteration = Some(1);
        state.planning_validated_outcome = Some(crate::reducer::state::PlanningValidatedOutcome {
            iteration: 1,
            valid: true,
            markdown: None,
        });

        let cleared = state.clear_phase_flags(PipelinePhase::Planning);

        assert_eq!(cleared.planning_prompt_prepared_iteration, None);
        assert_eq!(cleared.planning_agent_invoked_iteration, None);
        assert_eq!(cleared.planning_xml_extracted_iteration, None);
        assert_eq!(cleared.planning_validated_outcome, None);
    }

    #[test]
    fn test_clear_development_flags() {
        let mut state = PipelineState::initial(1, 0);
        state.development_context_prepared_iteration = Some(2);
        state.development_agent_invoked_iteration = Some(2);
        state.analysis_agent_invoked_iteration = Some(2);
        state.development_validated_outcome =
            Some(crate::reducer::state::DevelopmentValidatedOutcome {
                iteration: 2,
                status: crate::reducer::state::DevelopmentStatus::Completed,
                summary: "test".to_string(),
                files_changed: None,
                next_steps: None,
            });

        let cleared = state.clear_phase_flags(PipelinePhase::Development);

        assert_eq!(cleared.development_context_prepared_iteration, None);
        assert_eq!(cleared.development_agent_invoked_iteration, None);
        assert_eq!(cleared.analysis_agent_invoked_iteration, None);
        assert_eq!(cleared.development_validated_outcome, None);
    }

    #[test]
    fn test_clear_phase_flags_routes_to_correct_helper() {
        let mut state = PipelineState::initial(1, 0);
        state.planning_agent_invoked_iteration = Some(1);
        state.development_agent_invoked_iteration = Some(1);

        // Clear Planning should only affect Planning flags
        let cleared = state.clear_phase_flags(PipelinePhase::Planning);
        assert_eq!(cleared.planning_agent_invoked_iteration, None);
        assert_eq!(cleared.development_agent_invoked_iteration, Some(1));

        // Clear Development should only affect Development flags
        let cleared = state.clear_phase_flags(PipelinePhase::Development);
        assert_eq!(cleared.planning_agent_invoked_iteration, Some(1));
        assert_eq!(cleared.development_agent_invoked_iteration, None);
    }

    #[test]
    fn test_reset_iteration_decrements_counter() {
        let mut state = PipelineState::initial(5, 0);
        state.iteration = 3;
        state.planning_agent_invoked_iteration = Some(3);
        state.development_agent_invoked_iteration = Some(3);

        let reset = state.reset_iteration();

        assert_eq!(reset.iteration, 2);
        assert_eq!(reset.phase, PipelinePhase::Planning);
        assert_eq!(reset.planning_agent_invoked_iteration, None);
        assert_eq!(reset.development_agent_invoked_iteration, None);
    }

    #[test]
    fn test_reset_iteration_floor_at_zero() {
        let mut state = PipelineState::initial(1, 0);
        state.iteration = 0;

        let reset = state.reset_iteration();

        assert_eq!(reset.iteration, 0); // Floor at 0
    }

    #[test]
    fn test_reset_to_iteration_zero() {
        let mut state = PipelineState::initial(10, 0);
        state.iteration = 5;
        state.planning_agent_invoked_iteration = Some(5);
        state.development_agent_invoked_iteration = Some(5);

        let reset = state.reset_to_iteration_zero();

        assert_eq!(reset.iteration, 0);
        assert_eq!(reset.phase, PipelinePhase::Planning);
        assert_eq!(reset.planning_agent_invoked_iteration, None);
        assert_eq!(reset.development_agent_invoked_iteration, None);
    }

    #[test]
    fn test_phase_reset_preserves_unrelated_state() {
        let mut state = PipelineState::initial(10, 3);
        state.iteration = 2;
        state.reviewer_pass = 1;
        state.total_iterations = 10;
        state.planning_agent_invoked_iteration = Some(2);

        let cleared = state.clear_phase_flags(PipelinePhase::Planning);

        // Phase flags cleared
        assert_eq!(cleared.planning_agent_invoked_iteration, None);

        // Global counters preserved
        assert_eq!(cleared.iteration, 2);
        assert_eq!(cleared.reviewer_pass, 1);
        assert_eq!(cleared.total_iterations, 10);
    }

    #[test]
    fn checkpoint_resume_preserves_recovery_escalation_state() {
        use crate::checkpoint::state::{AgentConfigSnapshot, CliArgsSnapshot, RebaseState};
        use crate::checkpoint::{CheckpointBuilder, PipelinePhase as CheckpointPhase};

        let checkpoint = CheckpointBuilder::new()
            .phase(CheckpointPhase::AwaitingDevFix, 2, 5)
            .reviewer_pass(1, 2)
            .agents("dev", "rev")
            .cli_args(CliArgsSnapshot {
                developer_iters: 5,
                reviewer_reviews: 2,
                review_depth: None,
                isolation_mode: true,
                verbosity: 2,
                show_streaming_metrics: false,
                reviewer_json_parser: None,
            })
            .developer_config(AgentConfigSnapshot {
                name: "dev".to_string(),
                cmd: "dev".to_string(),
                output_flag: "-o".to_string(),
                yolo_flag: None,
                can_commit: true,
                model_override: None,
                provider_override: None,
                context_level: 1,
            })
            .reviewer_config(AgentConfigSnapshot {
                name: "rev".to_string(),
                cmd: "rev".to_string(),
                output_flag: "-o".to_string(),
                yolo_flag: None,
                can_commit: true,
                model_override: None,
                provider_override: None,
                context_level: 1,
            })
            .rebase_state(RebaseState::default())
            .git_identity(None, None)
            .build()
            .expect("checkpoint should build");

        let mut json: Value = serde_json::to_value(&checkpoint).expect("checkpoint to json");
        let obj = json.as_object_mut().expect("checkpoint json object");
        obj.insert("dev_fix_attempt_count".to_string(), Value::from(7));
        obj.insert("recovery_escalation_level".to_string(), Value::from(3));
        obj.insert(
            "failed_phase_for_recovery".to_string(),
            Value::String("CommitMessage".to_string()),
        );

        let checkpoint: PipelineCheckpoint =
            serde_json::from_value(json).expect("checkpoint json should deserialize");

        let state = PipelineState::from_checkpoint_with_execution_history_limit(checkpoint, 1000);

        assert_eq!(state.dev_fix_attempt_count, 7);
        assert_eq!(state.recovery_escalation_level, 3);
        assert_eq!(
            state.failed_phase_for_recovery,
            Some(PipelinePhase::CommitMessage)
        );
    }
}
