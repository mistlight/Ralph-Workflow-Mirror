//! Test helper utilities for reducer fault tolerance tests.
//!
//! Provides factory functions for creating common test state configurations
//! used across multiple fault tolerance test scenarios.

use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::state::{AgentChainState, PipelineState};

pub(super) fn create_state_with_agent_chain_in_development() -> PipelineState {
    use ralph_workflow::reducer::state::{CommitState, ContinuationState, RebaseState};

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
        previous_phase: None,
        iteration: 1,
        total_iterations: 5,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        review_issues_found: false,
        context_cleaned: false,
        rebase: RebaseState::NotStarted,
        commit: CommitState::NotStarted,
        continuation: ContinuationState::new(),
        checkpoint_saved_count: 0,
        execution_history: Vec::new(),
        ..PipelineState::initial(5, 2)
    }
}
