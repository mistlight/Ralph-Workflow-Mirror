fn determine_next_effect_for_phase(state: &PipelineState) -> Effect {
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

            let consumer_signature_sha256 = state.agent_chain.consumer_signature_sha256();

            // Clean up BEFORE planning to remove old PLAN.md from previous iteration
            if !state.context_cleaned {
                return Effect::CleanupContext;
            }

            if state.planning_prompt_prepared_iteration != Some(state.iteration) {
                let planning_inputs_materialized_for_iteration =
                    state.prompt_inputs.planning.as_ref().is_some_and(|p| {
                        p.iteration == state.iteration
                            && p.prompt.consumer_signature_sha256 == consumer_signature_sha256
                    });
                if !planning_inputs_materialized_for_iteration {
                    return Effect::MaterializePlanningInputs {
                        iteration: state.iteration,
                    };
                }
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

            let consumer_signature_sha256 = state.agent_chain.consumer_signature_sha256();

            if state.iteration < state.total_iterations {
                if state.development_context_prepared_iteration != Some(state.iteration) {
                    return Effect::PrepareDevelopmentContext {
                        iteration: state.iteration,
                    };
                }

                if state.development_prompt_prepared_iteration != Some(state.iteration) {
                    let development_inputs_materialized_for_iteration =
                        state.prompt_inputs.development.as_ref().is_some_and(|p| {
                            p.iteration == state.iteration
                                && p.prompt.consumer_signature_sha256 == consumer_signature_sha256
                                && p.plan.consumer_signature_sha256 == consumer_signature_sha256
                        });
                    if !development_inputs_materialized_for_iteration {
                        return Effect::MaterializeDevelopmentInputs {
                            iteration: state.iteration,
                        };
                    }

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

                // After EVERY development iteration, invoke analysis agent to verify results
                // Analysis agent produces development_result.xml by comparing git diff vs PLAN.md
                // This runs AFTER InvokeDevelopmentAgent completes (checked via development_agent_invoked_iteration)
                // and BEFORE ExtractDevelopmentXml (checked via analysis_agent_invoked_iteration)
                if state.development_agent_invoked_iteration == Some(state.iteration)
                    && state.analysis_agent_invoked_iteration != Some(state.iteration)
                {
                    return Effect::InvokeAnalysisAgent {
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

            let consumer_signature_sha256 = state.agent_chain.consumer_signature_sha256();

            // Otherwise, run next review pass or complete phase
            if state.reviewer_pass < state.total_reviewer_passes {
                if state.review_context_prepared_pass != Some(state.reviewer_pass) {
                    return Effect::PrepareReviewContext {
                        pass: state.reviewer_pass,
                    };
                }

                if state.review_prompt_prepared_pass != Some(state.reviewer_pass) {
                    let review_inputs_materialized_for_pass =
                        state.prompt_inputs.review.as_ref().is_some_and(|p| {
                            p.pass == state.reviewer_pass
                                && p.plan.consumer_signature_sha256 == consumer_signature_sha256
                                && p.diff.consumer_signature_sha256 == consumer_signature_sha256
                        });
                    if !review_inputs_materialized_for_pass {
                        return Effect::MaterializeReviewInputs {
                            pass: state.reviewer_pass,
                        };
                    }
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
                    let current_attempt = match state.commit {
                        CommitState::Generating { attempt, .. } => attempt,
                        _ => 1,
                    };
                    if let Some(outcome) = state.commit_validated_outcome.as_ref() {
                        if outcome.attempt == current_attempt && state.commit_xml_extracted {
                            return Effect::ApplyCommitMessageOutcome;
                        }
                    }

                    // Once the prompt is prepared, retry flows should not require rematerializing
                    // inputs (or re-checking the diff) before re-cleaning XML and reinvoking.
                    // The prompt file on disk is the source of truth for invocation.
                    if state.commit_prompt_prepared {
                        // IMPORTANT: For commit XSD retries, the agent must be able to read the
                        // previous invalid output at `.agent/tmp/commit_message.xml` before overwriting
                        // it (see commit_xsd_retry prompt). Therefore, skip cleanup on retry attempts.
                        if current_attempt == 1 && !state.commit_xml_cleaned {
                            return Effect::CleanupCommitXml;
                        }
                        if !state.commit_agent_invoked {
                            return Effect::InvokeCommitAgent;
                        }
                        if !state.commit_xml_extracted {
                            return Effect::ExtractCommitXml;
                        }
                        return Effect::ValidateCommitXml;
                    }

                    if !state.commit_diff_prepared {
                        return Effect::CheckCommitDiff;
                    }
                    if state.commit_diff_empty {
                        return Effect::SkipCommit {
                            reason: "No changes to commit (empty diff)".to_string(),
                        };
                    }
                    // Backward compatibility / recoverability: older checkpoints may have
                    // `commit_diff_prepared = true` but no recorded content id. Re-run diff
                    // preparation once to establish `commit_diff_content_id_sha256`, which is
                    // required to safely guard against stale materialized prompt inputs.
                    if state.commit_diff_content_id_sha256.is_none() {
                        return Effect::CheckCommitDiff;
                    }
                    let current_attempt = match state.commit {
                        CommitState::Generating { attempt, .. } => attempt,
                        _ => 1,
                    };
                    let consumer_signature_sha256 = state.agent_chain.consumer_signature_sha256();
                    let diff_content_id_sha256 = state.commit_diff_content_id_sha256.as_deref();
                    if !state.commit_prompt_prepared {
                        let commit_inputs_materialized_for_attempt =
                            state.prompt_inputs.commit.as_ref().is_some_and(|c| {
                                c.attempt == current_attempt
                                    && c.diff.consumer_signature_sha256 == consumer_signature_sha256
                                    && diff_content_id_sha256
                                        .is_some_and(|id| id == c.diff.content_id_sha256)
                            });
                        if !commit_inputs_materialized_for_attempt {
                            return Effect::MaterializeCommitInputs {
                                attempt: current_attempt,
                            };
                        }
                        return Effect::PrepareCommitPrompt {
                            prompt_mode: PromptMode::Normal,
                        };
                    }
                    // Prompt-prepared flow is handled above.
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
