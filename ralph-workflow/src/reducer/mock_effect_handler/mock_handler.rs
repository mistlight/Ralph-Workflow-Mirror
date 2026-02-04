// MockEffectHandler implementation (the handler struct and trait implementation).
//
// This file contains the execute_mock method and EffectHandler/StatefulHandler
// trait implementations for MockEffectHandler.

impl MockEffectHandler {
    /// Execute an effect without requiring PhaseContext.
    ///
    /// This is used for testing when you don't have a full PhaseContext.
    /// It captures the effect and returns an appropriate mock EffectResult.
    pub fn execute_mock(&mut self, effect: Effect) -> EffectResult {
        // Capture the effect
        self.captured_effects.borrow_mut().push(effect.clone());

        // Generate appropriate mock events based on effect type
        let additional_events = Vec::new();
        let (event, ui_events) = match effect {
            Effect::AgentInvocation {
                role,
                agent,
                model: _,
                prompt: _,
            } => {
                let ui = vec![UIEvent::AgentActivity {
                    agent: agent.clone(),
                    message: format!("Completed {} task", role),
                }];
                (PipelineEvent::agent_invocation_succeeded(role, agent), ui)
            }

            Effect::InitializeAgentChain { role } => {
                // Emit phase transition when initializing agent chain for a new phase
                let ui = match role {
                    crate::agents::AgentRole::Developer
                        if self.state.phase == PipelinePhase::Planning =>
                    {
                        vec![UIEvent::PhaseTransition {
                            from: None,
                            to: PipelinePhase::Planning,
                        }]
                    }
                    crate::agents::AgentRole::Reviewer
                        if self.state.phase == PipelinePhase::Review =>
                    {
                        vec![UIEvent::PhaseTransition {
                            from: Some(self.state.phase),
                            to: PipelinePhase::Review,
                        }]
                    }
                    _ => vec![],
                };
                (
                    PipelineEvent::agent_chain_initialized(
                        role,
                        vec!["mock_agent".to_string()],
                        3,
                        1000,
                        2.0,
                        60000,
                    ),
                    ui,
                )
            }

            Effect::PreparePlanningPrompt {
                iteration,
                prompt_mode: _,
            } => {
                (PipelineEvent::planning_prompt_prepared(iteration), vec![])
            }

            Effect::MaterializePlanningInputs { iteration } => (
                PipelineEvent::planning_inputs_materialized(
                    iteration,
                    crate::reducer::state::MaterializedPromptInput {
                        kind: crate::reducer::state::PromptInputKind::Prompt,
                        content_id_sha256: "id".to_string(),
                        consumer_signature_sha256: self.state.agent_chain.consumer_signature_sha256(),
                        original_bytes: 1,
                        final_bytes: 1,
                        model_budget_bytes: None,
                        inline_budget_bytes: None,
                        representation: crate::reducer::state::PromptInputRepresentation::Inline,
                        reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
                    },
                ),
                vec![],
            ),

            Effect::CleanupPlanningXml { iteration } => {
                (PipelineEvent::planning_xml_cleaned(iteration), vec![])
            }

            Effect::InvokePlanningAgent { iteration } => {
                (PipelineEvent::planning_agent_invoked(iteration), vec![])
            }

            Effect::ExtractPlanningXml { iteration } => {
                (PipelineEvent::planning_xml_extracted(iteration), vec![])
            }

            Effect::ValidatePlanningXml { iteration } => {
                let mock_plan_xml = r#"<ralph-plan>
 <ralph-summary>
 <context>Mock plan for testing</context>
 <scope-items>
 <scope-item count="1">test item</scope-item>
 <scope-item count="1">another item</scope-item>
 <scope-item count="1">third item</scope-item>
 </scope-items>
 </ralph-summary>
 <ralph-implementation-steps>
 <step number="1" type="file-change">
 <title>Mock step</title>
 <target-files><file path="src/test.rs" action="modify"/></target-files>
 <content><paragraph>Test content</paragraph></content>
 </step>
 </ralph-implementation-steps>
 <ralph-critical-files>
 <primary-files><file path="src/test.rs" action="modify"/></primary-files>
 <reference-files><file path="src/lib.rs" purpose="reference"/></reference-files>
 </ralph-critical-files>
 <ralph-risks-mitigations>
 <risk-pair severity="low"><risk>Test risk</risk><mitigation>Test mitigation</mitigation></risk-pair>
 </ralph-risks-mitigations>
 <ralph-verification-strategy>
 <verification><method>Test method</method><expected-outcome>Pass</expected-outcome></verification>
 </ralph-verification-strategy>
 </ralph-plan>"#;
                let ui = vec![UIEvent::XmlOutput {
                    xml_type: XmlOutputType::DevelopmentPlan,
                    content: mock_plan_xml.to_string(),
                    context: Some(XmlOutputContext {
                        iteration: Some(iteration),
                        pass: None,
                        snippets: Vec::new(),
                    }),
                }];
                let markdown = "# Plan\n\n- Mock step\n".to_string();
                (
                    PipelineEvent::planning_xml_validated(iteration, true, Some(markdown)),
                    ui,
                )
            }

            Effect::WritePlanningMarkdown { iteration } => {
                (PipelineEvent::planning_markdown_written(iteration), vec![])
            }

            Effect::ArchivePlanningXml { iteration } => {
                (PipelineEvent::planning_xml_archived(iteration), vec![])
            }

            Effect::ApplyPlanningOutcome { iteration, valid } => {
                let mut ui = Vec::new();
                if valid {
                    ui.push(UIEvent::PhaseTransition {
                        from: Some(self.state.phase),
                        to: PipelinePhase::Development,
                    });
                }
                (PipelineEvent::plan_generation_completed(iteration, valid), ui)
            }

            Effect::PrepareDevelopmentContext { iteration } => {
                (PipelineEvent::development_context_prepared(iteration), vec![])
            }

            Effect::MaterializeDevelopmentInputs { iteration } => {
                let prompt = crate::reducer::state::MaterializedPromptInput {
                    kind: crate::reducer::state::PromptInputKind::Prompt,
                    content_id_sha256: "id".to_string(),
                    consumer_signature_sha256: self.state.agent_chain.consumer_signature_sha256(),
                    original_bytes: 1,
                    final_bytes: 1,
                    model_budget_bytes: None,
                    inline_budget_bytes: None,
                    representation: crate::reducer::state::PromptInputRepresentation::Inline,
                    reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
                };
                let plan = crate::reducer::state::MaterializedPromptInput {
                    kind: crate::reducer::state::PromptInputKind::Plan,
                    content_id_sha256: "id".to_string(),
                    consumer_signature_sha256: self.state.agent_chain.consumer_signature_sha256(),
                    original_bytes: 1,
                    final_bytes: 1,
                    model_budget_bytes: None,
                    inline_budget_bytes: None,
                    representation: crate::reducer::state::PromptInputRepresentation::Inline,
                    reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
                };
                (
                    PipelineEvent::development_inputs_materialized(iteration, prompt, plan),
                    vec![],
                )
            }

            Effect::PrepareDevelopmentPrompt {
                iteration,
                prompt_mode: _,
            } => {
                (PipelineEvent::development_prompt_prepared(iteration), vec![])
            }

            Effect::CleanupDevelopmentXml { iteration } => {
                (PipelineEvent::development_xml_cleaned(iteration), vec![])
            }

            Effect::InvokeDevelopmentAgent { iteration } => {
                (PipelineEvent::development_agent_invoked(iteration), vec![])
            }

            Effect::ExtractDevelopmentXml { iteration } => {
                let mock_dev_result_xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Mock development iteration completed successfully</ralph-summary>
<ralph-files-changed>src/test.rs
src/lib.rs</ralph-files-changed>
</ralph-development-result>"#;
                let ui = vec![
                    UIEvent::IterationProgress {
                        current: iteration,
                        total: self.state.total_iterations,
                    },
                    UIEvent::XmlOutput {
                        xml_type: XmlOutputType::DevelopmentResult,
                        content: mock_dev_result_xml.to_string(),
                        context: Some(XmlOutputContext {
                            iteration: Some(iteration),
                            pass: None,
                            snippets: Vec::new(),
                        }),
                    },
                ];
                (PipelineEvent::development_xml_extracted(iteration), ui)
            }

            Effect::ValidateDevelopmentXml { iteration } => (
                PipelineEvent::development_xml_validated(
                    iteration,
                    crate::reducer::state::DevelopmentStatus::Completed,
                    "Mock development iteration completed successfully".to_string(),
                    Some(vec!["src/test.rs".to_string(), "src/lib.rs".to_string()]),
                    None,
                ),
                vec![],
            ),

            Effect::ArchiveDevelopmentXml { iteration } => {
                (PipelineEvent::development_xml_archived(iteration), vec![])
            }

            Effect::ApplyDevelopmentOutcome { iteration } => (
                PipelineEvent::development_outcome_applied(iteration),
                vec![],
            ),

            Effect::PrepareReviewContext { pass } => {
                (
                    PipelineEvent::review_context_prepared(pass),
                    vec![UIEvent::ReviewProgress {
                        pass,
                        total: self.state.total_reviewer_passes,
                    }],
                )
            }

            Effect::MaterializeReviewInputs { pass } => {
                let plan = crate::reducer::state::MaterializedPromptInput {
                    kind: crate::reducer::state::PromptInputKind::Plan,
                    content_id_sha256: "id".to_string(),
                    consumer_signature_sha256: self.state.agent_chain.consumer_signature_sha256(),
                    original_bytes: 1,
                    final_bytes: 1,
                    model_budget_bytes: None,
                    inline_budget_bytes: None,
                    representation: crate::reducer::state::PromptInputRepresentation::Inline,
                    reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
                };
                let diff = crate::reducer::state::MaterializedPromptInput {
                    kind: crate::reducer::state::PromptInputKind::Diff,
                    content_id_sha256: "id".to_string(),
                    consumer_signature_sha256: self.state.agent_chain.consumer_signature_sha256(),
                    original_bytes: 1,
                    final_bytes: 1,
                    model_budget_bytes: None,
                    inline_budget_bytes: None,
                    representation: crate::reducer::state::PromptInputRepresentation::Inline,
                    reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
                };
                (
                    PipelineEvent::review_inputs_materialized(pass, plan, diff),
                    vec![],
                )
            }

            Effect::PrepareReviewPrompt {
                pass,
                prompt_mode: _,
            } => {
                (PipelineEvent::review_prompt_prepared(pass), vec![])
            }

            Effect::CleanupReviewIssuesXml { pass } => {
                (PipelineEvent::review_issues_xml_cleaned(pass), vec![])
            }

            Effect::InvokeReviewAgent { pass } => {
                // In mock mode we only emit the review-specific progress event.
                (PipelineEvent::review_agent_invoked(pass), vec![])
            }

            Effect::ExtractReviewIssuesXml { pass } => {
                (PipelineEvent::review_issues_xml_extracted(pass), vec![])
            }

            Effect::ValidateReviewIssuesXml { pass } => {
                (
                    PipelineEvent::review_issues_xml_validated(
                        pass,
                        false,
                        true,
                        Vec::new(),
                        Some("ok".to_string()),
                    ),
                    vec![UIEvent::XmlOutput {
                        xml_type: XmlOutputType::ReviewIssues,
                        content: r#"<ralph-issues><ralph-no-issues-found>ok</ralph-no-issues-found></ralph-issues>"#
                            .to_string(),
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets: Vec::new(),
                        }),
                    }],
                )
            }

