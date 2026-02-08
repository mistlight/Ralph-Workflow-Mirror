// NOTE: split from reducer/state_reduction/review.rs (review pass events).

use crate::agents::AgentRole;
use crate::reducer::event::PipelinePhase;
use crate::reducer::state::*;

/// Handles `ReviewEvent::PhaseStarted`.
///
/// Transitions to Review phase and initializes review state.
/// Clears any populated developer chain and resets continuation state.
pub(super) fn reduce_phase_started(state: PipelineState) -> PipelineState {
    PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        review_issues_found: false,
        // IMPORTANT: entering Review must not reuse a populated developer chain.
        // Clearing the chain ensures orchestration deterministically emits
        // InitializeAgentChain for AgentRole::Reviewer.
        agent_chain: {
            // Entering Review must clear any populated developer chain, but must preserve
            // the configured retry/backoff policy so behavior stays consistent across phases.
            AgentChainState::initial()
                .with_max_cycles(state.agent_chain.max_cycles)
                .with_backoff_policy(
                    state.agent_chain.retry_delay_ms,
                    state.agent_chain.backoff_multiplier,
                    state.agent_chain.max_backoff_ms,
                )
                .reset_for_role(AgentRole::Reviewer)
        },
        // Entering Review must reset continuation state to avoid leaking
        // development continuation context into review/fix/rebase logic.
        // Preserve configured limits to keep budgets stable across phases.
        continuation: state.continuation.reset(),
        review_issues_xml_cleaned_pass: None,
        review_issue_snippets_extracted_pass: None,
        fix_result_xml_cleaned_pass: None,
        ..state
    }
}

/// Handles `ReviewEvent::PassStarted`.
///
/// Starts a new review pass or retries the current pass.
/// Increments metrics for truly new passes, resets per-pass state.
pub(super) fn reduce_pass_started(state: PipelineState, pass: u32) -> PipelineState {
    let mut metrics = state.metrics.clone();
    // Increment for the first PassStarted (pass 0) and for any truly new pass.
    // A PassStarted re-emitted for the same pass (retry) must not increment.
    if state.metrics.review_passes_started == 0 || state.reviewer_pass != pass {
        metrics.review_passes_started += 1;
    }
    // Update current pass tracker
    metrics.current_review_pass = pass;
    // Reset per-pass fix continuation attempt counter when starting a new pass.
    // If orchestration re-emits PassStarted for the same pass (retry), preserve the
    // current per-pass attempt counter so retries don't erase history.
    if state.reviewer_pass != pass {
        metrics.fix_continuation_attempt = 0;
    }

    PipelineState {
        reviewer_pass: pass,
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
        agent_chain: if pass == state.reviewer_pass {
            // If orchestration re-emits PassStarted for the same pass (e.g., retry after
            // OutputValidationFailed), preserve the agent selection so fallback is effective.
            state.agent_chain.clone()
        } else {
            state.agent_chain.reset()
        },
        continuation: if pass == state.reviewer_pass {
            // If orchestration re-emits PassStarted for the same pass (e.g., retry after
            // OutputValidationFailed), clear xsd_retry_pending to prevent infinite loops.
            // The reducer owns retry accounting for determinism.
            ContinuationState {
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: false,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            }
        } else {
            // New pass: reset retry state but preserve configured limits
            ContinuationState {
                invalid_output_attempts: 0,
                xsd_retry_count: 0,
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: false,
                same_agent_retry_count: 0,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                // Clear review error when starting a new pass
                last_review_xsd_error: None,
                ..state.continuation
            }
        },
        metrics,
        ..state
    }
}

/// Handles `ReviewEvent::ContextPrepared`.
///
/// Marks review context as prepared for this pass.
/// Invalidates cached prompt inputs to force re-preparation with fresh context.
pub(super) fn reduce_context_prepared(state: PipelineState, pass: u32) -> PipelineState {
    PipelineState {
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
    }
}

