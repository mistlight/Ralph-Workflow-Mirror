//! Commit message generation phase.
//!
//! This module generates commit messages using a single agent attempt per
//! reducer effect. All validation and retry decisions are handled by the
//! reducer via events; this code does not implement fallback chains or
//! in-session XSD retries.

use super::commit_logging::{AttemptOutcome, CommitLogSession, ExtractionAttempt};
use super::context::PhaseContext;
use crate::agents::AgentRegistry;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, try_extract_from_file_with_workspace,
    try_extract_xml_commit_with_trace, xml_paths, CommitExtractionResult,
};
use crate::logger::Logger;
use crate::pipeline::{run_with_prompt, PipelineRuntime, PromptCommand};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_generate_commit_message_with_diff_with_context,
    TemplateContext,
};
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::Path;

/// Maximum safe prompt size in bytes before pre-truncation.
const MAX_SAFE_PROMPT_SIZE: usize = 200_000;

/// Maximum prompt size for GLM-like agents (GLM, Zhipu, Qwen, DeepSeek).
const GLM_MAX_PROMPT_SIZE: usize = 100_000;

/// Maximum prompt size for Claude-based agents.
const CLAUDE_MAX_PROMPT_SIZE: usize = 300_000;

/// Result of commit message generation.
pub struct CommitMessageResult {
    /// The generated commit message
    pub message: String,
    /// Whether the generation was successful
    pub success: bool,
    /// Path to the agent log file for debugging (currently unused)
    pub _log_path: String,
    /// Prompts that were generated during this commit generation (key -> prompt)
    pub generated_prompts: HashMap<String, String>,
}

/// Outcome from a single commit attempt.
pub struct CommitAttemptResult {
    pub had_error: bool,
    pub output_valid: bool,
    pub message: Option<String>,
    pub validation_detail: String,
    pub auth_failure: bool,
}

enum CommitExtractionOutcome {
    MissingFile(String),
    InvalidXml(String),
    Valid(CommitExtractionResult),
}

/// Get the maximum safe prompt size for a specific agent.
fn max_prompt_size_for_agent(commit_agent: &str) -> usize {
    let agent_lower = commit_agent.to_lowercase();

    if agent_lower.contains("glm")
        || agent_lower.contains("zhipuai")
        || agent_lower.contains("zai")
        || agent_lower.contains("qwen")
        || agent_lower.contains("deepseek")
    {
        GLM_MAX_PROMPT_SIZE
    } else if agent_lower.contains("claude")
        || agent_lower.contains("ccs")
        || agent_lower.contains("anthropic")
    {
        CLAUDE_MAX_PROMPT_SIZE
    } else {
        MAX_SAFE_PROMPT_SIZE
    }
}

/// Truncate diff if it's too large for agents with small context windows.
fn truncate_diff_if_large(diff: &str, max_size: usize) -> String {
    if diff.len() <= max_size {
        return diff.to_string();
    }

    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file = DiffFile::default();
    let mut in_file = false;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if in_file && !current_file.lines.is_empty() {
                files.push(std::mem::take(&mut current_file));
            }
            in_file = true;
            current_file.lines.push(line.to_string());

            if let Some(path) = line.split(" b/").nth(1) {
                current_file.path = path.to_string();
                current_file.priority = prioritize_file_path(path);
            }
        } else if in_file {
            current_file.lines.push(line.to_string());
        }
    }

    if in_file && !current_file.lines.is_empty() {
        files.push(current_file);
    }

    files.sort_by(|a, b| b.priority.cmp(&a.priority));

    let mut result = String::new();
    let mut current_size = 0;
    let mut files_included = 0;
    let total_files = files.len();

    for file in &files {
        let file_size: usize = file.lines.iter().map(|l| l.len() + 1).sum();

        if current_size + file_size <= max_size {
            for line in &file.lines {
                result.push_str(line);
                result.push('\n');
            }
            current_size += file_size;
            files_included += 1;
        } else if files_included == 0 {
            let truncated_lines = truncate_lines_to_fit(&file.lines, max_size);
            for line in truncated_lines {
                result.push_str(&line);
                result.push('\n');
            }
            files_included = 1;
            break;
        } else {
            break;
        }
    }

    if files_included < total_files {
        let summary = format!(
            "\n[Truncated: {} of {} files shown]\n",
            files_included, total_files
        );
        if current_size + summary.len() <= max_size + 200 {
            result.push_str(&summary);
        }
    }

    result
}

