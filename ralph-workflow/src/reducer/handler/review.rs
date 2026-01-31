use super::util::{parse_issue_location, read_snippet_for_issue};
use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::files::llm_output_extraction::validate_issues_xml;
use crate::phases::{review, PhaseContext};
use crate::prompts::ContextLevel;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentErrorKind, PipelineEvent};
use crate::reducer::ui_event::{UIEvent, XmlCodeSnippet, XmlOutputContext, XmlOutputType};
use crate::workspace::Workspace;
use anyhow::Result;

impl MainEffectHandler {
    pub(super) fn prepare_review_context(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::{create_prompt_backup_with_workspace, write_diff_backup_with_workspace};

        let _ = create_prompt_backup_with_workspace(ctx.workspace);

        let diff = match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace) {
            Ok((diff, _baseline_oid)) => diff,
            Err(err) => {
                ctx.logger
                    .warn(&format!("Failed to compute review diff: {err}"));
                String::new()
            }
        };
        let _ = write_diff_backup_with_workspace(ctx.workspace, &diff);

        Ok(EffectResult::event(PipelineEvent::review_context_prepared(
            pass,
        )))
    }

    pub(super) fn prepare_review_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_review_xml_with_references, PromptContentBuilder,
        };
        use std::path::Path;

        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir)?;
        }

        let plan_content = ctx
            .workspace
            .read(Path::new(".agent/PLAN.md"))
            .unwrap_or_default();
        let diff_content = ctx
            .workspace
            .read(Path::new(".agent/DIFF.backup"))
            .unwrap_or_default();

        let baseline_oid_for_prompts = String::new();

        let prompt_key = format!("review_{pass}");
        let (review_prompt_xml, was_replayed) =
            get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                let refs = PromptContentBuilder::new(ctx.workspace)
                    .with_plan(plan_content.clone())
                    .with_diff(diff_content.clone(), &baseline_oid_for_prompts)
                    .build();

                prompt_review_xml_with_references(ctx.template_context, &refs)
            });

        if let Err(err) = crate::prompts::validate_no_unresolved_placeholders(&review_prompt_xml) {
            return Err(crate::prompts::TemplateVariablesInvalidError {
                template_name: "review_xml".to_string(),
                missing_variables: Vec::new(),
                unresolved_placeholders: err.unresolved_placeholders,
            }
            .into());
        }

        if !was_replayed {
            ctx.capture_prompt(&prompt_key, &review_prompt_xml);
        }

        ctx.workspace.write(
            Path::new(".agent/tmp/review_prompt.txt"),
            &review_prompt_xml,
        )?;

        Ok(EffectResult::event(PipelineEvent::review_prompt_prepared(
            pass,
        )))
    }

    pub(super) fn invoke_review_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use std::path::Path;

        let prompt = ctx
            .workspace
            .read(Path::new(".agent/tmp/review_prompt.txt"))
            .unwrap_or_default();

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.reviewer_agent.to_string());

        let mut result = self.invoke_agent(ctx, AgentRole::Reviewer, agent, None, prompt)?;
        result = result.with_additional_event(PipelineEvent::review_agent_invoked(pass));
        Ok(result)
    }

    pub(super) fn extract_review_issues_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        // Only the canonical path is considered input. Archived `.processed` files
        // are debug artifacts and must not be used as fallback inputs.
        let issues_xml = Path::new(xml_paths::ISSUES_XML);
        let content = ctx.workspace.read(issues_xml);

        match content {
            Ok(_) => Ok(EffectResult::event(
                PipelineEvent::review_issues_xml_extracted(pass),
            )),
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::review_output_validation_failed(
                    pass,
                    self.state.continuation.invalid_output_attempts,
                ),
            )),
        }
    }

    pub(super) fn validate_review_issues_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use crate::files::llm_output_extraction::validate_issues_xml;
        use std::path::Path;

        let issues_xml = ctx.workspace.read(Path::new(xml_paths::ISSUES_XML));
        let issues_xml = match issues_xml {
            Ok(s) => s,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::review_output_validation_failed(
                        pass,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ));
            }
        };

        match validate_issues_xml(&issues_xml) {
            Ok(elements) => {
                let issues_found = !elements.issues.is_empty();
                let clean_no_issues =
                    elements.no_issues_found.is_some() && elements.issues.is_empty();
                Ok(EffectResult::event(
                    PipelineEvent::review_issues_xml_validated(pass, issues_found, clean_no_issues),
                ))
            }
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::review_output_validation_failed(
                    pass,
                    self.state.continuation.invalid_output_attempts,
                ),
            )),
        }
    }

    pub(super) fn write_issues_markdown(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use crate::files::llm_output_extraction::validate_issues_xml;
        use std::path::Path;

        let issues_xml = match ctx.workspace.read(Path::new(xml_paths::ISSUES_XML)) {
            Ok(s) => s,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::review_output_validation_failed(
                        pass,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ));
            }
        };

        let elements = match validate_issues_xml(&issues_xml) {
            Ok(e) => e,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::review_output_validation_failed(
                        pass,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ));
            }
        };

        let markdown = render_issues_markdown(&elements);
        ctx.workspace
            .write(Path::new(".agent/ISSUES.md"), &markdown)?;

        Ok(EffectResult::event(
            PipelineEvent::review_issues_markdown_written(pass),
        ))
    }

    pub(super) fn archive_review_issues_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::ISSUES_XML));
        Ok(EffectResult::event(
            PipelineEvent::review_issues_xml_archived(pass),
        ))
    }

    pub(super) fn apply_review_outcome(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        pass: u32,
        issues_found: bool,
        clean_no_issues: bool,
    ) -> Result<EffectResult> {
        if clean_no_issues {
            return Ok(EffectResult::event(
                PipelineEvent::review_pass_completed_clean(pass),
            ));
        }
        Ok(EffectResult::event(PipelineEvent::review_completed(
            pass,
            issues_found,
        )))
    }

    pub(super) fn run_review_pass(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        let review_label = format!("review_{}", pass);

        // Get current reviewer agent from agent chain
        let review_agent = self.state.agent_chain.current_agent().cloned();

        // Keep invalid-output attempt tracking deterministic by sourcing it from state.
        let invalid_output_attempt = self.state.continuation.invalid_output_attempts;

        match review::run_review_pass(ctx, pass, &review_label, "", review_agent.as_deref()) {
            Ok(result) => {
                // Check for auth failure - trigger agent fallback
                if result.auth_failure {
                    let current_agent = review_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                if result.agent_failed {
                    let current_agent = review_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::InternalError,
                        false,
                    )));
                }

                if !result.output_valid {
                    return Ok(EffectResult::event(
                        PipelineEvent::review_output_validation_failed(
                            pass,
                            invalid_output_attempt,
                        ),
                    ));
                }

                let event = if result.issues_found {
                    PipelineEvent::review_completed(pass, true)
                } else if result.early_exit {
                    PipelineEvent::review_pass_completed_clean(pass)
                } else {
                    PipelineEvent::review_completed(pass, false)
                };

                // Build UI events
                let mut ui_events = vec![UIEvent::ReviewProgress {
                    pass,
                    total: self.state.total_reviewer_passes,
                }];

                if let Some(xml_content) = result.xml_content.as_deref() {
                    ui_events.push(build_review_issues_ui_event(
                        ctx.workspace,
                        pass,
                        xml_content,
                    ));
                }

                Ok(EffectResult::with_ui(event, ui_events))
            }
            Err(err) => {
                ctx.logger.warn(&format!(
                    "Review pass {} failed for agent {:?}: {}",
                    pass,
                    review_agent.as_deref().unwrap_or("(none)"),
                    err
                ));

                if let Some(tpl_err) =
                    err.downcast_ref::<crate::prompts::TemplateVariablesInvalidError>()
                {
                    return Ok(EffectResult::event(
                        PipelineEvent::agent_template_variables_invalid(
                            AgentRole::Reviewer,
                            tpl_err.template_name.clone(),
                            tpl_err.missing_variables.clone(),
                            tpl_err.unresolved_placeholders.clone(),
                        ),
                    ));
                }

                if Self::is_auth_failure(&err) {
                    let current_agent = review_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                let current_agent = review_agent.unwrap_or_else(|| "unknown".to_string());
                Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                    AgentRole::Reviewer,
                    current_agent,
                    1,
                    AgentErrorKind::InternalError,
                    false,
                )))
            }
        }
    }

    pub(super) fn run_fix_attempt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::checkpoint::restore::ResumeContext;

        let reviewer_context = ContextLevel::from(ctx.config.reviewer_context);

        // Get current reviewer agent from agent chain
        let fix_agent = self.state.agent_chain.current_agent().cloned();

        match review::run_fix_pass(
            ctx,
            pass,
            reviewer_context,
            None::<&ResumeContext>,
            fix_agent.as_deref(),
        ) {
            Ok(result) => {
                if result.auth_failure {
                    let current_agent = fix_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                if result.agent_failed {
                    let current_agent = fix_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::InternalError,
                        false,
                    )));
                }

                if !result.output_valid {
                    return Ok(EffectResult::event(
                        PipelineEvent::fix_output_validation_failed(
                            pass,
                            self.state.continuation.invalid_output_attempts,
                        ),
                    ));
                }

                // Output is valid: interpret fix status and emit a reducer event describing
                // completion vs. continuation requirement.
                let status = result
                    .status
                    .as_deref()
                    .and_then(crate::reducer::state::FixStatus::parse)
                    .unwrap_or(crate::reducer::state::FixStatus::Failed);

                let event = if status.needs_continuation() {
                    // Decide between triggering another continuation vs. reporting budget exhaustion.
                    let next_attempt = self.state.continuation.fix_continuation_attempt + 1;
                    if next_attempt >= self.state.continuation.max_fix_continue_count {
                        PipelineEvent::fix_continuation_budget_exhausted(
                            pass,
                            self.state.continuation.fix_continuation_attempt + 1,
                            status,
                        )
                    } else {
                        PipelineEvent::fix_continuation_triggered(pass, status, result.summary)
                    }
                } else if self.state.continuation.fix_continuation_attempt > 0 {
                    // We were in a fix continuation chain and have now reached a complete status.
                    PipelineEvent::fix_continuation_succeeded(
                        pass,
                        self.state.continuation.fix_continuation_attempt + 1,
                    )
                } else {
                    // First attempt completed with a complete status.
                    PipelineEvent::fix_attempt_completed(pass, result.changes_made)
                };

                let mut ui_events = vec![];
                if let Some(xml_content) = result.xml_content.as_deref() {
                    ui_events.push(UIEvent::XmlOutput {
                        xml_type: XmlOutputType::FixResult,
                        content: xml_content.to_string(),
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets: Vec::new(),
                        }),
                    });
                }

                Ok(EffectResult::with_ui(event, ui_events))
            }
            Err(err) => {
                if let Some(tpl_err) =
                    err.downcast_ref::<crate::prompts::TemplateVariablesInvalidError>()
                {
                    return Ok(EffectResult::event(
                        PipelineEvent::agent_template_variables_invalid(
                            AgentRole::Reviewer,
                            tpl_err.template_name.clone(),
                            tpl_err.missing_variables.clone(),
                            tpl_err.unresolved_placeholders.clone(),
                        ),
                    ));
                }

                if Self::is_auth_failure(&err) {
                    let current_agent = fix_agent.unwrap_or_else(|| "unknown".to_string());
                    return Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                        AgentRole::Reviewer,
                        current_agent,
                        1,
                        AgentErrorKind::Authentication,
                        false,
                    )));
                }

                let current_agent = fix_agent.unwrap_or_else(|| "unknown".to_string());
                Ok(EffectResult::event(PipelineEvent::agent_invocation_failed(
                    AgentRole::Reviewer,
                    current_agent,
                    1,
                    AgentErrorKind::InternalError,
                    false,
                )))
            }
        }
    }
}

