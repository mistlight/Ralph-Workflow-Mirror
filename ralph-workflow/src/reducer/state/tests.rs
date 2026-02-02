// Tests for state module.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::state::{AgentConfigSnapshot, CheckpointParams, CliArgsSnapshot};

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
    fn test_agent_chain_advance_to_next_model_switches_agent_when_models_exhausted() {
        let chain = AgentChainState::initial().with_agents(
            vec!["agent-a".to_string(), "agent-b".to_string()],
            vec![
                vec!["a1".to_string(), "a2".to_string()],
                vec!["b1".to_string()],
            ],
            AgentRole::Developer,
        );

        let chain = chain.advance_to_next_model(); // a1 -> a2
        assert_eq!(chain.current_agent(), Some(&"agent-a".to_string()));
        assert_eq!(chain.current_model(), Some(&"a2".to_string()));

        // Exhausted models for agent-a; should move to agent-b instead of looping models.
        let chain = chain.advance_to_next_model();
        assert_eq!(chain.current_agent(), Some(&"agent-b".to_string()));
        assert_eq!(chain.current_model_index, 0);
        assert_eq!(chain.current_model(), Some(&"b1".to_string()));
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

    fn make_checkpoint_for_state(
        phase: CheckpointPhase,
        rebase_state: CheckpointRebaseState,
    ) -> PipelineCheckpoint {
        let run_id = uuid::Uuid::new_v4().to_string();
        PipelineCheckpoint::from_params(CheckpointParams {
            phase,
            iteration: 2,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            developer_agent: "claude",
            reviewer_agent: "codex",
            cli_args: CliArgsSnapshot::new(5, 2, None, true, 2, false, None),
            developer_agent_config: AgentConfigSnapshot::new(
                "claude".into(),
                "cmd".into(),
                "-o".into(),
                None,
                true,
            ),
            reviewer_agent_config: AgentConfigSnapshot::new(
                "codex".into(),
                "cmd".into(),
                "-o".into(),
                None,
                true,
            ),
            rebase_state,
            git_user_name: None,
            git_user_email: None,
            run_id: &run_id,
            parent_run_id: None,
            resume_count: 0,
            actual_developer_runs: 2,
            actual_reviewer_runs: 0,
            working_dir: "/test/repo".to_string(),
            prompt_md_checksum: None,
            config_path: None,
            config_checksum: None,
        })
    }

    #[test]
    fn test_pipeline_state_from_checkpoint_phase_mapping() {
        let checkpoint = make_checkpoint_for_state(
            CheckpointPhase::Development,
            CheckpointRebaseState::NotStarted,
        );
        let state: PipelineState = checkpoint.into();
        assert_eq!(state.phase, PipelinePhase::Development);
    }

    #[test]
    fn test_pipeline_state_from_checkpoint_rebase_conflicts() {
        let checkpoint = make_checkpoint_for_state(
            CheckpointPhase::PreRebaseConflict,
            CheckpointRebaseState::HasConflicts {
                files: vec!["file1.rs".to_string()],
            },
        );
        let state: PipelineState = checkpoint.into();
        assert!(matches!(state.rebase, RebaseState::Conflicted { .. }));
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
    fn test_switch_to_next_agent_with_prompt_advances_retry_cycle_when_single_agent() {
        let chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        let next = chain.switch_to_next_agent_with_prompt(Some("prompt".to_string()));
        assert!(
            !next.is_exhausted(),
            "single-agent rate limit fallback should not immediately exhaust the chain"
        );
        assert_eq!(next.retry_cycle, 1);
    }

    #[test]
    fn test_switch_to_next_agent_with_prompt_advances_retry_cycle_on_wraparound() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );
        chain.current_agent_index = 1;

        let next = chain.switch_to_next_agent_with_prompt(Some("prompt".to_string()));
        assert!(
            !next.is_exhausted(),
            "rate limit fallback should not immediately exhaust on wraparound"
        );
        assert_eq!(next.retry_cycle, 1);
    }

    // =========================================================================
    // XSD retry and session tracking tests
    // =========================================================================

    #[test]
    fn test_artifact_type_display() {
        assert_eq!(format!("{}", ArtifactType::Plan), "plan");
        assert_eq!(
            format!("{}", ArtifactType::DevelopmentResult),
            "development_result"
        );
        assert_eq!(format!("{}", ArtifactType::Issues), "issues");
        assert_eq!(format!("{}", ArtifactType::FixResult), "fix_result");
        assert_eq!(format!("{}", ArtifactType::CommitMessage), "commit_message");
    }

    #[test]
    fn test_continuation_state_with_limits() {
        let state = ContinuationState::with_limits(5, 2);
        assert_eq!(state.max_xsd_retry_count, 5);
        assert_eq!(state.max_continue_count, 2);
        assert!(!state.is_continuation());
    }

    #[test]
    fn test_continuation_state_default_limits() {
        let state = ContinuationState::new();
        assert_eq!(state.max_xsd_retry_count, 10);
        assert_eq!(state.max_continue_count, 3);
    }

    #[test]
    fn test_continuation_reset_preserves_limits() {
        let state = ContinuationState::with_limits(5, 2)
            .trigger_xsd_retry()
            .trigger_xsd_retry();
        assert_eq!(state.xsd_retry_count, 2);

        let reset = state.reset();
        assert_eq!(reset.xsd_retry_count, 0);
        assert_eq!(reset.max_xsd_retry_count, 5);
        assert_eq!(reset.max_continue_count, 2);
    }

    #[test]
    fn test_continuation_with_artifact() {
        let state = ContinuationState::new().with_artifact(ArtifactType::DevelopmentResult);
        assert_eq!(
            state.current_artifact,
            Some(ArtifactType::DevelopmentResult)
        );
        assert_eq!(state.xsd_retry_count, 0);
        assert!(!state.xsd_retry_pending);
    }

    #[test]
    fn test_xsd_retry_trigger() {
        let state = ContinuationState::new()
            .with_artifact(ArtifactType::Plan)
            .trigger_xsd_retry();

        assert!(state.xsd_retry_pending);
        assert_eq!(state.xsd_retry_count, 1);
        assert_eq!(state.current_artifact, Some(ArtifactType::Plan));
    }

    #[test]
    fn test_xsd_retry_clear_pending() {
        let state = ContinuationState::new()
            .trigger_xsd_retry()
            .clear_xsd_retry_pending();

        assert!(!state.xsd_retry_pending);
        assert_eq!(state.xsd_retry_count, 1);
    }

    #[test]
    fn test_xsd_retries_exhausted() {
        let state = ContinuationState::with_limits(2, 3);
        assert!(!state.xsd_retries_exhausted());

        let state = state.trigger_xsd_retry();
        assert!(!state.xsd_retries_exhausted());

        let state = state.trigger_xsd_retry();
        assert!(state.xsd_retries_exhausted());
    }

    #[test]
    fn test_continue_trigger() {
        let state = ContinuationState::new().trigger_continue();
        assert!(state.continue_pending);
    }

    #[test]
    fn test_continue_clear_pending() {
        let state = ContinuationState::new()
            .trigger_continue()
            .clear_continue_pending();
        assert!(!state.continue_pending);
    }

    #[test]
    fn test_continuations_exhausted() {
        let state = ContinuationState::with_limits(10, 2);
        assert!(!state.continuations_exhausted());

        let state =
            state.trigger_continuation(DevelopmentStatus::Partial, "First".to_string(), None, None);
        assert!(!state.continuations_exhausted());

        let state = state.trigger_continuation(
            DevelopmentStatus::Partial,
            "Second".to_string(),
            None,
            None,
        );
        assert!(state.continuations_exhausted());
    }

    #[test]
    fn test_continuations_exhausted_semantics() {
        // Test the documented semantics: max_continue_count=3 means 3 total attempts
        // Attempts 0, 1, 2 are allowed; attempt 3+ triggers exhaustion
        let state = ContinuationState::with_limits(10, 3);
        assert_eq!(state.continuation_attempt, 0);
        assert!(
            !state.continuations_exhausted(),
            "attempt 0 should not be exhausted"
        );

        let state =
            state.trigger_continuation(DevelopmentStatus::Partial, "1".to_string(), None, None);
        assert_eq!(state.continuation_attempt, 1);
        assert!(
            !state.continuations_exhausted(),
            "attempt 1 should not be exhausted"
        );

        let state =
            state.trigger_continuation(DevelopmentStatus::Partial, "2".to_string(), None, None);
        assert_eq!(state.continuation_attempt, 2);
        assert!(
            !state.continuations_exhausted(),
            "attempt 2 should not be exhausted"
        );

        let state =
            state.trigger_continuation(DevelopmentStatus::Partial, "3".to_string(), None, None);
        assert_eq!(state.continuation_attempt, 3);
        assert!(
            state.continuations_exhausted(),
            "attempt 3 should be exhausted with max_continue_count=3"
        );
    }

    #[test]
    fn test_xsd_retries_exhausted_with_zero_max() {
        // max_xsd_retry_count=0 means XSD retries are disabled (immediate agent fallback)
        let state = ContinuationState::with_limits(10, 3).with_max_xsd_retry(0);
        assert!(
            state.xsd_retries_exhausted(),
            "0 max retries should be immediately exhausted"
        );
    }

    #[test]
    fn test_trigger_continuation_resets_xsd_retry() {
        let state = ContinuationState::new()
            .with_artifact(ArtifactType::DevelopmentResult)
            .trigger_xsd_retry()
            .trigger_xsd_retry()
            .trigger_continuation(
                DevelopmentStatus::Partial,
                "Work done".to_string(),
                None,
                None,
            );

        assert_eq!(state.xsd_retry_count, 0);
        assert!(!state.xsd_retry_pending);
        // continue_pending is now set to true by trigger_continuation to enable
        // orchestration to derive the continuation effect
        assert!(state.continue_pending);
        assert_eq!(
            state.current_artifact,
            Some(ArtifactType::DevelopmentResult)
        );
    }

    #[test]
    fn test_agent_chain_session_id() {
        let chain = AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("session-123".to_string()));

        assert_eq!(chain.last_session_id, Some("session-123".to_string()));
    }

    #[test]
    fn test_agent_chain_clear_session_id() {
        let chain = AgentChainState::initial()
            .with_session_id(Some("session-123".to_string()))
            .clear_session_id();

        assert!(chain.last_session_id.is_none());
    }

    #[test]
    fn test_agent_chain_reset_clears_session_id() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        chain.last_session_id = Some("session-123".to_string());

        let reset = chain.reset();
        assert!(
            reset.last_session_id.is_none(),
            "reset() should clear last_session_id"
        );
    }

    #[test]
    fn test_agent_chain_reset_for_role_clears_session_id() {
        let mut chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );
        chain.last_session_id = Some("session-123".to_string());

        let reset = chain.reset_for_role(AgentRole::Reviewer);
        assert!(
            reset.last_session_id.is_none(),
            "reset_for_role() should clear last_session_id"
        );
    }

    // =========================================================================
    // FixStatus tests
    // =========================================================================

    #[test]
    fn test_fix_status_parse() {
        assert_eq!(
            FixStatus::parse("all_issues_addressed"),
            Some(FixStatus::AllIssuesAddressed)
        );
        assert_eq!(
            FixStatus::parse("issues_remain"),
            Some(FixStatus::IssuesRemain)
        );
        assert_eq!(
            FixStatus::parse("no_issues_found"),
            Some(FixStatus::NoIssuesFound)
        );
        assert_eq!(FixStatus::parse("failed"), Some(FixStatus::Failed));
        assert_eq!(FixStatus::parse("unknown"), None);
    }

    #[test]
    fn test_fix_status_display() {
        assert_eq!(
            format!("{}", FixStatus::AllIssuesAddressed),
            "all_issues_addressed"
        );
        assert_eq!(format!("{}", FixStatus::IssuesRemain), "issues_remain");
        assert_eq!(format!("{}", FixStatus::NoIssuesFound), "no_issues_found");
        assert_eq!(format!("{}", FixStatus::Failed), "failed");
    }

    #[test]
    fn test_fix_status_is_complete() {
        assert!(FixStatus::AllIssuesAddressed.is_complete());
        assert!(FixStatus::NoIssuesFound.is_complete());
        assert!(!FixStatus::IssuesRemain.is_complete());
        assert!(!FixStatus::Failed.is_complete());
    }

    #[test]
    fn test_fix_status_needs_continuation() {
        assert!(!FixStatus::AllIssuesAddressed.needs_continuation());
        assert!(!FixStatus::NoIssuesFound.needs_continuation());
        assert!(FixStatus::IssuesRemain.needs_continuation());
        assert!(
            FixStatus::Failed.needs_continuation(),
            "Failed status should trigger continuation like IssuesRemain"
        );
    }
}
