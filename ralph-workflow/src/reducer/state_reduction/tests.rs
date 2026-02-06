// State reduction tests.
//
// Split into phase-specific test modules for maintainability.

use super::*;
use crate::agents::AgentRole;
use crate::reducer::event::AgentErrorKind;
use crate::reducer::event::PipelineEvent;
use crate::reducer::event::PipelinePhase;
use crate::reducer::event::RebasePhase;
use crate::reducer::state::AgentChainState;
use crate::reducer::state::CommitState;
use crate::reducer::state::ContinuationState;
use crate::reducer::state::PipelineState;
use crate::reducer::state::RebaseState;
use crate::reducer::state::SameAgentRetryReason;

fn create_test_state() -> PipelineState {
    PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec!["model1".to_string(), "model2".to_string()]],
            AgentRole::Developer,
        ),
        ..PipelineState::initial(5, 2)
    }
}

// Review phase started tests
#[path = "tests/review_phase.rs"]
mod review_phase;

// Basic pipeline transition tests
#[path = "tests/basic_transitions.rs"]
mod basic_transitions;

// Agent fallback and rate limit tests
#[path = "tests/agent_fallback.rs"]
mod agent_fallback;

// Rebase and commit state machine tests
#[path = "tests/rebase_commit.rs"]
mod rebase_commit;

// Finalization flow tests
#[path = "tests/finalization.rs"]
mod finalization;

// Continuation event handling tests
#[path = "tests/continuation.rs"]
mod continuation;

// Output validation failed tests
#[path = "tests/output_validation.rs"]
mod output_validation;

// Event sequence determinism tests
#[path = "tests/event_sequence.rs"]
mod event_sequence;

// Dev->Review transition agent chain tests
#[path = "tests/dev_review_transition.rs"]
mod dev_review_transition;

// XSD retry state transition tests
#[path = "tests/xsd_retry.rs"]
mod xsd_retry;

// Fix continuation tests
#[path = "tests/fix_continuation.rs"]
mod fix_continuation;

// Metrics tracking tests
#[path = "tests/metrics.rs"]
mod metrics;
