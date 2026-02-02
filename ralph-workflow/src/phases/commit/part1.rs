// Commit phase: constants, types, and diff truncation helpers.

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

pub(crate) fn check_and_pre_truncate_diff(
    diff: &str,
    commit_agent: &str,
    logger: &Logger,
) -> (String, bool) {
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