#[derive(Default)]
struct DiffFile {
    path: String,
    priority: i32,
    lines: Vec<String>,
}

fn prioritize_file_path(path: &str) -> i32 {
    if path.starts_with("src/") {
        100
    } else if path.starts_with("tests/") {
        50
    } else if path.ends_with(".md") || path.ends_with(".txt") {
        10
    } else {
        0
    }
}

fn truncate_lines_to_fit(lines: &[String], max_size: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current_size = 0;

    for line in lines {
        let line_size = line.len() + 1;
        if current_size + line_size <= max_size {
            current_size += line_size;
            result.push(line.clone());
        } else {
            break;
        }
    }

    if let Some(last) = result.last_mut() {
        last.push_str(" [truncated...]");
    }

    result
}

fn check_and_pre_truncate_diff(diff: &str, commit_agent: &str, logger: &Logger) -> (String, bool) {
    let max_size = max_prompt_size_for_agent(commit_agent);
    if diff.len() > max_size {
        logger.warn(&format!(
            "Diff size ({} KB) exceeds agent limit ({} KB). Pre-truncating to avoid token errors.",
            diff.len() / 1024,
            max_size / 1024
        ));
        (truncate_diff_if_large(diff, max_size), true)
    } else {
        logger.info(&format!(
            "Diff size ({} KB) is within safe limit ({} KB).",
            diff.len() / 1024,
            max_size / 1024
        ));
        (diff.to_string(), false)
    }
}

fn build_commit_prompt(
    prompt_key: &str,
    template_context: &TemplateContext,
    working_diff: &str,
    workspace: &dyn Workspace,
    prompt_history: &HashMap<String, String>,
) -> (String, bool) {
    get_stored_or_generate_prompt(prompt_key, prompt_history, || {
        prompt_generate_commit_message_with_diff_with_context(
            template_context,
            working_diff,
            workspace,
        )
    })
}

fn stderr_contains_auth_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("authentication")
        || lower.contains("api key")
        || lower.contains("invalid key")
        || lower.contains("unauthorized")
        || lower.contains("permission denied")
}

fn extract_commit_message_from_file_with_workspace(
    workspace: &dyn Workspace,
) -> CommitExtractionOutcome {
    let Some(xml) =
        try_extract_from_file_with_workspace(workspace, Path::new(xml_paths::COMMIT_MESSAGE_XML))
    else {
        return CommitExtractionOutcome::MissingFile(
            "XML output missing or invalid; agent must write .agent/tmp/commit_message.xml"
                .to_string(),
        );
    };

    let (message, detail) = try_extract_xml_commit_with_trace(&xml);
    match message {
        Some(msg) => CommitExtractionOutcome::Valid(CommitExtractionResult::new(msg)),
        None => CommitExtractionOutcome::InvalidXml(detail),
    }
}

