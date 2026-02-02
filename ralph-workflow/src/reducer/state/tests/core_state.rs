
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

