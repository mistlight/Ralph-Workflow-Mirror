//! Content validation for extracted plan and issues.
//!
//! Note: Some functions in this module are currently unused in production
//! (XML extraction is used instead). Kept for potential future use and test compatibility.

/// Validate plan content.
///
/// Checks if the content looks like a valid plan:
/// - Contains markdown headers (lines starting with #)
/// - Has reasonable length (> 50 chars)
/// - Contains plan-like structure indicators
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn validate_plan_content(content: &str) -> (bool, Option<String>) {
    let content_clean = content.trim();

    let has_header = content_clean
        .lines()
        .any(|line| line.trim().starts_with('#'));
    let has_min_length = content_clean.len() > 50;
    let has_structure = content_clean.contains("step")
        || content_clean.contains("task")
        || content_clean.contains("phase")
        || content_clean.contains("implement")
        || content_clean.contains("create")
        || content_clean.contains("add")
        || content_clean.contains("Step")
        || content_clean.contains("Task")
        || content_clean.contains("Phase");

    if has_header && has_min_length && has_structure {
        (true, None)
    } else {
        let mut warnings = Vec::new();
        if !has_header {
            warnings.push("no markdown headers");
        }
        if !has_min_length {
            warnings.push("content too short");
        }
        if !has_structure {
            warnings.push("no plan structure keywords");
        }
        (false, Some(warnings.join(", ")))
    }
}

/// Validate issues content.
///
/// Checks if the content looks like valid issues:
/// - Contains checkboxes (- \[ ] or - \[x\])
/// - Contains severity markers (Critical:, High:, etc.)
/// - Or contains "no issues" declaration
/// - Contains file path patterns (e.g., `path/to/file.rs:line`)
/// - Contains code-related keywords indicating substantive content
pub fn validate_issues_content(content: &str) -> (bool, Option<String>) {
    let content_clean = content.trim();

    let has_checkbox = content_clean.contains("- [")
        || content_clean.contains("- [x]")
        || content_clean.contains("- [ ]");
    let has_severity = content_clean.contains("Critical:")
        || content_clean.contains("High:")
        || content_clean.contains("Medium:")
        || content_clean.contains("Low:");
    let has_no_issues = content_clean.to_lowercase().contains("no issues");
    let has_min_length = content_clean.len() > 10;

    // Additional patterns that indicate valid issue content from various agents
    // Codex CLI and other agents may format issues differently
    let has_file_path = regex::Regex::new(r"[`\[]?\w+[\w/]*\.?\w*:\d+[\]`]?")
        .map(|re| re.is_match(content_clean))
        .unwrap_or(false);

    // Code-related keywords that suggest substantive issue content
    let has_code_keywords = content_clean.contains("fn ")
        || content_clean.contains("function")
        || content_clean.contains("compile")
        || content_clean.contains("error")
        || content_clean.contains("bug")
        || content_clean.contains("fix")
        || content_clean.contains("issue")
        || content_clean.contains("duplicate")
        || content_clean.contains("missing");

    // More permissive validation: accept if we have any indicator of valid content
    // AND the content is substantial enough (> 50 chars for the permissive path)
    let has_substantial_content = content_clean.len() > 50;
    let has_any_marker =
        has_checkbox || has_severity || has_no_issues || has_file_path || has_code_keywords;

    if has_any_marker && has_min_length {
        (true, None)
    } else if has_substantial_content && has_code_keywords {
        // Fallback: substantial content with code keywords is likely valid
        (true, None)
    } else {
        let mut warnings = Vec::new();
        if !has_checkbox && !has_severity && !has_no_issues && !has_file_path {
            warnings.push("no issue markers found");
        }
        if !has_min_length {
            warnings.push("content too short");
        }
        (false, Some(warnings.join(", ")))
    }
}