            Effect::WriteIssuesMarkdown { pass } => {
                (PipelineEvent::review_issues_markdown_written(pass), vec![])
            }

            Effect::ExtractReviewIssueSnippets { pass } => (
                PipelineEvent::review_issue_snippets_extracted(pass),
                vec![UIEvent::XmlOutput {
                    xml_type: XmlOutputType::ReviewIssues,
                    content: r#"<ralph-issues><ralph-no-issues-found>ok</ralph-no-issues-found></ralph-issues>"#
                        .to_string(),
                    context: Some(XmlOutputContext {
                        iteration: None,
                        pass: Some(pass),
                        snippets: Vec::new(),
                    }),
                }],
            ),

            Effect::ArchiveReviewIssuesXml { pass } => {
                (PipelineEvent::review_issues_xml_archived(pass), vec![])
            }

            Effect::ApplyReviewOutcome {
                pass,
                issues_found,
                clean_no_issues,
            } => {
                if clean_no_issues {
                    (PipelineEvent::review_pass_completed_clean(pass), vec![])
                } else {
                    (PipelineEvent::review_completed(pass, issues_found), vec![])
                }
            }

            Effect::PrepareFixPrompt {
                pass,
                prompt_mode: _,
            } => (PipelineEvent::fix_prompt_prepared(pass), vec![]),