fn collect_review_issue_snippets(
    workspace: &dyn Workspace,
    issues_xml: &str,
) -> Vec<XmlCodeSnippet> {
    let validated = match validate_issues_xml(issues_xml) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut snippets = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for issue in validated.issues {
        if let Some((file, issue_start, issue_end)) = parse_issue_location(&issue) {
            if let Some(snippet) = read_snippet_for_issue(workspace, &file, issue_start, issue_end)
            {
                let key = (
                    snippet.file.clone(),
                    snippet.line_start,
                    snippet.line_end,
                    snippet.content.clone(),
                );
                if seen.insert(key) {
                    snippets.push(snippet);
                }
            }
        }
    }

    snippets
}

fn build_review_issues_ui_event(
    workspace: &dyn Workspace,
    pass: u32,
    xml_content: &str,
) -> UIEvent {
    let snippets = collect_review_issue_snippets(workspace, xml_content);
    UIEvent::XmlOutput {
        xml_type: XmlOutputType::ReviewIssues,
        content: xml_content.to_string(),
        context: Some(XmlOutputContext {
            iteration: None,
            pass: Some(pass),
            snippets,
        }),
    }
}

fn render_issues_markdown(
    elements: &crate::files::llm_output_extraction::IssuesElements,
) -> String {
    let mut output = String::from("# Issues\n\n");

    if let Some(message) = &elements.no_issues_found {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            output.push_str("No issues found.\n");
        } else {
            output.push_str(trimmed);
            output.push('\n');
        }
        return output;
    }

    if elements.issues.is_empty() {
        output.push_str("No issues found.\n");
        return output;
    }

    for issue in &elements.issues {
        let trimmed = issue.trim();
        if trimmed.is_empty() {
            continue;
        }
        output.push_str("- [ ] ");
        output.push_str(trimmed);
        output.push('\n');
    }

    output
}
