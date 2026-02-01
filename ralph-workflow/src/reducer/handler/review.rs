use super::MainEffectHandler;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentEvent, PipelineEvent};
use crate::reducer::ui_event::{UIEvent, XmlCodeSnippet, XmlOutputContext, XmlOutputType};
use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

impl MainEffectHandler {
    const DIFF_BASELINE_PATH: &str = ".agent/DIFF.base";

    pub(super) fn prepare_review_context(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::{create_prompt_backup_with_workspace, write_diff_backup_with_workspace};

        let _ = create_prompt_backup_with_workspace(ctx.workspace);

        let (diff, baseline_oid) =
            match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace) {
                Ok((diff, baseline_oid)) => (diff, baseline_oid),
                Err(err) => {
                    ctx.logger
                        .warn(&format!("Failed to compute review diff: {err}"));
                    (String::new(), String::new())
                }
            };
        let _ = write_diff_backup_with_workspace(ctx.workspace, &diff);

        let baseline_path = Path::new(Self::DIFF_BASELINE_PATH);
        if baseline_oid.trim().is_empty() {
            let _ = ctx.workspace.remove_if_exists(baseline_path);
        } else if let Err(err) = ctx.workspace.write(baseline_path, &baseline_oid) {
            ctx.logger
                .warn(&format!("Failed to write review diff baseline: {err}"));
        }