            Effect::CleanupFixResultXml { pass } => {
                (PipelineEvent::fix_result_xml_cleaned(pass), vec![])
            }

            Effect::InvokeFixAgent { pass } => (PipelineEvent::fix_agent_invoked(pass), vec![]),

            Effect::ExtractFixResultXml { pass } => {
                (PipelineEvent::fix_result_xml_extracted(pass), vec![])
            }

            Effect::ValidateFixResultXml { pass } => (
                PipelineEvent::fix_result_xml_validated(
                    pass,
                    crate::reducer::state::FixStatus::AllIssuesAddressed,
                    None,
                ),
                vec![UIEvent::XmlOutput {
                    xml_type: XmlOutputType::FixResult,
                    content: r#"<ralph-fix-result><ralph-status>all_issues_addressed</ralph-status></ralph-fix-result>"#
                        .to_string(),
                    context: Some(XmlOutputContext {
                        iteration: None,
                        pass: Some(pass),
                        snippets: Vec::new(),
                    }),
                }],
            ),

            Effect::ApplyFixOutcome { pass } => {
                (PipelineEvent::fix_outcome_applied(pass), vec![])
            }

            Effect::ArchiveFixResultXml { pass } => {
                (PipelineEvent::fix_result_xml_archived(pass), vec![])
            }

