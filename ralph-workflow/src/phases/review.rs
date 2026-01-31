//! Review phase execution.
//!
//! This module handles the review and fix phase of the Ralph pipeline. It runs
//! a configurable number of review-fix cycles, where each cycle:
//! 1. Reviews the code and creates ISSUES.md
//! 2. Fixes the issues found
//! 3. Cleans up ISSUES.md (in isolation mode)
//!
//! # Module Structure
//!
//! - `validation` - Pre-flight and post-flight validation checks

use crate::checkpoint::restore::ResumeContext;
use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, try_extract_from_file_with_workspace, validate_fix_result_xml,
    validate_issues_xml, xml_paths, IssuesElements,
};
use crate::files::result_extraction::extract_file_paths_from_issues;
use crate::files::{delete_issues_file_for_isolation_with_workspace, update_status_with_workspace};
use crate::pipeline::{run_with_prompt, PipelineRuntime, PromptCommand};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_fix_xml_with_context, prompt_review_xml_with_references,
    ContextLevel, PromptContentBuilder,
};
use std::path::Path;

mod validation;
pub use validation::{
    post_flight_review_check, pre_flight_review_check, PostflightResult, PreflightResult,
};

use super::context::PhaseContext;

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};

use std::time::Instant;

/// Result of running a review pass.
#[derive(Debug)]
pub struct ReviewPassResult {
    /// Whether the review found no issues and should exit early.
    pub early_exit: bool,
    /// Whether an authentication/credential error was detected.
    /// When true, the caller should trigger agent fallback instead of retrying.
    pub auth_failure: bool,
    /// Whether the agent failed to run successfully.
    pub agent_failed: bool,
    /// Whether the review output was validated successfully.
    pub output_valid: bool,
    /// Whether issues were found in the validated output.
    pub issues_found: bool,
    /// Raw XML content for UI rendering (if available).
    pub xml_content: Option<String>,
}

/// Result of running a fix pass.
#[derive(Debug)]
pub struct FixPassResult {
    /// Whether an authentication/credential error was detected.
    pub auth_failure: bool,
    /// Whether the agent failed to run successfully.
    pub agent_failed: bool,
    /// Whether the fix output was validated successfully.
    pub output_valid: bool,
    /// Whether changes were made according to the fix output.
    pub changes_made: bool,
    /// Parsed fix status from `<ralph-status>` (when output is valid).
    pub status: Option<String>,
    /// Optional summary from `<ralph-summary>` (when output is valid).
    pub summary: Option<String>,
    /// Raw XML content for UI rendering (if available).
    pub xml_content: Option<String>,
}

/// Result of parsing review output.
#[derive(Debug)]
enum ParseResult {
    /// Successfully parsed with issues found
    IssuesFound {
        issues: Vec<String>,
        xml_content: String,
    },
    /// Successfully parsed with explicit "no issues" declaration
    NoIssuesExplicit { xml_content: String },
    /// Failed to parse - includes error description for re-prompting
    ParseFailed(String),
}

