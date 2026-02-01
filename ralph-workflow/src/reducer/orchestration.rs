//! Orchestration logic for determining next effect.
//!
//! Contains `determine_next_effect()` which decides which effect to execute
//! based on current pipeline state.

use super::event::{CheckpointTrigger, PipelinePhase, RebasePhase};
use super::state::{CommitState, PipelineState, PromptMode, RebaseState};

use crate::agents::AgentRole;
use crate::reducer::effect::{ContinuationContextData, Effect};

/// Derive the effect for XSD retry based on current phase.
///
/// XSD retry reuses the same agent and session if available.
/// Returns the appropriate phase-specific effect with retry context.
fn derive_xsd_retry_effect(state: &PipelineState) -> Effect {
    match state.phase {
        PipelinePhase::Planning => Effect::PreparePlanningPrompt {
            iteration: state.iteration,
            prompt_mode: PromptMode::XsdRetry,
        },
        PipelinePhase::Development => Effect::PrepareDevelopmentPrompt {
            iteration: state.iteration,
            prompt_mode: PromptMode::XsdRetry,
        },
        PipelinePhase::Review => {
            if state.review_issues_found || state.continuation.fix_continue_pending {
                Effect::PrepareFixPrompt {
                    pass: state.reviewer_pass,
                    prompt_mode: PromptMode::XsdRetry,
                }
            } else {
                Effect::PrepareReviewPrompt {
                    pass: state.reviewer_pass,
                    prompt_mode: PromptMode::XsdRetry,
                }
            }
        }
        PipelinePhase::CommitMessage => Effect::PrepareCommitPrompt {
            prompt_mode: PromptMode::XsdRetry,
        },
        // Other phases don't have XSD retry
        _ => Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::PhaseTransition,
        },
    }
}

/// Derive the effect for continuation based on current phase.
///
/// Continuation starts a new session (agent starts fresh but with context).
/// Only applies to Development and Fix phases where incomplete work can continue.
fn derive_continuation_effect(state: &PipelineState) -> Effect {
    match state.phase {
        PipelinePhase::Development => {
            // Write continuation context first if needed
            if state.continuation.context_write_pending {
                let status = state
                    .continuation
                    .previous_status
                    .clone()
                    .unwrap_or(super::state::DevelopmentStatus::Failed);
                let summary = state
                    .continuation
                    .previous_summary
                    .clone()
                    .unwrap_or_default();
                let files_changed = state.continuation.previous_files_changed.clone();
                let next_steps = state.continuation.previous_next_steps.clone();

                Effect::WriteContinuationContext(ContinuationContextData {
                    iteration: state.iteration,
                    attempt: state.continuation.continuation_attempt,
                    status,
                    summary,
                    files_changed,
                    next_steps,
                })
            } else {
                Effect::PrepareDevelopmentContext {
                    iteration: state.iteration,
                }
            }
        }
        // Fix continuation: start the fix chain with a fresh session
        PipelinePhase::Review
            if state.continuation.fix_continue_pending || state.review_issues_found =>
        {
            Effect::PrepareFixPrompt {
                pass: state.reviewer_pass,
                prompt_mode: PromptMode::Normal,
            }
        }
        // Other phases don't support continuation
        _ => Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::PhaseTransition,
        },
    }
}

