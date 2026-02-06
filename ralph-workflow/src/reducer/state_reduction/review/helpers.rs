fn reduce_phase_started(state: PipelineState) -> PipelineState {
    PipelineState {
        phase: crate::reducer::event::PipelinePhase::Review,
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

fn reduce_pass_started(state: PipelineState, pass: u32) -> PipelineState {
    let mut metrics = state.metrics.clone();
    // Only increment if this is truly a new pass (not a retry within same pass)
    if state.reviewer_pass != pass {
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

fn reduce_fix_continuation_budget_exhausted(state: PipelineState, pass: u32) -> PipelineState {
    // Fix continuation budget exhausted - proceed to commit with current state
    // Policy: We accept partial fixes rather than blocking the pipeline
    // Use reset() instead of new() to preserve configured limits
    PipelineState {
        phase: crate::reducer::event::PipelinePhase::CommitMessage,
        previous_phase: Some(crate::reducer::event::PipelinePhase::Review),
        reviewer_pass: pass,
        commit: crate::reducer::state::CommitState::NotStarted,
        commit_prompt_prepared: false,
        commit_diff_prepared: false,
        commit_diff_empty: false,
        commit_diff_content_id_sha256: None,
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

fn reduce_fix_output_validation_failure(
    state: PipelineState,
    pass: u32,
    attempt: u32,
    error_detail: Option<String>,
) -> PipelineState {
    // Same policy as review output validation failure
    let new_xsd_count = state.continuation.xsd_retry_count + 1;
    let mut metrics = state.metrics.clone();

    // Only increment metrics if we're actually retrying (not exhausted)
    let will_retry = new_xsd_count < state.continuation.max_xsd_retry_count;
    if will_retry {
        metrics.xsd_retry_fix += 1;
        metrics.xsd_retry_attempts_total += 1;
    }

    if new_xsd_count >= state.continuation.max_xsd_retry_count {
        // XSD retries exhausted - switch to next agent
        let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
        PipelineState {
            phase: crate::reducer::event::PipelinePhase::Review,
            reviewer_pass: pass,
            agent_chain: new_agent_chain,
            continuation: ContinuationState {
                invalid_output_attempts: 0,
                xsd_retry_count: 0,
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: false,
                // Clear error when switching agents
                last_fix_xsd_error: None,
                ..state.continuation
            },
            fix_result_xml_cleaned_pass: None,
            metrics,
            ..state
        }
    } else {
        // Stay in Review, increment attempt counters, set retry pending
        PipelineState {
            phase: crate::reducer::event::PipelinePhase::Review,
            reviewer_pass: pass,
            continuation: ContinuationState {
                invalid_output_attempts: attempt + 1,
                xsd_retry_count: new_xsd_count,
                xsd_retry_pending: true,
                xsd_retry_session_reuse_pending: false,
                // Preserve error detail for XSD retry prompt
                last_fix_xsd_error: error_detail,
                ..state.continuation
            },
            fix_result_xml_cleaned_pass: None,
            metrics,
            ..state
        }
    }
}