/// Run the review pass for a single cycle.
///
/// This function runs a single review pass and validates the XML output.
pub fn run_review_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    review_label: &str,
    _review_prompt: &str, // Unused - we build XML prompt internally
    _agent: Option<&str>,
) -> anyhow::Result<ReviewPassResult> {
    let active_agent = _agent.unwrap_or(ctx.reviewer_agent);
    let issues_path = Path::new(".agent/ISSUES.md");

    let plan_content = ctx
        .workspace
        .read(Path::new(".agent/PLAN.md"))
        .unwrap_or_default();

    let (changes_content, baseline_oid_for_prompts) =
        match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace) {
            Ok((diff, baseline_oid)) => (diff, baseline_oid),
            Err(e) => {
                ctx.logger
                    .warn(&format!("Failed to get baseline diff for review: {e}"));
                (String::new(), String::new())
            }
        };

    let prompt_key = format!("review_{}", j);
    let (review_prompt_xml, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
            let refs = PromptContentBuilder::new(ctx.workspace)
                .with_plan(plan_content.clone())
                .with_diff(changes_content.clone(), &baseline_oid_for_prompts)
                .build();

            prompt_review_xml_with_references(ctx.template_context, &refs)
        });

    // Enforce that the rendered prompt does not contain unresolved template placeholders.
    // This must happen before any agent invocation.
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
    } else {
        ctx.logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Review prompt length: {} characters",
            review_prompt_xml.len()
        ));
    }

    let log_dir = Path::new(".agent/logs");
    ctx.workspace.create_dir_all(log_dir)?;
    let logfile = format!(".agent/logs/reviewer_review_{j}.log");

    let agent_config = ctx
        .registry
        .resolve_config(active_agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", active_agent))?;
    let cmd_str = agent_config.build_cmd_with_model(true, true, true, None);

    let mut runtime = PipelineRuntime {
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
        executor: ctx.executor,
        executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
        workspace: ctx.workspace,
    };

    let prompt_cmd = PromptCommand {
        label: review_label,
        display_name: active_agent,
        cmd_str: &cmd_str,
        prompt: &review_prompt_xml,
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let attempt_start = Instant::now();
    let result = run_with_prompt(&prompt_cmd, &mut runtime)?;
    if result.exit_code != 0 {
        let auth_failure = stderr_contains_auth_error(&result.stderr);
        return Ok(ReviewPassResult {
            early_exit: false,
            auth_failure,
            agent_failed: true,
            output_valid: false,
            issues_found: false,
            xml_content: None,
        });
    }

    let log_prefix = format!(".agent/logs/reviewer_review_{j}");
    let parse_result = extract_and_validate_review_output_xml(ctx, &log_prefix, issues_path)?;

    match parse_result {
        ParseResult::IssuesFound {
            issues,
            xml_content,
        } => {
            handle_postflight_validation(ctx, j);

            ctx.logger
                .success(&format!("Issues extracted: {} total", issues.len()));

            let step = ExecutionStep::new(
                "Review",
                j,
                "review",
                StepOutcome::success(
                    Some(format!("{} issues found", issues.len())),
                    vec![".agent/ISSUES.md".to_string()],
                ),
            )
            .with_agent(active_agent)
            .with_duration(attempt_start.elapsed().as_secs());
            ctx.execution_history.add_step(step);

            Ok(ReviewPassResult {
                early_exit: false,
                auth_failure: false,
                agent_failed: false,
                output_valid: true,
                issues_found: true,
                xml_content: Some(xml_content),
            })
        }
        ParseResult::NoIssuesExplicit { xml_content } => {
            ctx.logger
                .success(&format!("No issues found after cycle {j} - stopping early"));

            if ctx.config.isolation_mode {
                delete_issues_file_for_isolation_with_workspace(ctx.workspace, ctx.logger)?;
            }

            let step = ExecutionStep::new(
                "Review",
                j,
                "review",
                StepOutcome::success(Some("No issues found".to_string()), vec![]),
            )
            .with_agent(active_agent)
            .with_duration(attempt_start.elapsed().as_secs());
            ctx.execution_history.add_step(step);

            Ok(ReviewPassResult {
                early_exit: true,
                auth_failure: false,
                agent_failed: false,
                output_valid: true,
                issues_found: false,
                xml_content: Some(xml_content),
            })
        }
        ParseResult::ParseFailed(reason) => {
            ctx.logger
                .warn(&format!("Review output validation failed: {reason}"));

            Ok(ReviewPassResult {
                early_exit: false,
                auth_failure: false,
                agent_failed: false,
                output_valid: false,
                issues_found: false,
                xml_content: None,
            })
        }
    }
}

/// Extract review output using XML extraction and validate with XSD.
///
/// Returns a `ParseResult` indicating whether the output was successfully parsed,
/// explicitly declared no issues, or failed to parse (with an error description).
///
/// # Extraction Priority
///
/// 1. File-based XML at `.agent/tmp/issues.xml` (required)
///
/// Legacy log extraction and ISSUES.md fallback have been removed. Agents must
/// produce XML output via the reducer/effect path.
fn extract_and_validate_review_output_xml(
    ctx: &mut PhaseContext<'_>,
    log_dir: &str,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    // Priority 1: Check for file-based XML at .agent/tmp/issues.xml
    // This is the preferred path for agents that write XML directly (e.g., opencode parser)
    if let Some(xml_content) =
        try_extract_from_file_with_workspace(ctx.workspace, Path::new(xml_paths::ISSUES_XML))
    {
        ctx.logger
            .info("Found XML in .agent/tmp/issues.xml (file-based mode)");
        return validate_and_process_issues_xml(ctx, &xml_content, issues_path);
    }

    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Review output missing at .agent/tmp/issues.xml; expected log prefix: {log_dir}"
        ));
    }

    // Legacy JSON log extraction removed - fail with clear error
    Ok(ParseResult::ParseFailed(
        "No review output captured. Agent did not write to .agent/tmp/issues.xml. \
         Ensure the agent produces valid XML output via the configured effects."
            .to_string(),
    ))
}