/// Handles `ReviewEvent::PromptPrepared`.
///
/// Marks review prompt as prepared for this pass.
/// Clears retry flags since fresh prompt preparation indicates new attempt.
pub(super) fn reduce_prompt_prepared(state: PipelineState, pass: u32) -> PipelineState {
    PipelineState {
        review_prompt_prepared_pass: Some(pass),
        continuation: ContinuationState {
            xsd_retry_pending: false,
            xsd_retry_session_reuse_pending: state.continuation.xsd_retry_session_reuse_pending,
            same_agent_retry_pending: false,
            same_agent_retry_reason: None,
            ..state.continuation
        },
        ..state
    }
}

/// Handles `ReviewEvent::IssuesXmlCleaned`.
///
/// Marks issues XML as cleaned for this pass (pre-invocation cleanup).
pub(super) fn reduce_issues_xml_cleaned(state: PipelineState, pass: u32) -> PipelineState {
    PipelineState {
        review_issues_xml_cleaned_pass: Some(pass),
        ..state
    }
}

/// Handles `ReviewEvent::AgentInvoked`.
///
/// Marks agent as invoked for this pass and increments metrics.
/// Clears retry flags since agent invocation is a fresh attempt.
pub(super) fn reduce_agent_invoked(state: PipelineState, pass: u32) -> PipelineState {
    let mut metrics = state.metrics.clone();
    metrics.review_runs_total += 1;

    PipelineState {
        review_agent_invoked_pass: Some(pass),
        continuation: ContinuationState {
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

/// Handles `ReviewEvent::IssuesXmlExtracted`.
///
/// Marks issues XML as extracted for this pass.
pub(super) fn reduce_issues_xml_extracted(state: PipelineState, pass: u32) -> PipelineState {
    PipelineState {
        review_issues_xml_extracted_pass: Some(pass),
        ..state
    }
}

/// Handles `ReviewEvent::IssuesXmlValidated`.
///
/// Stores validation outcome and clears XSD error (validation succeeded).
pub(super) fn reduce_issues_xml_validated(
    state: PipelineState,
    pass: u32,
    issues_found: bool,
    clean_no_issues: bool,
    issues: Vec<String>,
    no_issues_found: Option<String>,
) -> PipelineState {
    PipelineState {
        review_validated_outcome: Some(ReviewValidatedOutcome {
            pass,
            issues_found,
            clean_no_issues,
            issues,
            no_issues_found,
        }),
        continuation: ContinuationState {
            // Clear error when validation succeeds
            last_review_xsd_error: None,
            ..state.continuation
        },
        ..state
    }
}

/// Handles `ReviewEvent::IssuesMarkdownWritten`.
///
/// Marks issues markdown as written for this pass.
pub(super) fn reduce_issues_markdown_written(state: PipelineState, pass: u32) -> PipelineState {
    PipelineState {
        review_issues_markdown_written_pass: Some(pass),
        ..state
    }
}

/// Handles `ReviewEvent::IssueSnippetsExtracted`.
///
/// Marks issue snippets as extracted for this pass.
pub(super) fn reduce_issue_snippets_extracted(state: PipelineState, pass: u32) -> PipelineState {
    PipelineState {
        review_issue_snippets_extracted_pass: Some(pass),
        ..state
    }
}

/// Handles `ReviewEvent::IssuesXmlArchived`.
///
/// Marks issues XML as archived for this pass.
pub(super) fn reduce_issues_xml_archived(state: PipelineState, pass: u32) -> PipelineState {
    PipelineState {
        review_issues_xml_archived_pass: Some(pass),
        ..state
    }
}

/// Handles `ReviewEvent::Completed`.
///
/// Completes review pass. If no issues found and all passes complete, transitions to CommitMessage.
/// Otherwise, stays in Review for fix or next pass.
pub(super) fn reduce_completed(
    state: PipelineState,
    pass: u32,
    issues_found: bool,
) -> PipelineState {
    let next_pass = if issues_found { pass } else { pass + 1 };
    let next_phase = if !issues_found && next_pass >= state.total_reviewer_passes {
        PipelinePhase::CommitMessage
    } else {
        state.phase
    };

    // Increment completed passes counter if no issues found (clean pass)
    let mut metrics = state.metrics.clone();
    if !issues_found {
        metrics.review_passes_completed += 1;
    }

    if next_phase == PipelinePhase::CommitMessage {
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
            commit: CommitState::NotStarted,
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            continuation: ContinuationState {
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
            continuation: ContinuationState {
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

/// Handles `ReviewEvent::PhaseCompleted`.
///
/// Completes entire review phase and transitions to CommitMessage.
pub(super) fn reduce_phase_completed(state: PipelineState) -> PipelineState {
    PipelineState {
        phase: PipelinePhase::CommitMessage,
        previous_phase: None,
        commit: CommitState::NotStarted,
        commit_prompt_prepared: false,
        commit_diff_prepared: false,
        commit_diff_empty: false,
        commit_agent_invoked: false,
        commit_xml_cleaned: false,
        commit_xml_extracted: false,
        commit_validated_outcome: None,
        commit_xml_archived: false,
        continuation: ContinuationState {
            invalid_output_attempts: 0,
            ..state.continuation
        },
        review_issues_xml_cleaned_pass: None,
        fix_result_xml_cleaned_pass: None,
        ..state
    }
}

/// Handles `ReviewEvent::PassCompletedClean`.
///
/// Completes a clean review pass (no issues found).
/// Advances to next pass or transitions to CommitMessage if all passes complete.
pub(super) fn reduce_pass_completed_clean(state: PipelineState, pass: u32) -> PipelineState {
    // Clean pass means no issues found in this pass.
    // Advance to the next pass when more passes remain.
    // Increment completed passes counter
    let mut metrics = state.metrics.clone();
    metrics.review_passes_completed += 1;

    let next_pass = pass + 1;
    let next_phase = if next_pass >= state.total_reviewer_passes {
        PipelinePhase::CommitMessage
    } else {
        PipelinePhase::Review
    };

    if next_phase == PipelinePhase::CommitMessage {
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
            commit: CommitState::NotStarted,
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            continuation: ContinuationState {
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
            continuation: ContinuationState {
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

/// Handles `ReviewEvent::OutputValidationFailed` and `ReviewEvent::IssuesXmlMissing`.
///
/// Increments XSD retry count and either:
/// - Sets xsd_retry_pending for another attempt (if budget remains)
/// - Switches to next agent in chain (if XSD retries exhausted)
pub(super) fn reduce_output_validation_failed(
    state: PipelineState,
    pass: u32,
    attempt: u32,
    error_detail: Option<String>,
) -> PipelineState {
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
        // Reset orchestration flags to ensure prompt is prepared and new agent is invoked
        let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
        PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: pass,
            agent_chain: new_agent_chain,
            continuation: ContinuationState {
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
            // Reset orchestration flags to ensure:
            // 1. Prompt is prepared for new agent
            // 2. New agent is invoked
            // 3. Cleanup runs before invocation
            review_prompt_prepared_pass: None,
            review_agent_invoked_pass: None,
            review_issues_xml_cleaned_pass: None,
            metrics,
            ..state
        }
    } else {
        // Stay in Review, increment attempt counters, set retry pending
        // Reset orchestration flags to ensure XSD retry prompt is prepared
        // and agent is re-invoked with the retry prompt.
        PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: pass,
            continuation: ContinuationState {
                invalid_output_attempts: attempt + 1,
                xsd_retry_count: new_xsd_count,
                xsd_retry_pending: true,
                // Reuse last session id for review XSD retry when available.
                xsd_retry_session_reuse_pending: true,
                // Preserve error detail for XSD retry prompt
                last_review_xsd_error: error_detail.clone(),
                ..state.continuation
            },
            // Reset orchestration flags to ensure:
            // 1. XSD retry prompt is prepared (review_prompt_prepared_pass = None)
            // 2. Agent is re-invoked with the retry prompt (review_agent_invoked_pass = None)
            // 3. Cleanup runs before re-invocation (review_issues_xml_cleaned_pass = None)
            // 4. Extraction runs after agent produces new output (already None from missing)
            review_prompt_prepared_pass: None,
            review_agent_invoked_pass: None,
            review_issues_xml_cleaned_pass: None,
            metrics,
            ..state
        }
    }
}
