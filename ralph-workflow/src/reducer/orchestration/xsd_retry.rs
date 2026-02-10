/// Compute an effect fingerprint for loop detection.
///
/// The fingerprint uniquely identifies the "work context" that would produce
/// an effect. If the same fingerprint appears consecutively many times, we're
/// likely in a tight loop.
///
/// The fingerprint includes:
/// - Current phase
/// - Current agent role
/// - Current iteration
/// - Current reviewer pass
/// - XSD retry pending flag
/// - XSD retry count (to distinguish retry 1 from retry 10 in tight loop detection)
pub fn compute_effect_fingerprint(state: &PipelineState) -> String {
    format!(
        "{}:{}:iter={}:pass={}:xsd_retry={}:count={}",
        state.phase,
        state.agent_chain.current_role,
        state.iteration,
        state.reviewer_pass,
        state.continuation.xsd_retry_pending,
        state.continuation.xsd_retry_count
    )
}

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
            // development_result.xml is produced by the analysis agent.
            // When XSD validation fails, retry analysis output generation directly.
            // Ensure the analysis agent chain role is initialized (resume safety).
            if state.agent_chain.current_role != AgentRole::Analysis {
                return Effect::InitializeAgentChain {
                    role: AgentRole::Analysis,
                };
            }
            Effect::InvokeAnalysisAgent {
                iteration: state.iteration,
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
        PipelinePhase::Development => {
            // Development phase runs BOTH developer and analysis agents.
            // Same-agent retries must be role-aware so analysis failures retry analysis,
            // not the developer prompt chain.
            if state.agent_chain.current_role == AgentRole::Analysis {
                Effect::InvokeAnalysisAgent {
                    iteration: state.iteration,
                }
            } else {
                Effect::PrepareDevelopmentPrompt {
                    iteration: state.iteration,
                    prompt_mode: PromptMode::SameAgentRetry,
                }
            }
        }
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
        // BUT: if restoration is pending, do that FIRST before saving checkpoint
        if state.prompt_permissions.restore_needed && !state.prompt_permissions.restored {
            return Effect::RestorePromptPermissions;
        }
        return Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::Interrupt,
        };
    }

    // Startup: Lock PROMPT.md permissions before any work (best-effort protection)
    if !state.prompt_permissions.locked {
        return Effect::LockPromptPermissions;
    }

    // Loop detection: check if the same effect has been derived too many times consecutively.
    // This prevents infinite tight loops when XSD retry or other recovery mechanisms cannot
    // converge (e.g., due to workspace/CWD path mismatch).
    let effect_fingerprint = compute_effect_fingerprint(state);
    let loop_detected = state
        .continuation
        .last_effect_kind
        .as_deref()
        .is_some_and(|last| last == effect_fingerprint)
        && state.continuation.consecutive_same_effect_count
            >= state.continuation.max_consecutive_same_effect;

    if loop_detected
        && !matches!(
            state.phase,
            PipelinePhase::Complete | PipelinePhase::Interrupted
        )
    {
        // MANDATORY RECOVERY: we're in a tight loop and not in a terminal phase
        return Effect::TriggerLoopRecovery {
            detected_loop: effect_fingerprint,
            loop_count: state.continuation.consecutive_same_effect_count,
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
            | PipelinePhase::AwaitingDevFix
            | PipelinePhase::Interrupted => false,
        };

        if progressed
            && state.checkpoint_saved_count == 0
            && !matches!(
                state.phase,
                PipelinePhase::Complete
                    | PipelinePhase::Interrupted
                    | PipelinePhase::AwaitingDevFix
            )
        {
            return Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::Interrupt,
            };
        }

        // AwaitingDevFix is the phase we transition to AFTER reporting agent chain exhaustion.
        // If we're already in AwaitingDevFix with an exhausted chain, don't report exhaustion
        // again - instead fall through to phase-specific orchestration (TriggerDevFixFlow).
        if matches!(state.phase, PipelinePhase::AwaitingDevFix) {
            // Fall through to determine_next_effect_for_phase
        } else {
            return Effect::ReportAgentChainExhausted {
                role: state.agent_chain.current_role,
                phase: state.phase,
                cycle: state.agent_chain.retry_cycle,
            };
        }
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
