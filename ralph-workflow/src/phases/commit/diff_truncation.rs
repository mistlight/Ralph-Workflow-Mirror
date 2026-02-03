/// Maximum safe prompt size in bytes before pre-truncation.
const MAX_SAFE_PROMPT_SIZE: u64 = 200_000;

/// Maximum prompt size for GLM-like agents (GLM, Zhipu, Qwen, DeepSeek).
const GLM_MAX_PROMPT_SIZE: u64 = 100_000;

/// Maximum prompt size for Claude-based agents.
const CLAUDE_MAX_PROMPT_SIZE: u64 = 300_000;

/// Get the maximum safe prompt size for a specific agent.
pub(crate) fn model_budget_bytes_for_agent_name(commit_agent: &str) -> u64 {
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

pub(crate) fn effective_model_budget_bytes(agent_names: &[String]) -> u64 {
    agent_names
        .iter()
        .map(|name| model_budget_bytes_for_agent_name(name))
        .min()
        .unwrap_or(MAX_SAFE_PROMPT_SIZE)
}

fn max_prompt_size_for_agent(commit_agent: &str) -> usize {
    usize::try_from(model_budget_bytes_for_agent_name(commit_agent))
        .unwrap_or(usize::try_from(MAX_SAFE_PROMPT_SIZE).unwrap_or(200_000))
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

pub(crate) fn truncate_diff_to_model_budget(diff: &str, max_size_bytes: u64) -> (String, bool) {
    let max_size = usize::try_from(max_size_bytes).unwrap_or(usize::MAX);
    if diff.len() <= max_size {
        (diff.to_string(), false)
    } else {
        (truncate_diff_if_large(diff, max_size), true)
    }
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
        let truncated = truncate_diff_if_large(diff, max_size);
        logger.warn(&format!(
            "Diff size ({} KB) exceeds agent limit ({} KB). Pre-truncating to {} KB to avoid token errors.",
            diff.len() / 1024,
            max_size / 1024,
            truncated.len() / 1024
        ));
        (truncated, true)
    } else {
        logger.info(&format!(
            "Diff size ({} KB) is within safe limit ({} KB).",
            diff.len() / 1024,
            max_size / 1024
        ));
        (diff.to_string(), false)
    }
}
