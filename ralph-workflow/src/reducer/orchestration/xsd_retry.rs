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
        PipelinePhase::Development => {
            // NOTE: In the new architecture, development_result.xml is produced by the
            // ANALYSIS agent, not the development agent. If XSD validation fails, it means
            // the analysis agent's XML is invalid. Ideally, we should retry the analysis
            // agent directly. However, the current implementation retries the entire
            // development flow (dev agent + analysis agent), which works but is inefficient.
            //
            // The analysis prompt includes XSD schema reference to minimize validation
            // failures. If validation still fails, the retry mechanism will eventually
            // succeed by re-running the full development iteration.
            //
            // TODO: Optimize XSD retry to detect when analysis agent's XML failed and
            // retry InvokeAnalysisAgent directly instead of PrepareDevelopmentPrompt.
            Effect::PrepareDevelopmentPrompt {
                iteration: state.iteration,
                prompt_mode: PromptMode::XsdRetry,
            }
        }
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

/// Derive the effect for same-agent retry based on current phase.
///
/// Same-agent retry starts a new invocation with the same agent (no session reuse),
/// but uses a different prompt mode to provide retry-specific guidance.
fn derive_same_agent_retry_effect(state: &PipelineState) -> Effect {
    match state.phase {
        PipelinePhase::Planning => Effect::PreparePlanningPrompt {
            iteration: state.iteration,
            prompt_mode: PromptMode::SameAgentRetry,
        },
        PipelinePhase::Development => Effect::PrepareDevelopmentPrompt {
            iteration: state.iteration,
            prompt_mode: PromptMode::SameAgentRetry,
        },
        PipelinePhase::Review => {
            if state.review_issues_found || state.continuation.fix_continue_pending {
                Effect::PrepareFixPrompt {
                    pass: state.reviewer_pass,
                    prompt_mode: PromptMode::SameAgentRetry,
                }
            } else {
                Effect::PrepareReviewPrompt {
                    pass: state.reviewer_pass,
                    prompt_mode: PromptMode::SameAgentRetry,
                }
            }
        }
        PipelinePhase::CommitMessage => Effect::PrepareCommitPrompt {
            prompt_mode: PromptMode::SameAgentRetry,
        },
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
/// 2. Same-agent retry pending (timeout/internal error, retry same agent)
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

    // Same-agent retry: invocation failed (timeout/internal error), retry same agent with
    // retry-specific prompt guidance.
    if state.continuation.same_agent_retry_pending {
        if state.continuation.same_agent_retries_exhausted() {
            debug_assert!(
                false,
                "Unexpected state: same_agent_retry_pending=true but same_agent_retries_exhausted()=true. \
                 The reducer should have cleared same_agent_retry_pending when retries exhausted. \
                 same_agent_retry_count={}, max_same_agent_retry_count={}",
                state.continuation.same_agent_retry_count,
                state.continuation.max_same_agent_retry_count
            );
        } else {
            return derive_same_agent_retry_effect(state);
        }
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

        return Effect::ReportAgentChainExhausted {
            role: state.agent_chain.current_role,
            phase: state.phase,
            cycle: state.agent_chain.retry_cycle,
        };
    }

    if let Some(duration_ms) = state.agent_chain.backoff_pending_ms {
        return Effect::BackoffWait {
            role: state.agent_chain.current_role,
            cycle: state.agent_chain.retry_cycle,
            duration_ms,
        };
    }

    determine_next_effect_for_phase(state)
}