/// Determine the next effect to execute based on current state.
///
/// This function is pure - it only reads state and returns an effect.
/// The actual execution happens in the effect handler.
///
/// # Priority Order for Effects
///
/// 1. Continuation context cleanup (highest priority)
/// 2. XSD retry pending (validation failed, retry with same agent/session)
/// 3. Continue pending (output valid but incomplete, new session)
/// 4. Rebase in progress
/// 5. Agent chain exhausted
/// 6. Backoff wait
/// 7. Phase-specific effects
pub fn determine_next_effect(state: &PipelineState) -> Effect {
    // Terminal: once aborted, drive a single checkpoint save so the event loop can
    // deterministically complete (Interrupted + checkpoint_saved_count > 0).
    if state.phase == PipelinePhase::Interrupted && state.checkpoint_saved_count == 0 {
        return Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::Interrupt,
        };
    }

    if state.continuation.context_cleanup_pending {
        return Effect::CleanupContinuationContext;
    }

    // XSD retry: validation failed, retry with same agent/session if not exhausted.
    // Note: The reducer should clear xsd_retry_pending when retries are exhausted, so
    // normally we wouldn't see xsd_retry_pending=true AND xsd_retries_exhausted()=true.
    if state.continuation.xsd_retry_pending {
        if state.continuation.xsd_retries_exhausted() {
            // Edge case: xsd_retry_pending is true but retries are exhausted.
            // This shouldn't happen in normal operation since the reducer clears
            // xsd_retry_pending when exhausting retries. However, if it does occur
            // (e.g., due to a bug or unexpected state), we fall through to normal
            // phase effects rather than deriving a retry effect that would fail.
            debug_assert!(
                false,
                "Unexpected state: xsd_retry_pending=true but xsd_retries_exhausted()=true. \
                 The reducer should have cleared xsd_retry_pending when retries exhausted. \
                 xsd_retry_count={}, max_xsd_retry_count={}",
                state.continuation.xsd_retry_count, state.continuation.max_xsd_retry_count
            );
            // Fall through to normal phase effects
        } else {
            return derive_xsd_retry_effect(state);
        }
    }

    // Development continuation pending: output valid but work incomplete, start new session
    // Only check continue_pending in Development phase to avoid confusion with fix_continue_pending
    if state.phase == PipelinePhase::Development && state.continuation.continue_pending {
        if state.continuation.continuations_exhausted() {
            // Exhausted continuation budget - accept current state as complete
            // The budget exhaustion is handled by state reduction, so we proceed
            // to normal phase-specific effects
        } else {
            // Trigger continuation with new session
            return derive_continuation_effect(state);
        }
    }

    // Fix continuation pending: fix output valid but issues remain, start new session
    // Only check fix_continue_pending in Review phase to be explicit about phase context
    if state.phase == PipelinePhase::Review && state.continuation.fix_continue_pending {
        if state.continuation.fix_continuations_exhausted() {
            // Exhausted fix continuation budget - proceed to commit
            // The budget exhaustion is handled by state reduction
        } else {
            // Trigger fix continuation with new session
            return derive_continuation_effect(state);
        }
    }

    if matches!(
        state.rebase,
        RebaseState::InProgress { .. } | RebaseState::Conflicted { .. }
    ) {
        let phase = match state.phase {
            PipelinePhase::Planning => RebasePhase::Initial,
            _ => RebasePhase::PostReview,
        };

        return match &state.rebase {
            RebaseState::InProgress { target_branch, .. } => Effect::RunRebase {
                phase,
                target_branch: target_branch.clone(),
            },
            RebaseState::Conflicted { .. } => Effect::ResolveRebaseConflicts {
                strategy: super::event::ConflictStrategy::Continue,
            },
            _ => unreachable!("checked rebase state before matching"),
        };
    }

    if !state.agent_chain.agents.is_empty() && state.agent_chain.is_exhausted() {
        let progressed = match state.phase {
            PipelinePhase::Planning => state.iteration > 0,
            PipelinePhase::Development => state.iteration > 0,
            PipelinePhase::Review => state.reviewer_pass > 0,
            PipelinePhase::CommitMessage => matches!(
                state.commit,
                CommitState::Generated { .. }
                    | CommitState::Committed { .. }
                    | CommitState::Skipped
            ),
            PipelinePhase::FinalValidation
            | PipelinePhase::Finalizing
            | PipelinePhase::Complete
            | PipelinePhase::Interrupted => false,
        };

        if progressed
            && state.checkpoint_saved_count == 0
            && !matches!(
                state.phase,
                PipelinePhase::Complete | PipelinePhase::Interrupted
            )
        {
            return Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::Interrupt,
            };
        }

        return Effect::AbortPipeline {
            reason: format!(
                "Agent chain exhausted for role {:?} in phase {:?} (cycle {})",
                state.agent_chain.current_role, state.phase, state.agent_chain.retry_cycle
            ),
        };
    }

    if let Some(duration_ms) = state.agent_chain.backoff_pending_ms {
        return Effect::BackoffWait {
            role: state.agent_chain.current_role,
            cycle: state.agent_chain.retry_cycle,
            duration_ms,
        };
    }

    match state.phase {
        PipelinePhase::Planning => {
            if state.iteration == 0
                && state.checkpoint_saved_count == 0
                && matches!(
                    state.rebase,
                    RebaseState::Skipped | RebaseState::Completed { .. }
                )
            {
                return Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                };
            }

            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Developer,
                };
            }

            // Clean up BEFORE planning to remove old PLAN.md from previous iteration
            if !state.context_cleaned {
                return Effect::CleanupContext;
            }

            if state.planning_prompt_prepared_iteration != Some(state.iteration) {
                return Effect::PreparePlanningPrompt {
                    iteration: state.iteration,
                    prompt_mode: PromptMode::Normal,
                };
            }

            if state.planning_xml_cleaned_iteration != Some(state.iteration) {
                return Effect::CleanupPlanningXml {
                    iteration: state.iteration,
                };
            }

            if state.planning_agent_invoked_iteration != Some(state.iteration) {
                return Effect::InvokePlanningAgent {
                    iteration: state.iteration,
                };
            }

            if state.planning_xml_extracted_iteration != Some(state.iteration) {
                return Effect::ExtractPlanningXml {
                    iteration: state.iteration,
                };
            }

            let planning_validated_is_for_iteration = state
                .planning_validated_outcome
                .as_ref()
                .is_some_and(|o| o.iteration == state.iteration);
            if !planning_validated_is_for_iteration {
                return Effect::ValidatePlanningXml {
                    iteration: state.iteration,
                };
            }

            if state.planning_markdown_written_iteration != Some(state.iteration) {
                return Effect::WritePlanningMarkdown {
                    iteration: state.iteration,
                };
            }

            if state.planning_xml_archived_iteration != Some(state.iteration) {
                return Effect::ArchivePlanningXml {
                    iteration: state.iteration,
                };
            }

            let outcome = state
                .planning_validated_outcome
                .as_ref()
                .expect("validated outcome should exist before applying planning outcome");
            Effect::ApplyPlanningOutcome {
                iteration: outcome.iteration,
                valid: outcome.valid,
            }
        }

        PipelinePhase::Development => {
            if state.continuation.context_write_pending {
                let status = state
                    .continuation
                    .previous_status
                    .clone()
                    .unwrap_or(super::state::DevelopmentStatus::Failed);
                let summary = state
                    .continuation
                    .previous_summary
                    .clone()
                    .unwrap_or_default();
                let files_changed = state.continuation.previous_files_changed.clone();
                let next_steps = state.continuation.previous_next_steps.clone();

                return Effect::WriteContinuationContext(ContinuationContextData {
                    iteration: state.iteration,
                    attempt: state.continuation.continuation_attempt,
                    status,
                    summary,
                    files_changed,
                    next_steps,
                });
            }

            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Developer,
                };
            }

            if state.iteration < state.total_iterations {
                if state.development_context_prepared_iteration != Some(state.iteration) {
                    return Effect::PrepareDevelopmentContext {
                        iteration: state.iteration,
                    };
                }

                if state.development_prompt_prepared_iteration != Some(state.iteration) {
                    let prompt_mode = if state.continuation.is_continuation() {
                        PromptMode::Continuation
                    } else {
                        PromptMode::Normal
                    };
                    return Effect::PrepareDevelopmentPrompt {
                        iteration: state.iteration,
                        prompt_mode,
                    };
                }

                if state.development_xml_cleaned_iteration != Some(state.iteration) {
                    return Effect::CleanupDevelopmentXml {
                        iteration: state.iteration,
                    };
                }

                if state.development_agent_invoked_iteration != Some(state.iteration) {
                    return Effect::InvokeDevelopmentAgent {
                        iteration: state.iteration,
                    };
                }

                if state.development_xml_extracted_iteration != Some(state.iteration) {
                    return Effect::ExtractDevelopmentXml {
                        iteration: state.iteration,
                    };
                }

                let dev_validated_is_for_iteration = state
                    .development_validated_outcome
                    .as_ref()
                    .is_some_and(|o| o.iteration == state.iteration);
                if !dev_validated_is_for_iteration {
                    return Effect::ValidateDevelopmentXml {
                        iteration: state.iteration,
                    };
                }

                if state.development_xml_archived_iteration != Some(state.iteration) {
                    return Effect::ArchiveDevelopmentXml {
                        iteration: state.iteration,
                    };
                }

                Effect::ApplyDevelopmentOutcome {
                    iteration: state.iteration,
                }
            } else {
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                }
            }
        }

        PipelinePhase::Review => {
            // If review found issues, run fix attempt
            if state.review_issues_found {
                if state.agent_chain.agents.is_empty()
                    || state.agent_chain.current_role != AgentRole::Reviewer
                {
                    return Effect::InitializeAgentChain {
                        role: AgentRole::Reviewer,
                    };
                }

                if state.fix_prompt_prepared_pass != Some(state.reviewer_pass) {
                    return Effect::PrepareFixPrompt {
                        pass: state.reviewer_pass,
                        prompt_mode: PromptMode::Normal,
                    };
                }

                if state.fix_result_xml_cleaned_pass != Some(state.reviewer_pass) {
                    return Effect::CleanupFixResultXml {
                        pass: state.reviewer_pass,
                    };
                }

                if state.fix_agent_invoked_pass != Some(state.reviewer_pass) {
                    return Effect::InvokeFixAgent {
                        pass: state.reviewer_pass,
                    };
                }

                if state.fix_result_xml_extracted_pass != Some(state.reviewer_pass) {
                    return Effect::ExtractFixResultXml {
                        pass: state.reviewer_pass,
                    };
                }

                let fix_validated_is_for_pass = state
                    .fix_validated_outcome
                    .as_ref()
                    .is_some_and(|o| o.pass == state.reviewer_pass);
                if !fix_validated_is_for_pass {
                    return Effect::ValidateFixResultXml {
                        pass: state.reviewer_pass,
                    };
                }

                if state.fix_result_xml_archived_pass != Some(state.reviewer_pass) {
                    return Effect::ArchiveFixResultXml {
                        pass: state.reviewer_pass,
                    };
                }

                return Effect::ApplyFixOutcome {
                    pass: state.reviewer_pass,
                };

                // Legacy super-effect placeholder. Removed once the fix chain is complete.
            }

            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Reviewer,
                };
            }

            // Otherwise, run next review pass or complete phase
            if state.reviewer_pass < state.total_reviewer_passes {
                if state.review_context_prepared_pass != Some(state.reviewer_pass) {
                    return Effect::PrepareReviewContext {
                        pass: state.reviewer_pass,
                    };
                }

                if state.review_prompt_prepared_pass != Some(state.reviewer_pass) {
                    return Effect::PrepareReviewPrompt {
                        pass: state.reviewer_pass,
                        prompt_mode: PromptMode::Normal,
                    };
                }

                if state.review_issues_xml_cleaned_pass != Some(state.reviewer_pass) {
                    return Effect::CleanupReviewIssuesXml {
                        pass: state.reviewer_pass,
                    };
                }

                if state.review_agent_invoked_pass != Some(state.reviewer_pass) {
                    return Effect::InvokeReviewAgent {
                        pass: state.reviewer_pass,
                    };
                }

                if state.review_issues_xml_extracted_pass != Some(state.reviewer_pass) {
                    return Effect::ExtractReviewIssuesXml {
                        pass: state.reviewer_pass,
                    };
                }

                let review_validated_is_for_pass = state
                    .review_validated_outcome
                    .as_ref()
                    .is_some_and(|o| o.pass == state.reviewer_pass);
                if !review_validated_is_for_pass {
                    return Effect::ValidateReviewIssuesXml {
                        pass: state.reviewer_pass,
                    };
                }

                if state.review_issues_markdown_written_pass != Some(state.reviewer_pass) {
                    return Effect::WriteIssuesMarkdown {
                        pass: state.reviewer_pass,
                    };
                }

                if state.review_issue_snippets_extracted_pass != Some(state.reviewer_pass) {
                    return Effect::ExtractReviewIssueSnippets {
                        pass: state.reviewer_pass,
                    };
                }

                if state.review_issues_xml_archived_pass != Some(state.reviewer_pass) {
                    return Effect::ArchiveReviewIssuesXml {
                        pass: state.reviewer_pass,
                    };
                }

                let outcome = state
                    .review_validated_outcome
                    .as_ref()
                    .expect("validated outcome should exist before applying review outcome");
                Effect::ApplyReviewOutcome {
                    pass: outcome.pass,
                    issues_found: outcome.issues_found,
                    clean_no_issues: outcome.clean_no_issues,
                }
            } else {
                Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                }
            }
        }

        PipelinePhase::CommitMessage => {
            // Commit phase requires explicit agent chain initialization like other phases
            if state.agent_chain.agents.is_empty() {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Commit,
                };
            }
            match state.commit {
                CommitState::NotStarted | CommitState::Generating { .. } => {
                    if let Some(outcome) = state.commit_validated_outcome.as_ref() {
                        let current_attempt = match state.commit {
                            CommitState::Generating { attempt, .. } => attempt,
                            _ => 1,
                        };
                        if outcome.attempt == current_attempt && state.commit_xml_extracted {
                            return Effect::ApplyCommitMessageOutcome;
                        }
                    }
                    if !state.commit_diff_prepared {
                        return Effect::CheckCommitDiff;
                    }
                    if state.commit_diff_empty {
                        return Effect::SkipCommit {
                            reason: "No changes to commit (empty diff)".to_string(),
                        };
                    }
                    if !state.commit_prompt_prepared {
                        return Effect::PrepareCommitPrompt {
                            prompt_mode: PromptMode::Normal,
                        };
                    }
                    if !state.commit_xml_cleaned {
                        return Effect::CleanupCommitXml;
                    }
                    if !state.commit_agent_invoked {
                        return Effect::InvokeCommitAgent;
                    }
                    if !state.commit_xml_extracted {
                        return Effect::ExtractCommitXml;
                    }
                    Effect::ValidateCommitXml
                }
                CommitState::Generated { ref message } => {
                    if !state.commit_xml_archived {
                        Effect::ArchiveCommitXml
                    } else {
                        Effect::CreateCommit {
                            message: message.clone(),
                        }
                    }
                }
                CommitState::Committed { .. } | CommitState::Skipped => Effect::SaveCheckpoint {
                    trigger: CheckpointTrigger::PhaseTransition,
                },
            }
        }

        PipelinePhase::FinalValidation => Effect::ValidateFinalState,

        PipelinePhase::Finalizing => Effect::RestorePromptPermissions,

        PipelinePhase::Complete | PipelinePhase::Interrupted => Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::Interrupt,
        },
    }
}
#[cfg(test)]
#[path = "orchestration/tests.rs"]
mod tests;