/// Helper to validate XML and process the result for issues extraction.
fn validate_and_process_issues_xml(
    ctx: &mut PhaseContext<'_>,
    xml_content: &str,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    // Validate the extracted XML against XSD
    let validated: Result<IssuesElements, XsdValidationError> = validate_issues_xml(xml_content);

    match validated {
        Ok(elements) => {
            let markdown = render_issues_markdown(&elements);
            ctx.workspace.write(issues_path, &markdown)?;
            archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::ISSUES_XML));

            if elements.no_issues_found.is_some() {
                return Ok(ParseResult::NoIssuesExplicit {
                    xml_content: xml_content.to_string(),
                });
            }

            if !elements.issues.is_empty() {
                return Ok(ParseResult::IssuesFound {
                    issues: elements.issues,
                    xml_content: xml_content.to_string(),
                });
            }

            Ok(ParseResult::ParseFailed(
                "XML validated but contains no issues or no-issues-found element.".to_string(),
            ))
        }
        Err(xsd_error) => {
            // Return the specific XSD error for retry
            Ok(ParseResult::ParseFailed(xsd_error.format_for_ai_retry()))
        }
    }
}

fn render_issues_markdown(elements: &IssuesElements) -> String {
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

/// Handle post-flight validation after a review pass.
fn handle_postflight_validation(ctx: &PhaseContext<'_>, j: u32) {
    let postflight_result = post_flight_review_check(ctx.workspace, ctx.logger, j);
    match postflight_result {
        PostflightResult::Valid => {
            // ISSUES.md found and valid, continue
        }
        PostflightResult::Missing(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. Proceeding with fix pass anyway."
            ));
        }
        PostflightResult::Malformed(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. The fix pass may not work correctly."
            ));
            ctx.logger.info(&format!(
                "{}Tip:{} Try with generic parser: {}RALPH_REVIEWER_JSON_PARSER=generic ralph{}",
                ctx.colors.bold(),
                ctx.colors.reset(),
                ctx.colors.bold(),
                ctx.colors.reset()
            ));
        }
    }
}

fn stderr_contains_auth_error(stderr: &str) -> bool {
    let combined = stderr.to_lowercase();
    combined.contains("authentication")
        || combined.contains("unauthorized")
        || combined.contains("credential")
        || combined.contains("api key")
        || combined.contains("not authorized")
}

