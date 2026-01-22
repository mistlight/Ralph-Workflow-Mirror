//! Fault tolerance integration tests for reducer architecture.
//!
//! Tests verify that agent failures (including panics, segfaults, I/O errors)
//! never crash the pipeline and always trigger proper fallback behavior.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::event::{
    AgentErrorKind, CheckpointTrigger, PipelineEvent, PipelinePhase,
};
use ralph_workflow::reducer::state::{AgentChainState, PipelineState};

fn create_state_with_agent_chain() -> PipelineState {
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
        ..PipelineState::initial(5, 2)
    }
}

#[test]
fn test_agent_sigsegv_caught_by_fault_tolerant_executor() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();

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
            Some(ref agent) if *agent != "agent1"
        ));
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_agent_panic_caught_by_fault_tolerant_executor() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();

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
            Some(ref agent) if *agent != "agent1"
        ));
    });
}

#[test]
fn test_network_error_triggers_model_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();

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
        let state = create_state_with_agent_chain();

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
        let mut state = create_state_with_agent_chain();

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
