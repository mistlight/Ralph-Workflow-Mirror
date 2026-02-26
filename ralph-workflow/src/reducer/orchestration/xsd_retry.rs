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
///
/// Intentionally excludes retry counters like `xsd_retry_count` so that repeated
/// retries still register as the "same effect" for tight-loop detection.
#[must_use] 
pub fn compute_effect_fingerprint(state: &PipelineState) -> String {
    format!(
        "{}:{}:iter={}:pass={}:xsd_retry={}",
        state.phase,
        state.agent_chain.current_role,
        state.iteration,
        state.reviewer_pass,
        state.continuation.xsd_retry_pending
    )
}

#[cfg(test)]
mod xsd_retry_fingerprint_tests {
    use super::compute_effect_fingerprint;
    use crate::agents::AgentRole;
    use crate::reducer::event::PipelinePhase;
    use crate::reducer::state::PipelineState;

    #[test]
    fn test_effect_fingerprint_ignores_xsd_retry_count() {
        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain.current_role = AgentRole::Developer;
        state.iteration = 1;
        state.reviewer_pass = 0;
        state.continuation.xsd_retry_pending = true;

        state.continuation.xsd_retry_count = 1;
        let fp1 = compute_effect_fingerprint(&state);
        state.continuation.xsd_retry_count = 2;
        let fp2 = compute_effect_fingerprint(&state);

        assert_eq!(fp1, fp2);
    }
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
#[must_use] 
pub fn determine_next_effect(state: &PipelineState) -> Effect {
    // Terminal: once aborted, drive a single checkpoint save so the event loop can
    // deterministically complete (Interrupted + checkpoint_saved_count > 0).
    if state.phase == PipelinePhase::Interrupted && state.checkpoint_saved_count == 0 {
        // BUT: if restoration is pending, do that FIRST before termination effects.
        if state.prompt_permissions.restore_needed && !state.prompt_permissions.restored {
            return Effect::RestorePromptPermissions;
        }

        // Do NOT bypass the pre-termination commit safety check here.
        // The ONLY exception is Ctrl+C (interrupted_by_user=true), which is handled
        // in phase-specific orchestration.
        return determine_next_effect_for_phase(state);
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

    // Cloud mode orchestration: sequence cloud-specific operations
    // CRITICAL: All cloud-specific logic is guarded by cloud_config.enabled check.
    // When cloud mode is disabled, this entire block is skipped and behavior is
    // identical to current CLI behavior.
    if state.cloud_config.enabled {
        // After a successful commit, push immediately (cloud mode only)
        if let Some(commit_sha) = &state.pending_push_commit {
            // Configure git auth first if not done yet
            if !state.git_auth_configured {
                // Format auth method for the effect
                let auth_method = match &state.cloud_config.git_remote.auth_method {
                    crate::config::GitAuthStateMethod::SshKey { key_path } => key_path
                        .as_ref().map_or_else(|| "ssh-key:default".to_string(), |p| format!("ssh-key:{p}")),
                    crate::config::GitAuthStateMethod::Token { username } => {
                        format!("token:{username}")
                    }
                    crate::config::GitAuthStateMethod::CredentialHelper { helper } => {
                        format!("credential-helper:{helper}")
                    }
                };
                return Effect::ConfigureGitAuth { auth_method };
            }

            // Then push the commit
            if state.cloud_config.git_remote.push_branch.is_empty() {
                return Effect::EmitCompletionMarkerAndTerminate {
                    is_failure: true,
                    reason: Some(
                        "Cloud mode is enabled but no push branch was resolved".to_string(),
                    ),
                };
            }
            return Effect::PushToRemote {
                remote: state.cloud_config.git_remote.remote_name.clone(),
                branch: state.cloud_config.git_remote.push_branch.clone(),
                force: state.cloud_config.git_remote.force_push,
                commit_sha: commit_sha.clone(),
            };
        }

        // In Finalizing phase, create PR if configured
        if state.phase == PipelinePhase::Finalizing
            && state.cloud_config.git_remote.create_pr
            && !state.pr_created
        {
            // PR creation is only meaningful if we actually produced commits.
            // If no commits were created, skip PR creation even if configured.
            if state.metrics.commits_created_total == 0 {
                // Fall through to normal phase effects (finalization/cleanup).
                // Completion reporting will still include push_count/unpushed_commits.
            } else {
                if !state.unpushed_commits.is_empty()
                    || state.push_count == 0
                    || state.last_pushed_commit.is_none()
                {
                    return Effect::EmitCompletionMarkerAndTerminate {
                        is_failure: true,
                        reason: Some(
                            "Cannot create PR because required pushes did not succeed (unpushed commits remain)"
                                .to_string(),
                        ),
                    };
                }

                if state.cloud_config.git_remote.push_branch.is_empty() {
                    return Effect::EmitCompletionMarkerAndTerminate {
                        is_failure: true,
                        reason: Some(
                            "Cloud mode is enabled but no PR head branch was resolved".to_string(),
                        ),
                    };
                }
                let (title, body) = render_cloud_pr_title_and_body(state);
                return Effect::CreatePullRequest {
                    base_branch: state
                        .cloud_config
                        .git_remote
                        .pr_base_branch
                        .clone()
                        .unwrap_or_else(|| "main".to_string()),
                    head_branch: state.cloud_config.git_remote.push_branch.clone(),
                    title,
                    body,
                };
            }
        }
    }

    // Recovery completion: if the pipeline entered recovery due to a commit failure,
    // only clear recovery state AFTER CreateCommit has succeeded.
    //
    // Commit success is represented by CommitState::Committed (or Skipped) which occurs
    // after the CreateCommit/SkipCommit effect has completed and the reducer advanced
    // the phase. We intentionally do this here (not in commit-phase orchestration) so
    // we don't clear counters before retrying a potentially failing CreateCommit.
    if state.dev_fix_attempt_count > 0
        && state.recovery_escalation_level > 0
        && state.failed_phase_for_recovery == Some(PipelinePhase::CommitMessage)
        && matches!(
            state.commit,
            CommitState::Committed { .. } | CommitState::Skipped
        )
    {
        return Effect::EmitRecoverySuccess {
            level: state.recovery_escalation_level,
            total_attempts: state.dev_fix_attempt_count,
        };
    }

    determine_next_effect_for_phase(state)
}

fn render_cloud_pr_title_and_body(state: &PipelineState) -> (String, String) {
    use std::collections::HashMap;

    let run_id = state.cloud_config.run_id.as_deref().unwrap_or("unknown");

    // Intentionally avoid using any prompt text or other potentially sensitive input.
    // This value is safe to publish in a PR title/body.
    let prompt_summary = format!("Ralph workflow run {run_id}");

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("run_id", run_id.to_string());
    vars.insert("prompt_summary", prompt_summary);

    let default_title = "Ralph workflow changes".to_string();

    let title = state
        .cloud_config
        .git_remote
        .pr_title_template
        .as_deref()
        .and_then(|t| try_render_cloud_pr_template(t, &vars))
        .unwrap_or(default_title);

    let body = state
        .cloud_config
        .git_remote
        .pr_body_template
        .as_deref()
        .and_then(|t| try_render_cloud_pr_template(t, &vars))
        .unwrap_or_default();

    (title, body)
}

fn try_render_cloud_pr_template(
    template: &str,
    vars: &std::collections::HashMap<&str, String>,
) -> Option<String> {
    let converted = convert_cloud_pr_template_placeholders(template)?;

    let partials: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let t = crate::prompts::template_engine::Template::new(&converted);
    t.render_with_partials(vars, &partials).ok()
}

fn convert_cloud_pr_template_placeholders(input: &str) -> Option<String> {
    // Supported placeholders are documented as {run_id} and {prompt_summary}.
    // We render them using the existing template engine's {{var}} syntax.
    const ALLOWED: [&str; 2] = ["run_id", "prompt_summary"];

    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '{' {
            out.push(ch);
            continue;
        }

        // Preserve template-engine escapes/variables like {{run_id}}.
        if chars.peek() == Some(&'{') {
            out.push('{');
            out.push('{');
            let _ = chars.next();
            continue;
        }

        let mut name = String::new();
        while let Some(&next) = chars.peek() {
            if next == '}' {
                break;
            }
            name.push(next);
            let _ = chars.next();
        }

        // No closing brace; treat as literal.
        if chars.peek() != Some(&'}') {
            out.push('{');
            out.push_str(&name);
            continue;
        }
        let _ = chars.next();

        let trimmed = name.trim();
        if is_simple_placeholder_name(trimmed) {
            if ALLOWED.contains(&trimmed) {
                out.push_str("{{");
                out.push_str(trimmed);
                out.push_str("}}");
            } else {
                // Fail-fast: unknown placeholders must not pass through verbatim.
                return None;
            }
        } else {
            // Not a placeholder shape; keep original braces.
            out.push('{');
            out.push_str(&name);
            out.push('}');
        }
    }

    Some(out)
}

fn is_simple_placeholder_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
