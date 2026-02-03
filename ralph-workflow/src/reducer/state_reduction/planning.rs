// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_planning_event(state: PipelineState, event: PlanningEvent) -> PipelineState {
    match event {
        PlanningEvent::PhaseStarted => PipelineState {
            phase: crate::reducer::event::PipelinePhase::Planning,
            planning_prompt_prepared_iteration: None,
            planning_xml_cleaned_iteration: None,
            planning_agent_invoked_iteration: None,
            planning_xml_extracted_iteration: None,
            planning_validated_outcome: None,
            planning_markdown_written_iteration: None,
            planning_xml_archived_iteration: None,
            continuation: ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            ..state
        },
        PlanningEvent::PhaseCompleted => PipelineState {
            phase: crate::reducer::event::PipelinePhase::Development,
            planning_prompt_prepared_iteration: None,
            planning_xml_cleaned_iteration: None,
            planning_agent_invoked_iteration: None,
            planning_xml_extracted_iteration: None,
            planning_validated_outcome: None,
            planning_markdown_written_iteration: None,
            planning_xml_archived_iteration: None,
            continuation: ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            ..state
        },
        PlanningEvent::PromptPrepared { iteration } => PipelineState {
            planning_prompt_prepared_iteration: Some(iteration),
            continuation: ContinuationState {
                xsd_retry_pending: false,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        },
        PlanningEvent::PlanXmlCleaned { iteration } => PipelineState {
            planning_xml_cleaned_iteration: Some(iteration),
            ..state
        },
        PlanningEvent::AgentInvoked { iteration } => PipelineState {
            planning_agent_invoked_iteration: Some(iteration),
            continuation: ContinuationState {
                xsd_retry_pending: false,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        },
        PlanningEvent::PlanXmlExtracted { iteration } => PipelineState {
            planning_xml_extracted_iteration: Some(iteration),
            ..state
        },
        PlanningEvent::PlanXmlValidated {
            iteration,
            valid,
            markdown,
        } => PipelineState {
            planning_validated_outcome: Some(PlanningValidatedOutcome {
                iteration,
                valid,
                markdown,
            }),
            ..state
        },
        PlanningEvent::PlanMarkdownWritten { iteration } => PipelineState {
            planning_markdown_written_iteration: Some(iteration),
            ..state
        },
        PlanningEvent::PlanXmlArchived { iteration } => PipelineState {
            planning_xml_archived_iteration: Some(iteration),
            ..state
        },
        PlanningEvent::GenerationCompleted { valid, .. } => {
            if valid {
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Development,
                    planning_prompt_prepared_iteration: None,
                    planning_xml_cleaned_iteration: None,
                    planning_agent_invoked_iteration: None,
                    planning_xml_extracted_iteration: None,
                    planning_validated_outcome: None,
                    planning_markdown_written_iteration: None,
                    planning_xml_archived_iteration: None,
                    continuation: ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    ..state
                }
            } else {
                // Do not proceed to Development without a valid plan.
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Planning,
                    planning_prompt_prepared_iteration: None,
                    planning_xml_cleaned_iteration: None,
                    planning_agent_invoked_iteration: None,
                    planning_xml_extracted_iteration: None,
                    planning_validated_outcome: None,
                    planning_markdown_written_iteration: None,
                    planning_xml_archived_iteration: None,
                    ..state
                }
            }
        }

        PlanningEvent::OutputValidationFailed { iteration, attempt }
        | PlanningEvent::PlanXmlMissing { iteration, attempt } => {
            let new_xsd_count = state.continuation.xsd_retry_count + 1;
            if new_xsd_count >= state.continuation.max_xsd_retry_count {
                // XSD retries exhausted - switch to next agent
                let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Planning,
                    iteration,
                    agent_chain: new_agent_chain,
                    planning_prompt_prepared_iteration: None,
                    planning_xml_cleaned_iteration: None,
                    planning_agent_invoked_iteration: None,
                    planning_xml_extracted_iteration: None,
                    planning_validated_outcome: None,
                    planning_markdown_written_iteration: None,
                    planning_xml_archived_iteration: None,
                    continuation: ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        ..state.continuation
                    },
                    ..state
                }
            } else {
                // Stay in Planning, increment attempt counters, set retry pending
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Planning,
                    iteration,
                    planning_prompt_prepared_iteration: None,
                    planning_xml_cleaned_iteration: None,
                    planning_agent_invoked_iteration: None,
                    planning_xml_extracted_iteration: None,
                    planning_validated_outcome: None,
                    planning_markdown_written_iteration: None,
                    planning_xml_archived_iteration: None,
                    continuation: ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        xsd_retry_count: new_xsd_count,
                        xsd_retry_pending: true,
                        ..state.continuation
                    },
                    ..state
                }
            }
        }
    }
}