        Ok(EffectResult::with_ui(
            PipelineEvent::review_context_prepared(pass),
            vec![UIEvent::ReviewProgress {
                pass,
                total: self.state.total_reviewer_passes,
            }],
        ))
    }

    pub(super) fn prepare_review_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_review_xml_with_references,
            prompt_review_xsd_retry_with_context, PromptContentBuilder,
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

        let baseline_oid_for_prompts = ctx
            .workspace
            .read(Path::new(Self::DIFF_BASELINE_PATH))
            .unwrap_or_default()
            .trim()
            .to_string();

        let continuation_state = &self.state.continuation;
        let mut last_output = String::new();
        if continuation_state.invalid_output_attempts > 0 {
            last_output = ctx
                .workspace
                .read(Path::new(xml_paths::ISSUES_XML))
                .unwrap_or_default();
        }
        let mut ignore_sources = vec![plan_content.as_str(), diff_content.as_str()];
        if continuation_state.invalid_output_attempts > 0 {
            ignore_sources.push(last_output.as_str());
        }
        let (prompt_key, review_prompt_xml, was_replayed) =
            if continuation_state.invalid_output_attempts > 0 {
                let prompt_key = format!(
                    "review_{pass}_xsd_retry_{}",
                    continuation_state.invalid_output_attempts
                );
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_review_xsd_retry_with_context(
                            ctx.template_context,
                            "",
                            &plan_content,
                            &diff_content,
                            "XML output failed validation. Provide valid XML output.",
                            &last_output,
                            ctx.workspace,
                        )
                    });
                (prompt_key, prompt, was_replayed)
            } else {
                let prompt_key = format!("review_{pass}");
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        let refs = PromptContentBuilder::new(ctx.workspace)
                            .with_plan(plan_content.clone())
                            .with_diff(diff_content.clone(), &baseline_oid_for_prompts)
                            .build();

                        prompt_review_xml_with_references(ctx.template_context, &refs)
                    });
                (prompt_key, prompt, was_replayed)
            };

        let template_name = if continuation_state.invalid_output_attempts > 0 {
            "review_xsd_retry"
        } else {
            "review_xml"
        };
        if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
            &review_prompt_xml,
            &ignore_sources,
        ) {
            return Ok(EffectResult::event(
                PipelineEvent::agent_template_variables_invalid(
                    AgentRole::Reviewer,
                    template_name.to_string(),
                    Vec::new(),
                    err.unresolved_placeholders,
                ),
            ));
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

        let prompt = match ctx
            .workspace
            .read(Path::new(".agent/tmp/review_prompt.txt"))
        {
            Ok(prompt) => prompt,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing review prompt at .agent/tmp/review_prompt.txt".to_string(),
                )));
            }
        };

        let issues_xml = Path::new(xml_paths::ISSUES_XML);
        let _ = ctx.workspace.remove_if_exists(issues_xml);

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.reviewer_agent.to_string());

        let mut result = self.invoke_agent(ctx, AgentRole::Reviewer, agent, None, prompt)?;
        if matches!(
            result.event,
            PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
        ) {
            result = result.with_additional_event(PipelineEvent::review_agent_invoked(pass));
        }
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
                PipelineEvent::review_issues_xml_missing(
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
                let markdown = render_issues_markdown(&elements);
                let snippets = extract_issue_snippets(&elements.issues, ctx.workspace);
                Ok(EffectResult::with_ui(
                    PipelineEvent::review_issues_xml_validated(
                        pass,
                        issues_found,
                        clean_no_issues,
                        Some(markdown),
                    ),
                    vec![UIEvent::XmlOutput {
                        xml_type: XmlOutputType::ReviewIssues,
                        content: issues_xml,
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets,
                        }),
                    }],
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
        use std::path::Path;

        let markdown = match self
            .state
            .review_validated_outcome
            .as_ref()
            .filter(|outcome| outcome.pass == pass)
            .and_then(|outcome| outcome.markdown.clone())
        {
            Some(markdown) => markdown,
            None => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing validated review markdown".to_string(),
                )));
            }
        };
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

    pub(super) fn prepare_fix_prompt(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use crate::prompts::{
            get_stored_or_generate_prompt, prompt_fix_xml_with_context,
            prompt_fix_xsd_retry_with_context,
        };
        use std::path::Path;

        let tmp_dir = Path::new(".agent/tmp");
        if !ctx.workspace.exists(tmp_dir) {
            ctx.workspace.create_dir_all(tmp_dir)?;
        }

        let prompt_content = ctx
            .workspace
            .read(Path::new(".agent/PROMPT.md.backup"))
            .unwrap_or_default();
        let plan_content = ctx
            .workspace
            .read(Path::new(".agent/PLAN.md"))
            .unwrap_or_default();
        let issues_content = ctx
            .workspace
            .read(Path::new(".agent/ISSUES.md"))
            .unwrap_or_default();

        let continuation_state = &self.state.continuation;
        let mut last_output = String::new();
        if continuation_state.invalid_output_attempts > 0 {
            last_output = ctx
                .workspace
                .read(Path::new(xml_paths::FIX_RESULT_XML))
                .unwrap_or_default();
        }
        let mut ignore_sources = vec![
            prompt_content.as_str(),
            plan_content.as_str(),
            issues_content.as_str(),
        ];
        if continuation_state.invalid_output_attempts > 0 {
            ignore_sources.push(last_output.as_str());
        }
        let (prompt_key, fix_prompt, was_replayed) =
            if continuation_state.invalid_output_attempts > 0 {
                let prompt_key = format!(
                    "fix_{pass}_xsd_retry_{}",
                    continuation_state.invalid_output_attempts
                );
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_fix_xsd_retry_with_context(
                            ctx.template_context,
                            &issues_content,
                            "XML output failed validation. Provide valid XML output.",
                            &last_output,
                            ctx.workspace,
                        )
                    });
                (prompt_key, prompt, was_replayed)
            } else {
                let prompt_key = format!("fix_{pass}");
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_fix_xml_with_context(
                            ctx.template_context,
                            &prompt_content,
                            &plan_content,
                            &issues_content,
                            &[],
                        )
                    });
                (prompt_key, prompt, was_replayed)
            };

        let template_name = if continuation_state.invalid_output_attempts > 0 {
            "fix_mode_xsd_retry"
        } else {
            "fix_mode_xml"
        };
        if let Err(err) = crate::prompts::validate_no_unresolved_placeholders_with_ignored_content(
            &fix_prompt,
            &ignore_sources,
        ) {
            return Ok(EffectResult::event(
                PipelineEvent::agent_template_variables_invalid(
                    AgentRole::Reviewer,
                    template_name.to_string(),
                    Vec::new(),
                    err.unresolved_placeholders,
                ),
            ));
        }

        if !was_replayed {
            ctx.capture_prompt(&prompt_key, &fix_prompt);
        }

        ctx.workspace
            .write(Path::new(".agent/tmp/fix_prompt.txt"), &fix_prompt)?;

        Ok(EffectResult::event(PipelineEvent::fix_prompt_prepared(
            pass,
        )))
    }

    pub(super) fn invoke_fix_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::agents::AgentRole;
        use std::path::Path;

        let prompt = match ctx.workspace.read(Path::new(".agent/tmp/fix_prompt.txt")) {
            Ok(prompt) => prompt,
            Err(_) => {
                return Ok(EffectResult::event(PipelineEvent::pipeline_aborted(
                    "Missing fix prompt at .agent/tmp/fix_prompt.txt".to_string(),
                )));
            }
        };

        let fix_xml = Path::new(xml_paths::FIX_RESULT_XML);
        let _ = ctx.workspace.remove_if_exists(fix_xml);

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.reviewer_agent.to_string());

        let mut result = self.invoke_agent(ctx, AgentRole::Reviewer, agent, None, prompt)?;
        if matches!(
            result.event,
            PipelineEvent::Agent(AgentEvent::InvocationSucceeded { .. })
        ) {
            result = result.with_additional_event(PipelineEvent::fix_agent_invoked(pass));
        }
        Ok(result)
    }

    pub(super) fn extract_fix_result_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        let fix_xml = Path::new(xml_paths::FIX_RESULT_XML);
        match ctx.workspace.read(fix_xml) {
            Ok(_) => Ok(EffectResult::event(
                PipelineEvent::fix_result_xml_extracted(pass),
            )),
            Err(_) => Ok(EffectResult::event(PipelineEvent::fix_result_xml_missing(
                pass,
                self.state.continuation.invalid_output_attempts,
            ))),
        }
    }

    pub(super) fn validate_fix_result_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use crate::files::llm_output_extraction::validate_fix_result_xml;
        use std::path::Path;

        let fix_xml = match ctx.workspace.read(Path::new(xml_paths::FIX_RESULT_XML)) {
            Ok(s) => s,
            Err(_) => {
                return Ok(EffectResult::event(
                    PipelineEvent::fix_output_validation_failed(
                        pass,
                        self.state.continuation.invalid_output_attempts,
                    ),
                ));
            }
        };

        match validate_fix_result_xml(&fix_xml) {
            Ok(elements) => {
                let status = crate::reducer::state::FixStatus::parse(&elements.status)
                    .unwrap_or(crate::reducer::state::FixStatus::Failed);
                Ok(EffectResult::with_ui(
                    PipelineEvent::fix_result_xml_validated(pass, status, elements.summary),
                    vec![UIEvent::XmlOutput {
                        xml_type: XmlOutputType::FixResult,
                        content: fix_xml,
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets: Vec::new(),
                        }),
                    }],
                ))
            }
            Err(_) => Ok(EffectResult::event(
                PipelineEvent::fix_output_validation_failed(
                    pass,
                    self.state.continuation.invalid_output_attempts,
                ),
            )),
        }
    }

    pub(super) fn apply_fix_outcome(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        let outcome = self
            .state
            .fix_validated_outcome
            .as_ref()
            .filter(|o| o.pass == pass)
            .ok_or_else(|| anyhow::anyhow!("missing validated fix outcome for pass {pass}"))?;

        let _ = outcome;
        Ok(EffectResult::event(PipelineEvent::fix_outcome_applied(
            pass,
        )))
    }

    pub(super) fn archive_fix_result_xml(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        pass: u32,
    ) -> Result<EffectResult> {
        use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
        use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
        use std::path::Path;

        archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::FIX_RESULT_XML));
        Ok(EffectResult::event(PipelineEvent::fix_result_xml_archived(
            pass,
        )))
    }
}

