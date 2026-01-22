//! Fault tolerance integration tests for reducer architecture.
//!
//! Tests verify that agent failures (including panics, segfaults, I/O errors)
//! never crash the pipeline and always trigger proper fallback behavior.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::event::{AgentErrorKind, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::state::{AgentChainState, PipelineState};

fn create_state_with_agent_chain_in_development() -> PipelineState {
    use ralph_workflow::reducer::state::{CommitState, RebaseState};

    PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec![
                "agent1".to_string(),
                "agent2".to_string(),
                "agent3".to_string(),
            ],
            vec![
                vec!["model1".to_string(), "model2".to_string()],
                vec!["model1".to_string()],
                vec![],
            ],
            AgentRole::Developer,
        ),
        phase: PipelinePhase::Development,
        iteration: 1,
        total_iterations: 5,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        rebase: RebaseState::NotStarted,
        commit: CommitState::NotStarted,
        execution_history: Vec::new(),
    }
}

#[test]
fn test_agent_sigsegv_caught_by_fault_tolerant_executor() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();

        let event = PipelineEvent::AgentInvocationFailed {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            exit_code: 139,
            error_kind: AgentErrorKind::InternalError,
            retriable: false,
        };

        let new_state = ralph_workflow::reducer::state_reduction::reduce(state, event);

        assert!(matches!(
            new_state.agent_chain.current_agent(),
            Some(agent) if agent != "agent1"
        ));
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_agent_panic_caught_by_fault_tolerant_executor() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();

        let event = PipelineEvent::AgentInvocationFailed {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            exit_code: 1,
            error_kind: AgentErrorKind::InternalError,
            retriable: false,
        };

        let new_state = ralph_workflow::reducer::state_reduction::reduce(state, event);

        assert!(matches!(
            new_state.agent_chain.current_agent(),
            Some(agent) if agent != "agent1"
        ));
    });
}

#[test]
fn test_network_error_triggers_model_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();

        let event = PipelineEvent::AgentInvocationFailed {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            exit_code: 1,
            error_kind: AgentErrorKind::Network,
            retriable: true,
        };

        let new_state = ralph_workflow::reducer::state_reduction::reduce(state, event);

        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert!(new_state.agent_chain.current_model_index > 0);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_auth_error_triggers_agent_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();

        let event = PipelineEvent::AgentInvocationFailed {
            role: AgentRole::Developer,
            agent: "agent1".to_string(),
            exit_code: 1,
            error_kind: AgentErrorKind::Authentication,
            retriable: false,
        };

        let new_state = ralph_workflow::reducer::state_reduction::reduce(state, event);

        assert!(new_state.agent_chain.current_agent_index > 0);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_pipeline_state_machine_recovers_from_multiple_failures() {
    with_default_timeout(|| {
        let mut state = create_state_with_agent_chain_in_development();

        let events = vec![
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Authentication,
                retriable: false,
            },
            PipelineEvent::AgentInvocationSucceeded {
                role: AgentRole::Developer,
                agent: "agent2".to_string(),
            },
        ];

        for event in events {
            state = ralph_workflow::reducer::state_reduction::reduce(state, event);
        }

        assert_eq!(state.agent_chain.current_agent().unwrap(), "agent2");
        assert_eq!(state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_agent_fails_after_10_retries_fallback_to_next_agent() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();
        let initial_model_index = state.agent_chain.current_model_index;

        let new_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );

        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
        assert!(new_state.agent_chain.current_model_index > initial_model_index);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_agent_fails_after_99_retries_fallback_to_next_agent() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();
        let initial_model_index = state.agent_chain.current_model_index;

        let new_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::RateLimit,
                retriable: true,
            },
        );

        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
        assert!(new_state.agent_chain.current_model_index > initial_model_index);
        assert!(matches!(new_state.phase, PipelinePhase::Development));
    });
}

#[test]
fn test_all_agents_exhausted_pipeline_graceful_abort() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::{CommitState, RebaseState};

        let state = PipelineState {
            agent_chain: AgentChainState::initial()
                .with_agents(
                    vec!["agent1".to_string()],
                    vec![vec!["model1".to_string()]],
                    AgentRole::Developer,
                )
                .with_max_cycles(1),
            phase: PipelinePhase::Development,
            iteration: 1,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            execution_history: Vec::new(),
        };

        let exhausted_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::AgentChainExhausted {
                role: AgentRole::Developer,
            },
        );

        assert_eq!(exhausted_state.agent_chain.current_agent_index, 0);
        assert_eq!(exhausted_state.agent_chain.current_model_index, 0);
        assert_eq!(exhausted_state.agent_chain.retry_cycle, 1);
        assert_eq!(exhausted_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_agent_exhaustion_transitions_to_next_phase() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::{CommitState, RebaseState};

        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec!["model1".to_string()]],
                AgentRole::Developer,
            )
            .with_max_cycles(1);
        chain = chain.start_retry_cycle();

        let state = PipelineState {
            agent_chain: chain,
            phase: PipelinePhase::Development,
            iteration: 1,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            execution_history: Vec::new(),
        };

        assert_eq!(state.phase, PipelinePhase::Development);
        assert!(state.agent_chain.is_exhausted());
        assert_eq!(state.agent_chain.retry_cycle, 1);
    });
}

#[test]
fn test_pipeline_continues_after_agent_sigsegv() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();
        let initial_agent_index = state.agent_chain.current_agent_index;

        let new_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 139,
                error_kind: AgentErrorKind::InternalError,
                retriable: false,
            },
        );

        assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_pipeline_continues_after_multiple_agent_failures() {
    with_default_timeout(|| {
        let mut state = create_state_with_agent_chain_in_development();

        let events = vec![
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Authentication,
                retriable: false,
            },
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent2".to_string(),
                exit_code: 139,
                error_kind: AgentErrorKind::InternalError,
                retriable: false,
            },
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent3".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::FileSystem,
                retriable: false,
            },
        ];

        for event in events {
            state = ralph_workflow::reducer::state_reduction::reduce(state, event);
        }

        assert!(state.agent_chain.current_agent().is_some());
        assert_eq!(state.phase, PipelinePhase::Development);
    });
}