/// Run the fix pass for a single cycle.
///
/// This function runs a single fix pass and validates the XML output.
pub fn run_fix_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    _reviewer_context: ContextLevel,
    _resume_context: Option<&ResumeContext>,
    _agent: Option<&str>,
) -> anyhow::Result<FixPassResult> {
    let active_agent = _agent.unwrap_or(ctx.reviewer_agent);
    let fix_start_time = Instant::now();

    update_status_with_workspace(ctx.workspace, "Applying fixes", ctx.config.isolation_mode)?;

    let prompt_content = ctx
        .workspace
        .read(Path::new("PROMPT.md"))
        .unwrap_or_default();
    let plan_content = ctx
        .workspace
        .read(Path::new(".agent/PLAN.md"))
        .unwrap_or_default();
    let issues_content = ctx
        .workspace
        .read(Path::new(".agent/ISSUES.md"))
        .unwrap_or_default();

    let files_to_modify = extract_file_paths_from_issues(&issues_content);

    let prompt_key = format!("fix_{}", j);
    let (fix_prompt, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
            prompt_fix_xml_with_context(
                ctx.template_context,
                &prompt_content,
                &plan_content,
                &issues_content,
                &files_to_modify,
            )
        });

    // Enforce that the rendered prompt does not contain unresolved template placeholders.
    // This must happen before any agent invocation.
    if let Err(err) = crate::prompts::validate_no_unresolved_placeholders(&fix_prompt) {
        return Err(crate::prompts::TemplateVariablesInvalidError {
            template_name: "fix_mode_xml".to_string(),
            missing_variables: Vec::new(),
            unresolved_placeholders: err.unresolved_placeholders,
        }
        .into());
    }

    if !was_replayed {
        ctx.capture_prompt(&prompt_key, &fix_prompt);
    } else {
        ctx.logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Fix prompt length: {} characters",
            fix_prompt.len()
        ));
    }

    let log_dir = Path::new(".agent/logs");
    ctx.workspace.create_dir_all(log_dir)?;
    let logfile = format!(".agent/logs/reviewer_fix_{j}.log");

    let agent_config = ctx
        .registry
        .resolve_config(active_agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", active_agent))?;
    let cmd_str = agent_config.build_cmd_with_model(true, true, true, None);

    let mut runtime = PipelineRuntime {
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
        executor: ctx.executor,
        executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
        workspace: ctx.workspace,
    };

    let prompt_cmd = PromptCommand {
        label: "fix",
        display_name: active_agent,
        cmd_str: &cmd_str,
        prompt: &fix_prompt,
        logfile: &logfile,
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let result = run_with_prompt(&prompt_cmd, &mut runtime)?;
    if result.exit_code != 0 {
        let auth_failure = stderr_contains_auth_error(&result.stderr);
        return Ok(FixPassResult {
            auth_failure,
            agent_failed: true,
            output_valid: false,
            changes_made: false,
            status: None,
            summary: None,
            xml_content: None,
        });
    }

    let xml_content =
        try_extract_from_file_with_workspace(ctx.workspace, Path::new(xml_paths::FIX_RESULT_XML));

    let Some(xml_to_validate) = xml_content else {
        return Ok(FixPassResult {
            auth_failure: false,
            agent_failed: false,
            output_valid: false,
            changes_made: false,
            status: None,
            summary: None,
            xml_content: None,
        });
    };

    match validate_fix_result_xml(&xml_to_validate) {
        Ok(result_elements) => {
            archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::FIX_RESULT_XML));

            let changes_made = !result_elements.is_no_issues();

            let step = ExecutionStep::new(
                "Review",
                j,
                "fix",
                StepOutcome::success(result_elements.summary.clone(), vec![]),
            )
            .with_agent(active_agent)
            .with_duration(fix_start_time.elapsed().as_secs());
            ctx.execution_history.add_step(step);

            Ok(FixPassResult {
                auth_failure: false,
                agent_failed: false,
                output_valid: true,
                changes_made,
                status: Some(result_elements.status.clone()),
                summary: result_elements.summary.clone(),
                xml_content: Some(xml_to_validate),
            })
        }
        Err(err) => {
            ctx.logger
                .warn(&format!("Fix XML validation failed: {err}"));
            Ok(FixPassResult {
                auth_failure: false,
                agent_failed: false,
                output_valid: false,
                changes_made: false,
                status: None,
                summary: None,
                xml_content: Some(xml_to_validate),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRegistry;
    use crate::checkpoint::execution_history::ExecutionHistory;
    use crate::checkpoint::RunContext;
    use crate::config::Config;
    use crate::executor::MockProcessExecutor;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;
    use crate::workspace::MemoryWorkspace;
    use crate::workspace::Workspace;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    struct TestFixture {
        config: Config,
        registry: AgentRegistry,
        colors: Colors,
        logger: Logger,
        timer: Timer,
        stats: Stats,
        template_context: TemplateContext,
        executor_arc: Arc<dyn crate::executor::ProcessExecutor>,
        repo_root: PathBuf,
        workspace: MemoryWorkspace,
    }

    impl TestFixture {
        fn new(workspace: MemoryWorkspace) -> Self {
            let colors = Colors { enabled: false };
            let executor_arc =
                Arc::new(MockProcessExecutor::new()) as Arc<dyn crate::executor::ProcessExecutor>;
            let repo_root = PathBuf::from("/test/repo");
            Self {
                config: Config::default(),
                registry: AgentRegistry::new().unwrap(),
                colors,
                logger: Logger::new(colors),
                timer: Timer::new(),
                stats: Stats::default(),
                template_context: TemplateContext::default(),
                executor_arc,
                repo_root,
                workspace,
            }
        }

        fn ctx(&mut self) -> PhaseContext<'_> {
            PhaseContext {
                config: &self.config,
                registry: &self.registry,
                logger: &self.logger,
                colors: &self.colors,
                timer: &mut self.timer,
                stats: &mut self.stats,
                developer_agent: "dev",
                reviewer_agent: "review",
                review_guidelines: None,
                template_context: &self.template_context,
                run_context: RunContext::new(),
                execution_history: ExecutionHistory::new(),
                prompt_history: std::collections::HashMap::new(),
                executor: self.executor_arc.as_ref(),
                executor_arc: self.executor_arc.clone(),
                repo_root: self.repo_root.as_path(),
                workspace: &self.workspace,
            }
        }
    }

    #[test]
    fn test_validate_and_process_issues_xml_archives_and_writes_markdown() {
        let xml_content = r#"<ralph-issues>
 <ralph-no-issues-found>No issues were found during review</ralph-no-issues-found>
 </ralph-issues>"#;

        let workspace = MemoryWorkspace::new_test().with_file(xml_paths::ISSUES_XML, xml_content);
        let mut fixture = TestFixture::new(workspace);
        let mut ctx = fixture.ctx();

        let _ =
            validate_and_process_issues_xml(&mut ctx, xml_content, Path::new(".agent/ISSUES.md"))
                .expect("validate_and_process_issues_xml should succeed for valid XML");

        assert!(
            !fixture.workspace.exists(Path::new(xml_paths::ISSUES_XML)),
            "expected {} to be archived after validation",
            xml_paths::ISSUES_XML
        );
        assert!(
            fixture
                .workspace
                .exists(Path::new(".agent/tmp/issues.xml.processed")),
            "expected archived issues XML to exist"
        );

        let issues_md = fixture
            .workspace
            .read(Path::new(".agent/ISSUES.md"))
            .expect("ISSUES.md should be written");
        assert!(
            issues_md.contains("No issues"),
            "expected ISSUES.md to contain the no-issues summary"
        );
        assert!(
            !issues_md.contains("<ralph-issues>"),
            "expected ISSUES.md to be markdown, not raw XML"
        );
    }
}
