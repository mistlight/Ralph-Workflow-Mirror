// NOTE: split from reducer/state_reduction.rs.

use crate::agents::AgentRole;
use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_review_event(state: PipelineState, event: ReviewEvent) -> PipelineState {
    match event {
        ReviewEvent::PhaseStarted => reduce_phase_started(state),
        ReviewEvent::PassStarted { pass } => reduce_pass_started(state, pass),

        ReviewEvent::ContextPrepared { pass } => PipelineState {
            review_context_prepared_pass: Some(pass),
            // Preparing review context rewrites the diff backup and baseline.
            // Invalidate any materialized inputs for this pass so we don't reuse
            // stale PLAN/DIFF materializations.
            prompt_inputs: PromptInputsState {
                review: None,
                ..state.prompt_inputs.clone()
            },
            // Also force prompt re-preparation for this pass if it had already been prepared.
            review_prompt_prepared_pass: None,
            ..state
        },

        ReviewEvent::PromptPrepared { pass } => PipelineState {
            review_prompt_prepared_pass: Some(pass),
            continuation: crate::reducer::state::ContinuationState {
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: state.continuation.xsd_retry_pending,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        },

        ReviewEvent::IssuesXmlCleaned { pass } => PipelineState {
            review_issues_xml_cleaned_pass: Some(pass),
            ..state
        },

        ReviewEvent::AgentInvoked { pass } => {
            let mut metrics = state.metrics.clone();
            metrics.review_runs_total += 1;

            PipelineState {
                review_agent_invoked_pass: Some(pass),
                continuation: crate::reducer::state::ContinuationState {
                    xsd_retry_pending: false,
                    xsd_retry_session_reuse_pending: false,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    ..state.continuation
                },
                metrics,
                ..state
            }
        }

        ReviewEvent::IssuesXmlExtracted { pass } => PipelineState {
            review_issues_xml_extracted_pass: Some(pass),
            ..state
        },

        ReviewEvent::IssuesXmlValidated {
            pass,
            issues_found,
            clean_no_issues,
            issues,
            no_issues_found,
        } => PipelineState {
            review_validated_outcome: Some(crate::reducer::state::ReviewValidatedOutcome {
                pass,
                issues_found,
                clean_no_issues,
                issues,
                no_issues_found,
            }),
            continuation: crate::reducer::state::ContinuationState {
                // Clear error when validation succeeds
                last_review_xsd_error: None,
                ..state.continuation
            },
            ..state
        },

        ReviewEvent::IssuesMarkdownWritten { pass } => PipelineState {
            review_issues_markdown_written_pass: Some(pass),
            ..state
        },

        ReviewEvent::IssueSnippetsExtracted { pass } => PipelineState {
            review_issue_snippets_extracted_pass: Some(pass),
            ..state
        },

        ReviewEvent::IssuesXmlArchived { pass } => PipelineState {
            review_issues_xml_archived_pass: Some(pass),
            ..state
        },
        ReviewEvent::Completed { pass, issues_found } => {
            let next_pass = if issues_found { pass } else { pass + 1 };
            let next_phase = if !issues_found && next_pass >= state.total_reviewer_passes {
                crate::reducer::event::PipelinePhase::CommitMessage
            } else {
                state.phase
            };

            // Increment completed passes counter if no issues found (clean pass)
            let mut metrics = state.metrics.clone();
            if !issues_found {
                metrics.review_passes_completed += 1;
            }

            if next_phase == crate::reducer::event::PipelinePhase::CommitMessage {
                PipelineState {
                    phase: next_phase,
                    previous_phase: None,
                    reviewer_pass: next_pass,
                    review_issues_found: issues_found,
                    review_context_prepared_pass: None,
                    review_prompt_prepared_pass: None,
                    review_issues_xml_cleaned_pass: None,
                    review_agent_invoked_pass: None,
                    review_issues_xml_extracted_pass: None,
                    review_validated_outcome: None,
                    review_issues_markdown_written_pass: None,
                    review_issue_snippets_extracted_pass: None,
                    review_issues_xml_archived_pass: None,
                    commit: crate::reducer::state::CommitState::NotStarted,
                    commit_prompt_prepared: false,
                    commit_diff_prepared: false,
                    commit_diff_empty: false,
                    commit_agent_invoked: false,
                    commit_xml_cleaned: false,
                    commit_xml_extracted: false,
                    commit_validated_outcome: None,
                    commit_xml_archived: false,
                    continuation: crate::reducer::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
                        // Clear review error when transitioning to commit phase
                        last_review_xsd_error: None,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    metrics,
                    ..state
                }
            } else {
                PipelineState {
                    phase: next_phase,
                    reviewer_pass: next_pass,
                    review_issues_found: issues_found,
                    review_context_prepared_pass: None,
                    review_prompt_prepared_pass: None,
                    review_issues_xml_cleaned_pass: None,
                    review_agent_invoked_pass: None,
                    review_issues_xml_extracted_pass: None,
                    review_validated_outcome: None,
                    review_issues_markdown_written_pass: None,
                    review_issue_snippets_extracted_pass: None,
                    review_issues_xml_archived_pass: None,
                    continuation: crate::reducer::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    metrics,
                    ..state
                }
            }
        }
        // Fix attempts use the Reviewer agent chain by design. The pipeline has three
        // agent roles: Developer, Reviewer, and Commit. Fixes are performed by the same
        // agent chain configured for review (there is no separate "Fixer" role), since
        // the fix phase is part of the review workflow.
        ReviewEvent::FixAttemptStarted { .. } => PipelineState {
            agent_chain: crate::reducer::state::AgentChainState::initial()
                .with_max_cycles(state.agent_chain.max_cycles)
                .with_backoff_policy(
                    state.agent_chain.retry_delay_ms,
                    state.agent_chain.backoff_multiplier,
                    state.agent_chain.max_backoff_ms,
                )
                .reset_for_role(AgentRole::Reviewer),
            // Clear pending flags when fix attempt starts to prevent infinite loops.
            // xsd_retry_pending is cleared to ensure the XSD retry effect doesn't re-trigger
            // after the fix attempt starts a fresh agent invocation.
            continuation: crate::reducer::state::ContinuationState {
                invalid_output_attempts: 0,
                fix_continue_pending: false,
                xsd_retry_pending: false,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                // Clear fix error when starting a new fix attempt
                last_fix_xsd_error: None,
                ..state.continuation
            },
            fix_prompt_prepared_pass: None,
            fix_result_xml_cleaned_pass: None,
            fix_agent_invoked_pass: None,
            fix_result_xml_extracted_pass: None,
            fix_validated_outcome: None,
            fix_result_xml_archived_pass: None,
            ..state
        },

        ReviewEvent::FixPromptPrepared { pass } => PipelineState {
            fix_prompt_prepared_pass: Some(pass),
            continuation: crate::reducer::state::ContinuationState {
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: state.continuation.xsd_retry_pending,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                // Clear fix_continue_pending to prevent infinite loop.
                // Once the fix prompt is prepared, the fix continuation attempt has started,
                // so we should not re-derive PrepareFixPrompt.
                fix_continue_pending: false,
                ..state.continuation
            },
            ..state
        },

        ReviewEvent::FixResultXmlCleaned { pass } => PipelineState {
            fix_result_xml_cleaned_pass: Some(pass),
            ..state
        },

        ReviewEvent::FixAgentInvoked { pass } => {
            let mut metrics = state.metrics.clone();
            metrics.fix_runs_total += 1;

            PipelineState {
                fix_agent_invoked_pass: Some(pass),
                continuation: crate::reducer::state::ContinuationState {
                    xsd_retry_pending: false,
                    xsd_retry_session_reuse_pending: false,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    ..state.continuation
                },
                metrics,
                ..state
            }
        }

        ReviewEvent::FixResultXmlExtracted { pass } => PipelineState {
            fix_result_xml_extracted_pass: Some(pass),
            ..state
        },

        ReviewEvent::FixResultXmlValidated {
            pass,
            status,
            summary,
        } => PipelineState {
            fix_validated_outcome: Some(crate::reducer::state::FixValidatedOutcome {
                pass,
                status,
                summary,
            }),
            continuation: crate::reducer::state::ContinuationState {
                // Clear error when validation succeeds
                last_fix_xsd_error: None,
                ..state.continuation
            },
            ..state
        },

        ReviewEvent::FixResultXmlArchived { pass } => PipelineState {
            fix_result_xml_archived_pass: Some(pass),
            ..state
        },
        ReviewEvent::FixOutcomeApplied { pass } => {
            let Some(outcome) = state
                .fix_validated_outcome
                .as_ref()
                .filter(|o| o.pass == pass)
            else {
                return state;
            };

            let next_event = if outcome.status.needs_continuation() {
                let next_attempt = state.continuation.fix_continuation_attempt + 1;
                if next_attempt >= state.continuation.max_fix_continue_count {
                    ReviewEvent::FixContinuationBudgetExhausted {
                        pass,
                        total_attempts: next_attempt,
                        last_status: outcome.status.clone(),
                    }
                } else {
                    ReviewEvent::FixContinuationTriggered {
                        pass,
                        status: outcome.status.clone(),
                        summary: outcome.summary.clone(),
                    }
                }
            } else {
                let changes_made = matches!(outcome.status, FixStatus::AllIssuesAddressed);
                ReviewEvent::FixAttemptCompleted { pass, changes_made }
            };

            reduce_review_event(state, next_event)
        }
        ReviewEvent::FixAttemptCompleted { pass, .. } => {
            // Fix completed successfully - increment completed passes counter
            let mut metrics = state.metrics.clone();
            metrics.review_passes_completed += 1;

            PipelineState {
                phase: crate::reducer::event::PipelinePhase::CommitMessage,
                previous_phase: Some(crate::reducer::event::PipelinePhase::Review),
                reviewer_pass: pass,
                review_issues_found: false,
                fix_prompt_prepared_pass: None,
                fix_result_xml_cleaned_pass: None,
                fix_agent_invoked_pass: None,
                fix_result_xml_extracted_pass: None,
                fix_validated_outcome: None,
                fix_result_xml_archived_pass: None,
                commit: crate::reducer::state::CommitState::NotStarted,
                commit_prompt_prepared: false,
                commit_diff_prepared: false,
                commit_diff_empty: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                continuation: crate::reducer::state::ContinuationState {
                    invalid_output_attempts: 0,
                    // Clear fix error when transitioning to commit phase
                    last_fix_xsd_error: None,
                    ..state.continuation
                },
                metrics,
                ..state
            }
        }
        ReviewEvent::PhaseCompleted { .. } => PipelineState {
            phase: crate::reducer::event::PipelinePhase::CommitMessage,
            previous_phase: None,
            commit: crate::reducer::state::CommitState::NotStarted,
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            continuation: crate::reducer::state::ContinuationState {
                invalid_output_attempts: 0,
                ..state.continuation
            },
            review_issues_xml_cleaned_pass: None,
            fix_result_xml_cleaned_pass: None,
            ..state
        },
        ReviewEvent::PassCompletedClean { pass } => {
            // Clean pass means no issues found in this pass.
            // Advance to the next pass when more passes remain.
            // Increment completed passes counter
            let mut metrics = state.metrics.clone();
            metrics.review_passes_completed += 1;

            let next_pass = pass + 1;
            let next_phase = if next_pass >= state.total_reviewer_passes {
                crate::reducer::event::PipelinePhase::CommitMessage
            } else {
                crate::reducer::event::PipelinePhase::Review
            };

            if next_phase == crate::reducer::event::PipelinePhase::CommitMessage {
                PipelineState {
                    phase: next_phase,
                    previous_phase: None,
                    reviewer_pass: next_pass,
                    review_issues_found: false,
                    review_context_prepared_pass: None,
                    review_prompt_prepared_pass: None,
                    review_issues_xml_cleaned_pass: None,
                    review_agent_invoked_pass: None,
                    review_issues_xml_extracted_pass: None,
                    review_validated_outcome: None,
                    review_issues_markdown_written_pass: None,
                    review_issue_snippets_extracted_pass: None,
                    review_issues_xml_archived_pass: None,
                    commit: crate::reducer::state::CommitState::NotStarted,
                    commit_prompt_prepared: false,
                    commit_diff_prepared: false,
                    commit_diff_empty: false,
                    commit_agent_invoked: false,
                    commit_xml_cleaned: false,
                    commit_xml_extracted: false,
                    commit_validated_outcome: None,
                    commit_xml_archived: false,
                    continuation: crate::reducer::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    metrics,
                    ..state
                }
            } else {
                PipelineState {
                    phase: next_phase,
                    reviewer_pass: next_pass,
                    review_issues_found: false,
                    review_context_prepared_pass: None,
                    review_prompt_prepared_pass: None,
                    review_issues_xml_cleaned_pass: None,
                    review_agent_invoked_pass: None,
                    review_issues_xml_extracted_pass: None,
                    review_validated_outcome: None,
                    review_issues_markdown_written_pass: None,
                    review_issue_snippets_extracted_pass: None,
                    review_issues_xml_archived_pass: None,
                    continuation: crate::reducer::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
                        ..state.continuation
                    },
                    fix_result_xml_cleaned_pass: None,
                    metrics,
                    ..state
                }
            }
        }
        ReviewEvent::OutputValidationFailed {
            pass,
            attempt,
            error_detail,
        }
        | ReviewEvent::IssuesXmlMissing {
            pass,
            attempt,
            error_detail,
        } => {
            // Policy: The reducer maintains retry state for determinism.
            // Handlers should emit `attempt` from state (checkpoint-resume safe).
            let new_xsd_count = state.continuation.xsd_retry_count + 1;
            let mut metrics = state.metrics.clone();

            // Only increment metrics if we're actually retrying (not exhausted)
            let will_retry = new_xsd_count < state.continuation.max_xsd_retry_count;
            if will_retry {
                metrics.xsd_retry_review += 1;
                metrics.xsd_retry_attempts_total += 1;
            }

            if new_xsd_count >= state.continuation.max_xsd_retry_count {
                // XSD retries exhausted - switch to next agent
                let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Review,
                    reviewer_pass: pass,
                    agent_chain: new_agent_chain,
                    continuation: crate::reducer::state::ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
                        same_agent_retry_count: 0,
                        same_agent_retry_pending: false,
                        same_agent_retry_reason: None,
                        // Clear error when switching agents
                        last_review_xsd_error: None,
                        ..state.continuation
                    },
                    review_issues_xml_cleaned_pass: None,
                    metrics,
                    ..state
                }
            } else {
                // Stay in Review, increment attempt counters, set retry pending
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Review,
                    reviewer_pass: pass,
                    continuation: crate::reducer::state::ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        xsd_retry_count: new_xsd_count,
                        xsd_retry_pending: true,
                        xsd_retry_session_reuse_pending: false,
                        // Preserve error detail for XSD retry prompt
                        last_review_xsd_error: error_detail.clone(),
                        ..state.continuation
                    },
                    review_issues_xml_cleaned_pass: None,
                    metrics,
                    ..state
                }
            }
        }

        // Fix continuation events
        ReviewEvent::FixContinuationTriggered {
            pass,
            status,
            summary,
        } => {
            // Fix output is valid but indicates work is incomplete (issues_remain)
            let mut metrics = state.metrics.clone();
            metrics.fix_continuations_total += 1;

            PipelineState {
                reviewer_pass: pass,
                fix_prompt_prepared_pass: None,
                fix_result_xml_cleaned_pass: None,
                fix_agent_invoked_pass: None,
                fix_result_xml_extracted_pass: None,
                fix_validated_outcome: None,
                fix_result_xml_archived_pass: None,
                continuation: state.continuation.trigger_fix_continuation(status, summary),
                metrics,
                ..state
            }
        }

        ReviewEvent::FixContinuationSucceeded {
            pass,
            total_attempts: _,
        } => {
            // Fix continuation succeeded - transition to CommitMessage
            // Use reset() instead of new() to preserve configured limits
            PipelineState {
                phase: crate::reducer::event::PipelinePhase::CommitMessage,
                previous_phase: Some(crate::reducer::event::PipelinePhase::Review),
                reviewer_pass: pass,
                review_issues_found: false,
                commit: crate::reducer::state::CommitState::NotStarted,
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                continuation: state.continuation.reset(),
                fix_result_xml_cleaned_pass: None,
                ..state
            }
        }

        ReviewEvent::FixContinuationBudgetExhausted {
            pass,
            total_attempts: _,
            last_status: _,
        } => reduce_fix_continuation_budget_exhausted(state, pass),

        ReviewEvent::FixOutputValidationFailed {
            pass,
            attempt,
            error_detail,
        }
        | ReviewEvent::FixResultXmlMissing {
            pass,
            attempt,
            error_detail,
        } => reduce_fix_output_validation_failure(state, pass, attempt, error_detail.clone()),
    }
}

include!("review/helpers.rs");