            Effect::RunRebase {
                phase,
                target_branch: _,
            } => (
                PipelineEvent::rebase_succeeded(phase, "mock_head_abc123".to_string()),
                vec![],
            ),

            Effect::ResolveRebaseConflicts { strategy: _ } => {
                (PipelineEvent::rebase_conflict_resolved(vec![]), vec![])
            }

            Effect::CheckCommitDiff => {
                let empty = self.simulate_empty_diff;
                (
                    PipelineEvent::commit_diff_prepared(empty, "id".to_string()),
                    vec![],
                )
            }

            Effect::MaterializeCommitInputs { attempt } => (
                PipelineEvent::commit_inputs_materialized(
                    attempt,
                    crate::reducer::state::MaterializedPromptInput {
                        kind: crate::reducer::state::PromptInputKind::Diff,
                        content_id_sha256: "id".to_string(),
                        consumer_signature_sha256: self.state.agent_chain.consumer_signature_sha256(),
                        original_bytes: 1,
                        final_bytes: 1,
                        model_budget_bytes: None,
                        inline_budget_bytes: None,
                        representation: crate::reducer::state::PromptInputRepresentation::Inline,
                        reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
                    },
                ),
                vec![],
            ),

            Effect::PrepareCommitPrompt { prompt_mode: _ } => {
                let attempt = match self.state.commit {
                    crate::reducer::state::CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                let ui = vec![UIEvent::PhaseTransition {
                    from: Some(self.state.phase),
                    to: PipelinePhase::CommitMessage,
                }];
                (PipelineEvent::commit_prompt_prepared(attempt), ui)
            }

            Effect::InvokeCommitAgent => {
                let attempt = match self.state.commit {
                    crate::reducer::state::CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                (PipelineEvent::commit_agent_invoked(attempt), vec![])
            }

            Effect::CleanupCommitXml => {
                let attempt = match self.state.commit {
                    crate::reducer::state::CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                (PipelineEvent::commit_xml_cleaned(attempt), vec![])
            }

            Effect::ExtractCommitXml => {
                let attempt = match self.state.commit {
                    crate::reducer::state::CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                (PipelineEvent::commit_xml_extracted(attempt), vec![])
            }

            Effect::ValidateCommitXml => {
                let attempt = match self.state.commit {
                    crate::reducer::state::CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                let mock_commit_xml = r#"<ralph-commit>
<ralph-subject>feat: mock commit message for testing</ralph-subject>
<ralph-body>This is a mock commit body generated for testing purposes.

- Changed some files
- Added new features</ralph-body>
</ralph-commit>"#;
                let ui = vec![UIEvent::XmlOutput {
                    xml_type: XmlOutputType::CommitMessage,
                    content: mock_commit_xml.to_string(),
                    context: None,
                }];
                (
                    PipelineEvent::commit_xml_validated(
                        "mock commit message".to_string(),
                        attempt,
                    ),
                    ui,
                )
            }

            Effect::ApplyCommitMessageOutcome => {
                let event = match self.state.commit_validated_outcome.as_ref() {
                    Some(outcome) => match (&outcome.message, &outcome.reason) {
                        (Some(message), _) => PipelineEvent::commit_message_generated(
                            message.clone(),
                            outcome.attempt,
                        ),
                        (None, Some(reason)) => PipelineEvent::commit_message_validation_failed(
                            reason.clone(),
                            outcome.attempt,
                        ),
                        _ => PipelineEvent::commit_generation_failed(
                            "Mock commit outcome missing message and reason".to_string(),
                        ),
                    },
                    None => PipelineEvent::commit_generation_failed(
                        "Mock commit outcome missing".to_string(),
                    ),
                };
                (event, vec![])
            }

            Effect::ArchiveCommitXml => {
                let attempt = match self.state.commit {
                    crate::reducer::state::CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                (PipelineEvent::commit_xml_archived(attempt), vec![])
            }

            Effect::CreateCommit { message } => (
                PipelineEvent::commit_created("mock_commit_hash_abc123".to_string(), message),
                vec![],
            ),

            Effect::SkipCommit { reason } => (PipelineEvent::commit_skipped(reason), vec![]),

            Effect::BackoffWait {
                role,
                cycle,
                duration_ms: _,
            } => (
                PipelineEvent::agent_retry_cycle_started(role, cycle),
                vec![],
            ),

            Effect::AbortPipeline { reason } => {
                panic!("MockEffectHandler received AbortPipeline effect: {reason}")
            }

            Effect::ValidateFinalState => {
                let ui = vec![UIEvent::PhaseTransition {
                    from: Some(self.state.phase),
                    to: PipelinePhase::Finalizing,
                }];
                (PipelineEvent::finalizing_started(), ui)
            }

            Effect::SaveCheckpoint { trigger } => {
                (PipelineEvent::checkpoint_saved(trigger), vec![])
            }

            Effect::CleanupContext => (PipelineEvent::context_cleaned(), vec![]),

            Effect::RestorePromptPermissions => {
                let ui = vec![UIEvent::PhaseTransition {
                    from: Some(self.state.phase),
                    to: PipelinePhase::Complete,
                }];
                (PipelineEvent::prompt_permissions_restored(), ui)
            }

            Effect::WriteContinuationContext(ref data) => (
                PipelineEvent::development_continuation_context_written(
                    data.iteration,
                    data.attempt,
                ),
                vec![],
            ),

            Effect::CleanupContinuationContext => (
                PipelineEvent::development_continuation_context_cleaned(),
                vec![],
            ),
        };

        // Capture UI events
        self.captured_ui_events
            .borrow_mut()
            .extend(ui_events.clone());

        EffectResult {
            event,
            additional_events,
            ui_events,
        }
    }
}

/// Implement the EffectHandler trait for MockEffectHandler.
///
/// This allows MockEffectHandler to be used as a drop-in replacement for
/// MainEffectHandler in tests. The PhaseContext is ignored - the mock
/// simply captures the effect and returns an appropriate mock event.
impl<'ctx> EffectHandler<'ctx> for MockEffectHandler {
    fn execute(&mut self, effect: Effect, _ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        match effect {
            Effect::AbortPipeline { reason } => Err(anyhow::anyhow!(reason)),
            _ => Ok(self.execute_mock(effect)),
        }
    }
}

/// Implement StatefulHandler for MockEffectHandler.
///
/// This allows the event loop to update the mock's internal state after
/// each event is processed.
impl crate::app::event_loop::StatefulHandler for MockEffectHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
    }
}