/// Run a single commit generation attempt with explicit agent and prompt.
///
/// This does **not** perform in-session XSD retries. If validation fails, the
/// caller should emit a MessageValidationFailed event and let the reducer decide
/// retry/fallback behavior.
pub fn run_commit_attempt(
    ctx: &mut PhaseContext<'_>,
    attempt: u32,
    diff: &str,
    commit_agent: &str,
) -> anyhow::Result<CommitAttemptResult> {
    let (working_diff, diff_truncated) =
        check_and_pre_truncate_diff(diff, commit_agent, ctx.logger);

    let prompt_key = format!("commit_message_attempt_{attempt}");
    let (prompt, was_replayed) = build_commit_prompt(
        &prompt_key,
        ctx.template_context,
        &working_diff,
        ctx.workspace,
        &ctx.prompt_history,
    );

    // Enforce that the rendered prompt does not contain unresolved template placeholders.
    // This must happen before any agent invocation.
    if let Err(err) = crate::prompts::validate_no_unresolved_placeholders(&prompt) {
        return Err(crate::prompts::TemplateVariablesInvalidError {
            template_name: "commit_message_xml".to_string(),
            missing_variables: Vec::new(),
            unresolved_placeholders: err.unresolved_placeholders,
        }
        .into());
    }

    if !was_replayed {
        ctx.capture_prompt(&prompt_key, &prompt);
    }

    let mut runtime = PipelineRuntime {
        timer: ctx.timer,
        logger: ctx.logger,
        colors: ctx.colors,
        config: ctx.config,
        executor: ctx.executor,
        executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
        workspace: ctx.workspace,
    };

    let log_dir = Path::new(".agent/logs/commit_generation");
    let mut session = CommitLogSession::new(log_dir.to_str().unwrap(), ctx.workspace)
        .unwrap_or_else(|_| CommitLogSession::noop());
    let mut attempt_log = session.new_attempt(commit_agent, "single");
    attempt_log.set_prompt_size(prompt.len());
    attempt_log.set_diff_info(working_diff.len(), diff_truncated);

    let agent_config = ctx
        .registry
        .resolve_config(commit_agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", commit_agent))?;
    let cmd_str = agent_config.build_cmd_with_model(true, true, true, None);

    let prompt_cmd = PromptCommand {
        label: commit_agent,
        display_name: commit_agent,
        cmd_str: &cmd_str,
        prompt: &prompt,
        logfile: ".agent/logs/commit_generation/commit_generation.log",
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let result = run_with_prompt(&prompt_cmd, &mut runtime)?;
    let had_error = result.exit_code != 0;
    let auth_failure = had_error && stderr_contains_auth_error(&result.stderr);
    attempt_log.set_raw_output(&result.stderr);

    if auth_failure {
        attempt_log.set_outcome(AttemptOutcome::ExtractionFailed(
            "Authentication error detected".to_string(),
        ));
        if !session.is_noop() {
            let _ = attempt_log.write_to_workspace(session.run_dir(), ctx.workspace);
            let _ = session.write_summary(1, "AUTHENTICATION_FAILURE", ctx.workspace);
        }
        return Ok(CommitAttemptResult {
            had_error,
            output_valid: false,
            message: None,
            validation_detail: "Authentication error detected".to_string(),
            auth_failure: true,
        });
    }

    let extraction = extract_commit_message_from_file_with_workspace(ctx.workspace);
    let (outcome, detail, extraction_result) = match extraction {
        CommitExtractionOutcome::Valid(result) => (
            AttemptOutcome::Success(result.clone().into_message()),
            "Valid commit message extracted".to_string(),
            Some(result),
        ),
        CommitExtractionOutcome::InvalidXml(detail) => (
            AttemptOutcome::XsdValidationFailed(detail.clone()),
            detail,
            None,
        ),
        CommitExtractionOutcome::MissingFile(detail) => (
            AttemptOutcome::ExtractionFailed(detail.clone()),
            detail,
            None,
        ),
    };
    attempt_log.add_extraction_attempt(match &extraction_result {
        Some(_) => ExtractionAttempt::success("XML", detail.clone()),
        None => ExtractionAttempt::failure("XML", detail.clone()),
    });
    attempt_log.set_outcome(outcome.clone());

    if !session.is_noop() {
        let _ = attempt_log.write_to_workspace(session.run_dir(), ctx.workspace);
        let final_outcome = format!("{outcome}");
        let _ = session.write_summary(1, &final_outcome, ctx.workspace);
    }

    if let Some(result) = extraction_result {
        let message = result.into_message();
        return Ok(CommitAttemptResult {
            had_error,
            output_valid: true,
            message: Some(message),
            validation_detail: detail,
            auth_failure: false,
        });
    }

    Ok(CommitAttemptResult {
        had_error,
        output_valid: false,
        message: None,
        validation_detail: detail,
        auth_failure: false,
    })
}

/// Generate a commit message using a single agent attempt.
///
/// Returns an error if XML validation fails or the agent output is missing.
pub fn generate_commit_message(
    diff: &str,
    registry: &AgentRegistry,
    runtime: &mut PipelineRuntime,
    commit_agent: &str,
    template_context: &TemplateContext,
    workspace: &dyn Workspace,
    prompt_history: &HashMap<String, String>,
) -> anyhow::Result<CommitMessageResult> {
    let (working_diff, _diff_truncated) =
        check_and_pre_truncate_diff(diff, commit_agent, runtime.logger);

    let prompt_key = "commit_message_attempt_1";
    let (prompt, was_replayed) = build_commit_prompt(
        prompt_key,
        template_context,
        &working_diff,
        workspace,
        prompt_history,
    );

    let mut generated_prompts = HashMap::new();
    if !was_replayed {
        generated_prompts.insert(prompt_key.to_string(), prompt.clone());
    }

    let agent_config = registry
        .resolve_config(commit_agent)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", commit_agent))?;
    let cmd_str = agent_config.build_cmd_with_model(true, true, true, None);

    let prompt_cmd = PromptCommand {
        label: commit_agent,
        display_name: commit_agent,
        cmd_str: &cmd_str,
        prompt: &prompt,
        logfile: ".agent/logs/commit_generation/commit_generation.log",
        parser_type: agent_config.json_parser,
        env_vars: &agent_config.env_vars,
    };

    let result = run_with_prompt(&prompt_cmd, runtime)?;
    let had_error = result.exit_code != 0;
    let auth_failure = had_error && stderr_contains_auth_error(&result.stderr);
    if auth_failure {
        anyhow::bail!("Authentication error detected");
    }

    let extraction = extract_commit_message_from_file_with_workspace(workspace);
    let result = match extraction {
        CommitExtractionOutcome::Valid(result) => result,
        CommitExtractionOutcome::InvalidXml(detail)
        | CommitExtractionOutcome::MissingFile(detail) => anyhow::bail!(detail),
    };

    archive_xml_file_with_workspace(workspace, Path::new(xml_paths::COMMIT_MESSAGE_XML));

    Ok(CommitMessageResult {
        message: result.into_message(),
        success: true,
        _log_path: String::new(),
        generated_prompts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;

    #[test]
    fn test_truncate_diff_if_large() {
        let large_diff = "diff --git a/src/main.rs b/src/main.rs\n".repeat(1000);
        let truncated = truncate_diff_if_large(&large_diff, 10_000);

        assert!(truncated.len() <= 10_000 + 200);
        assert!(truncated.contains("[Truncated:"));
    }

    #[test]
    fn test_truncate_diff_no_truncation_needed() {
        let small_diff = "diff --git a/src/main.rs b/src/main.rs\n+change\n";
        let truncated = truncate_diff_if_large(&small_diff, 10_000);

        assert_eq!(truncated, small_diff);
    }

    #[test]
    fn test_truncate_diff_preserves_structure() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+change1\n\
            diff --git a/src/lib.rs b/src/lib.rs\n+change2\n";
        let truncated = truncate_diff_if_large(&diff, 10_000);

        assert!(truncated.contains("diff --git a/src/main.rs"));
        assert!(truncated.contains("diff --git a/src/lib.rs"));
    }

    #[test]
    fn test_truncate_diff_very_small_limit() {
        let large_diff = "diff --git a/src/main.rs b/src/main.rs\n".repeat(100);
        let truncated = truncate_diff_if_large(&large_diff, 50);

        assert!(truncated.len() <= 100);
        assert!(truncated.contains("diff --git"));
    }

    #[test]
    fn test_truncate_keeps_high_priority_files() {
        let diff = "diff --git a/README.md b/README.md\n\
            +doc change\n\
            diff --git a/src/main.rs b/src/main.rs\n\
            +important change\n\
            diff --git a/tests/test.rs b/tests/test.rs\n\
            +test change\n";

        let truncated = truncate_diff_if_large(diff, 80);
        assert!(truncated.contains("src/main.rs"));
    }

    #[test]
    fn test_truncate_lines_to_fit() {
        let lines = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
            "line4".to_string(),
        ];

        let truncated = truncate_lines_to_fit(&lines, 18);

        assert_eq!(truncated.len(), 3);
        assert!(truncated[2].ends_with("[truncated...]"));
    }

    #[test]
    fn test_extract_commit_message_from_file_reads_primary_xml() {
        let workspace = MemoryWorkspace::new_test().with_file(
            ".agent/tmp/commit_message.xml",
            "<ralph-commit><ralph-subject>feat: add</ralph-subject></ralph-commit>",
        );

        let extraction = extract_commit_message_from_file_with_workspace(&workspace);
        let CommitExtractionOutcome::Valid(extracted) = extraction else {
            panic!("expected extraction");
        };
        assert_eq!(extracted.into_message(), "feat: add");
    }

    #[test]
    fn test_extract_commit_message_from_file_ignores_processed_archive() {
        let workspace = MemoryWorkspace::new_test().with_file(
            ".agent/tmp/commit_message.xml.processed",
            "<ralph-commit><ralph-subject>feat: add</ralph-subject></ralph-commit>",
        );

        let extraction = extract_commit_message_from_file_with_workspace(&workspace);
        assert!(matches!(
            extraction,
            CommitExtractionOutcome::MissingFile(_)
        ));
    }
}