fn extract_issue_snippets(
    issues: &[String],
    workspace: &dyn crate::workspace::Workspace,
) -> Vec<XmlCodeSnippet> {
    let mut snippets = Vec::new();
    let mut seen = HashSet::new();

    let location_re = Regex::new(
        r"(?m)(?P<file>[A-Za-z0-9 ._\-/\\:]+\.[A-Za-z0-9]+):(?P<start>\d+)(?:[-–—](?P<end>\d+))?(?::(?P<col>\d+))?",
    )
    .unwrap();
    let gh_location_re = Regex::new(
        r"(?m)(?P<file>[A-Za-z0-9 ._\-/\\:]+\.[A-Za-z0-9]+)#L(?P<start>\d+)(?:-L(?P<end>\d+))?",
    )
    .unwrap();

    for issue in issues {
        let (file, line_start, line_end) = if let Some(cap) = location_re.captures(issue) {
            let file = cap
                .name("file")
                .map(|m| m.as_str().trim().replace('\\', "/"));
            let start = cap
                .name("start")
                .and_then(|m| m.as_str().parse::<u32>().ok());
            let end = cap
                .name("end")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .or(start);
            (file, start, end)
        } else if let Some(cap) = gh_location_re.captures(issue) {
            let file = cap
                .name("file")
                .map(|m| m.as_str().trim().replace('\\', "/"));
            let start = cap
                .name("start")
                .and_then(|m| m.as_str().parse::<u32>().ok());
            let end = cap
                .name("end")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .or(start);
            (file, start, end)
        } else {
            (None, None, None)
        };

        let Some(file) = file else { continue };
        let Some(start) = line_start else { continue };
        let end = line_end.unwrap_or(start);

        let key = (file.clone(), start, end);
        if !seen.insert(key) {
            continue;
        }

        let content = match workspace.read(Path::new(&file)) {
            Ok(content) => content,
            Err(_) => continue,
        };

        if let Some(snippet) = extract_snippet_lines(&content, start, end) {
            snippets.push(XmlCodeSnippet {
                file,
                line_start: start,
                line_end: end,
                content: snippet,
            });
        }
    }

    snippets
}

fn extract_snippet_lines(content: &str, start: u32, end: u32) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let start_idx = start.saturating_sub(1) as usize;
    if start_idx >= lines.len() {
        return None;
    }

    let end_idx = end.saturating_sub(1) as usize;
    let end_idx = end_idx.min(lines.len().saturating_sub(1));
    let mut out = String::new();
    for (offset, line) in lines[start_idx..=end_idx].iter().enumerate() {
        let line_no = start + offset as u32;
        out.push_str(&format!("{line_no} | {line}\n"));
    }
    Some(out.trim_end().to_string())
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
