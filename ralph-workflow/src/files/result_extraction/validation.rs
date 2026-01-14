//! Content validation for extracted plan and issues.

/// Validate plan content.
///
/// Checks if the content looks like a valid plan:
/// - Contains markdown headers (lines starting with #)
/// - Has reasonable length (> 50 chars)
/// - Contains plan-like structure indicators
pub fn validate_plan_content(content: &str) -> (bool, Option<String>) {
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
/// - Contains checkboxes (- [ ] or - [x])
/// - Contains severity markers (Critical:, High:, etc.)
/// - Or contains "no issues" declaration
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

    if (has_checkbox || has_severity || has_no_issues) && has_min_length {
        (true, None)
    } else {
        let mut warnings = Vec::new();
        if !has_checkbox && !has_severity && !has_no_issues {
            warnings.push("no issue markers found");
        }
        if !has_min_length {
            warnings.push("content too short");
        }
        (false, Some(warnings.join(", ")))
    }
}
