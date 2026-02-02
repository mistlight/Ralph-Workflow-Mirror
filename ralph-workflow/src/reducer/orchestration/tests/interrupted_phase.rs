// Interrupted phase tests.
//
// Tests for checkpoint behavior when pipeline is interrupted.

use super::*;

#[test]
fn test_interrupted_phase_saves_checkpoint_before_abort_loop() {
    // Regression: if agent chain exhaustion triggers AbortPipeline and the reducer
    // transitions to Interrupted, orchestration must not keep returning AbortPipeline.
    // It should drive a checkpoint save so the event loop can mark completion.
    let state = PipelineState {
        phase: PipelinePhase::Interrupted,
        checkpoint_saved_count: 0,
        agent_chain: AgentChainState::initial()
            .with_agents(vec!["a".to_string()], vec![vec![]], AgentRole::Reviewer)
            .with_max_cycles(0),
        ..PipelineState::initial(0, 1)
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(
        effect,
        Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::Interrupt
        }
    ));
}
