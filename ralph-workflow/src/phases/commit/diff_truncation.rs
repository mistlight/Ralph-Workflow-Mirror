/// Maximum safe prompt size in bytes before pre-truncation.
const MAX_SAFE_PROMPT_SIZE: u64 = 200_000;

/// Maximum prompt size for GLM-like agents (GLM, Zhipu, Qwen, DeepSeek).
const GLM_MAX_PROMPT_SIZE: u64 = 100_000;

/// Maximum prompt size for Claude-based agents.
const CLAUDE_MAX_PROMPT_SIZE: u64 = 300_000;

/// Get the maximum safe prompt size for a specific agent.
pub fn model_budget_bytes_for_agent_name(commit_agent: &str) -> u64 {
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

pub fn effective_model_budget_bytes(agent_names: &[String]) -> u64 {
    agent_names
        .iter()
        .map(|name| model_budget_bytes_for_agent_name(name))
        .min()
        .unwrap_or(MAX_SAFE_PROMPT_SIZE)
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
        if summary.len() <= max_size {
            if result.len() + summary.len() <= max_size {
                result.push_str(&summary);
            } else {
                let target_bytes = max_size.saturating_sub(summary.len());
                if target_bytes < result.len() {
                    let mut cut = 0usize;
                    for (idx, _) in result.char_indices() {
                        if idx > target_bytes {
                            break;
                        }
                        cut = idx;
                    }
                    result.truncate(cut);
                }
                result.push_str(&summary);
            }
        }
    }

    result
}

pub fn truncate_diff_to_model_budget(diff: &str, max_size_bytes: u64) -> (String, bool) {
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

#[cfg(test)]
mod diff_truncation_tests {
    use super::*;

    #[test]
    fn truncate_diff_to_model_budget_never_exceeds_max_size() {
        let files_included = 1;
        let total_files = 2;
        let summary = format!(
            "\n[Truncated: {} of {} files shown]\n",
            files_included, total_files
        );

        let max_size = 1_000usize;

        // Craft a diff where:
        // - file 1 fits within max_size
        // - file 2 does not fit, so a truncation summary is appended
        // - file 1 content is sized so adding summary would exceed max_size
        let file1_header = "diff --git a/src/a.rs b/src/a.rs";
        let desired_file1_size = max_size - summary.len() + 1;
        let filler_line_len = desired_file1_size.saturating_sub(file1_header.len() + 2);
        let file1 = format!(
            "{file1_header}\n+{}\n",
            "x".repeat(filler_line_len.saturating_sub(1))
        );

        let file2 = "diff --git a/tests/b.rs b/tests/b.rs\n+small\n";
        let diff = format!("{file1}{file2}");

        let (truncated, was_truncated) = truncate_diff_to_model_budget(&diff, max_size as u64);
        assert!(was_truncated, "expected truncation when diff exceeds max size");
        assert!(
            truncated.len() <= max_size,
            "truncated diff must not exceed max_size (got {} > {})",
            truncated.len(),
            max_size
        );
    }
}
