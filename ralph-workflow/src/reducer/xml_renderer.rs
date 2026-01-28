//! Semantic XML renderers for user-friendly output.
//!
//! This module provides pure functions that transform raw XML into
//! human-readable terminal output. Each XML type gets a dedicated
//! renderer with semantic understanding of its content.
//!
//! # Architecture
//!
//! The renderers receive raw XML from `UIEvent::XmlOutput` events and
//! transform them into formatted strings for terminal display. This keeps
//! rendering logic at the boundary (event loop) rather than in phase functions.
//!
//! # Graceful Degradation
//!
//! If XML parsing fails, renderers fall back to displaying the raw XML
//! with a warning message. This ensures users always see output even if
//! the format is unexpected.

use super::ui_event::{XmlCodeSnippet, XmlOutputContext, XmlOutputType};
use crate::files::llm_output_extraction::xsd_validation_plan::{FileAction, Priority, Severity};
use crate::files::llm_output_extraction::{
    validate_development_result_xml, validate_fix_result_xml, validate_issues_xml,
    validate_plan_xml,
};
use regex::Regex;
use std::collections::BTreeMap;

/// Render XML content based on its type.
///
/// Returns formatted string for terminal display.
/// Falls back to raw XML with warning if parsing fails.
pub fn render_xml(
    xml_type: &XmlOutputType,
    content: &str,
    context: &Option<XmlOutputContext>,
) -> String {
    match xml_type {
        XmlOutputType::DevelopmentResult => render_development_result(content, context),
        XmlOutputType::DevelopmentPlan => render_plan(content),
        XmlOutputType::ReviewIssues => render_issues(content, context),
        XmlOutputType::FixResult => render_fix_result(content, context),
        XmlOutputType::CommitMessage => render_commit(content),
    }
}

/// Render development result XML with semantic formatting.
///
/// Shows:
/// - Header with box-drawing characters
/// - Status with emoji indicator and label
/// - Summary description with proper indentation
/// - Files changed with action type indicators
/// - Next steps if present
fn render_development_result(content: &str, context: &Option<XmlOutputContext>) -> String {
    let mut output = String::new();

    // Header with optional iteration context
    if let Some(ctx) = context {
        if let Some(iter) = ctx.iteration {
            output.push_str(&format!("\n╔═══ Development Iteration {} ═══╗\n\n", iter));
        }
    }

    match validate_development_result_xml(content) {
        Ok(elements) => {
            // Status with emoji and label
            let (status_emoji, status_label) = match elements.status.as_str() {
                "completed" => ("✅", "Completed"),
                "partial" => ("🔄", "In Progress"),
                "failed" => ("❌", "Failed"),
                _ => ("❓", "Unknown"),
            };
            output.push_str(&format!("{} Status: {}\n\n", status_emoji, status_label));

            // Summary with proper formatting for multiline
            output.push_str("📋 Summary:\n");
            for line in elements.summary.lines() {
                output.push_str(&format!("   {}\n", line));
            }

            // Files changed: prefer diff-like rendering when unified diff is present.
            if let Some(ref files) = elements.files_changed {
                output.push_str(&render_files_changed_as_diff_like_view(files));
            }

            // Next steps with proper formatting
            if let Some(ref next) = elements.next_steps {
                output.push_str("\n➡️  Next Steps:\n");
                for line in next.lines() {
                    output.push_str(&format!("   {}\n", line));
                }
            }
        }
        Err(_) => {
            output.push_str("⚠️  Unable to parse development result XML\n\n");
            output.push_str(content);
        }
    }

    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeAction {
    Create,
    Modify,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffFileSection {
    path: String,
    action: ChangeAction,
    diff: String,
}

fn render_files_changed_as_diff_like_view(files_changed: &str) -> String {
    let trimmed = files_changed.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.contains("diff --git ") {
        let sections = parse_unified_diff_files(trimmed);
        return render_diff_sections("📁 Files Changed", &sections);
    }

    let items = parse_files_changed_list(trimmed);
    if items.is_empty() {
        return String::new();
    }

    let file_list: Vec<&str> = items.iter().map(|(p, _)| p.as_str()).collect();
    let mut output = String::new();
    output.push_str("\n📁 Files Changed:\n");
    output.push_str(&format!(
        "   Modified {} file(s): {}\n",
        file_list.len(),
        file_list.join(", ")
    ));

    for (path, action) in items {
        output.push_str(&format!("\n   📄 {}\n", path));
        output.push_str(&format!(
            "      Action: {}\n",
            match action {
                ChangeAction::Create => "created",
                ChangeAction::Modify => "modified",
                ChangeAction::Delete => "deleted",
            }
        ));
        output.push_str("      (no diff provided)\n");
    }

    output
}

fn parse_unified_diff_files(diff: &str) -> Vec<DiffFileSection> {
    let mut sections: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if !current.is_empty() {
                sections.push(current);
            }
            current = vec![line];
        } else if !current.is_empty() {
            current.push(line);
        }
    }
    if !current.is_empty() {
        sections.push(current);
    }

    sections
        .into_iter()
        .filter_map(|lines| parse_diff_section(&lines))
        .collect()
}

fn parse_diff_section(lines: &[&str]) -> Option<DiffFileSection> {
    let header = *lines.first()?;
    // Example: "diff --git a/src/main.rs b/src/main.rs"
    let mut parts = header.split_whitespace();
    let _ = parts.next()?; // diff
    let _ = parts.next()?; // --git
    let a_path = parts.next()?.trim();
    let b_path = parts.next()?.trim();

    let path = if b_path == "/dev/null" {
        a_path
    } else {
        b_path
    }
    .trim_start_matches("a/")
    .trim_start_matches("b/")
    .to_string();

    let mut action = ChangeAction::Modify;
    for line in lines {
        if line.starts_with("new file mode ") {
            action = ChangeAction::Create;
            break;
        }
        if line.starts_with("deleted file mode ") {
            action = ChangeAction::Delete;
            break;
        }
    }

    Some(DiffFileSection {
        path,
        action,
        diff: lines.join("\n"),
    })
}

fn render_diff_sections(title: &str, sections: &[DiffFileSection]) -> String {
    if sections.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    output.push_str(&format!("\n{}:\n", title));
    output.push_str(&format!(
        "   Modified {} file(s): {}\n",
        sections.len(),
        sections
            .iter()
            .map(|s| s.path.as_str())
            .collect::<Vec<&str>>()
            .join(", ")
    ));

    for section in sections {
        output.push_str(&format!("\n   📄 {}\n", section.path));
        output.push_str(&format!(
            "      Action: {}\n",
            match section.action {
                ChangeAction::Create => "created",
                ChangeAction::Modify => "modified",
                ChangeAction::Delete => "deleted",
            }
        ));
        for line in section.diff.lines() {
            output.push_str(&format!("      {}\n", line));
        }
    }

    output
}

fn parse_files_changed_list(files: &str) -> Vec<(String, ChangeAction)> {
    files
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.trim_start_matches("- ").trim())
        .map(|l| {
            let lowered = l.to_ascii_lowercase();
            let action = if lowered.contains("(created)") || lowered.contains("(new)") {
                ChangeAction::Create
            } else if lowered.contains("(deleted)") || lowered.contains("(removed)") {
                ChangeAction::Delete
            } else {
                ChangeAction::Modify
            };
            let path = l.split_once(" (").map_or(l, |(p, _)| p).trim().to_string();
            (path, action)
        })
        .collect()
}

/// Render development plan XML with semantic formatting.
///
/// Shows:
/// - Box-drawing header
/// - Context description
/// - Scope items with counts and categories
/// - Implementation steps with priorities, file targets, rationale, and dependencies
/// - Risks and mitigations with severity
/// - Verification strategy
fn render_plan(content: &str) -> String {
    let mut output = String::new();

    output.push_str("\n╔════════════════════════════════════╗\n");
    output.push_str("║      Implementation Plan           ║\n");
    output.push_str("╚════════════════════════════════════╝\n\n");

    match validate_plan_xml(content) {
        Ok(elements) => {
            // Context section
            output.push_str("📋 Context:\n");
            output.push_str(&format!("   {}\n\n", elements.summary.context));

            // Scope section with categories
            output.push_str("📊 Scope:\n");
            for item in &elements.summary.scope_items {
                if let Some(ref count) = item.count {
                    output.push_str(&format!("   • {} {}", count, item.description));
                } else {
                    output.push_str(&format!("   • {}", item.description));
                }
                if let Some(ref category) = item.category {
                    output.push_str(&format!(" ({})", category));
                }
                output.push('\n');
            }

            // Steps section with priorities and dependencies
            output.push_str("\n───────────────────────────────────\n");
            output.push_str("📝 Implementation Steps:\n\n");
            for step in &elements.steps {
                let priority_badge = step.priority.map_or(String::new(), |p| {
                    format!(
                        " [{}]",
                        match p {
                            Priority::Critical => "🔴 critical",
                            Priority::High => "🟠 high",
                            Priority::Medium => "🟡 medium",
                            Priority::Low => "🟢 low",
                        }
                    )
                });
                output.push_str(&format!(
                    "   {}. {}{}\n",
                    step.number, step.title, priority_badge
                ));

                for file in &step.target_files {
                    let action_icon = match file.action {
                        FileAction::Create => "➕",
                        FileAction::Modify => "📝",
                        FileAction::Delete => "🗑️",
                    };
                    output.push_str(&format!("      {} {}\n", action_icon, file.path));
                }

                if let Some(ref rationale) = step.rationale {
                    output.push_str(&format!("      💡 {}\n", rationale));
                }

                if !step.depends_on.is_empty() {
                    let deps: Vec<String> = step
                        .depends_on
                        .iter()
                        .map(|d| format!("Step {}", d))
                        .collect();
                    output.push_str(&format!("      🔗 Depends on: {}\n", deps.join(", ")));
                }
                output.push('\n');
            }

            // Risks section with severity
            if !elements.risks_mitigations.is_empty() {
                output.push_str("───────────────────────────────────\n");
                output.push_str("⚠️  Risks & Mitigations:\n\n");
                for risk in &elements.risks_mitigations {
                    let severity_icon = risk.severity.map_or("", |s| match s {
                        Severity::Critical => "🔴",
                        Severity::High => "🟠",
                        Severity::Medium => "🟡",
                        Severity::Low => "🟢",
                    });
                    output.push_str(&format!("   {} Risk: {}\n", severity_icon, risk.risk));
                    output.push_str(&format!("     → Mitigation: {}\n\n", risk.mitigation));
                }
            }

            // Verification section
            if !elements.verification_strategy.is_empty() {
                output.push_str("───────────────────────────────────\n");
                output.push_str("✓ Verification Strategy:\n\n");
                for (i, v) in elements.verification_strategy.iter().enumerate() {
                    output.push_str(&format!("   {}. {}\n", i + 1, v.method));
                    output.push_str(&format!("      Expected: {}\n", v.expected_outcome));
                }
            }
        }
        Err(_) => {
            output.push_str("⚠️  Unable to parse plan XML\n\n");
            output.push_str(content);
        }
    }

    output
}

/// Render review issues XML with semantic formatting.
///
/// Shows:
/// - Box-drawing header with pass number
/// - Issue count or approval celebration
/// - Each issue as numbered item with file path extraction
/// - Visual separators between issues
fn render_issues(content: &str, context: &Option<XmlOutputContext>) -> String {
    let mut output = String::new();

    // Header with pass context
    if let Some(ctx) = context {
        if let Some(pass) = ctx.pass {
            output.push_str(&format!("\n╔═══ Review Pass {} ═══╗\n\n", pass));
        } else {
            output.push_str("\n╔═══ Review Results ═══╗\n\n");
        }
    } else {
        output.push_str("\n╔═══ Review Results ═══╗\n\n");
    }

    match validate_issues_xml(content) {
        Ok(elements) => {
            if elements.issues.is_empty() {
                // Celebration for no issues
                if let Some(ref msg) = elements.no_issues_found {
                    output.push_str("🎉 ✅ Code Approved!\n\n");
                    output.push_str(&format!("   {}\n", msg));
                } else {
                    output.push_str("🎉 ✅ No issues found! Code looks good.\n");
                }
            } else {
                output.push_str(&format!(
                    "🔍 Found {} issue(s) to address:\n\n",
                    elements.issues.len()
                ));
                output.push_str(&render_issues_grouped_by_file(&elements.issues, context));
            }
        }
        Err(_) => {
            output.push_str("⚠️  Unable to parse issues XML\n\n");
            output.push_str(content);
        }
    }

    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedIssue {
    original: String,
    file: Option<String>,
    line_start: Option<u32>,
    line_end: Option<u32>,
    severity: Option<String>,
    snippet: Option<String>,
    description: String,
}

fn render_issues_grouped_by_file(issues: &[String], context: &Option<XmlOutputContext>) -> String {
    let parsed: Vec<ParsedIssue> = issues.iter().map(|i| parse_issue(i)).collect();
    let mut grouped: BTreeMap<String, Vec<ParsedIssue>> = BTreeMap::new();

    for issue in parsed {
        let key = issue
            .file
            .clone()
            .unwrap_or_else(|| "(no file)".to_string());
        grouped.entry(key).or_default().push(issue);
    }

    let mut output = String::new();
    for (file, issues) in grouped {
        output.push_str(&format!("📄 {}\n", file));
        for issue in issues {
            let mut header = String::new();
            if let Some(sev) = &issue.severity {
                header.push_str(&format!("[{}] ", sev));
            }
            if let Some(start) = issue.line_start {
                header.push_str(&format!("L{}", start));
                if let Some(end) = issue.line_end {
                    if end != start {
                        header.push_str(&format!("-L{}", end));
                    }
                }
                header.push_str(": ");
            }

            let desc = issue.description.trim();
            if header.is_empty() {
                output.push_str(&format!("   - {}\n", desc));
            } else {
                output.push_str(&format!("   - {}{}\n", header, desc));
            }

            let snippet = issue
                .snippet
                .clone()
                .or_else(|| snippet_from_context(&issue, context));
            if let Some(snippet) = snippet {
                for line in snippet.lines() {
                    output.push_str(&format!("      {}\n", line));
                }
            }
        }
        output.push('\n');
    }

    output
}

fn snippet_from_context(issue: &ParsedIssue, context: &Option<XmlOutputContext>) -> Option<String> {
    let ctx = context.as_ref()?;
    let file = issue.file.as_ref()?;
    let start = issue.line_start?;
    let end = issue.line_end.unwrap_or(start);

    ctx.snippets
        .iter()
        .find(|s| snippet_matches_issue(s, file, start, end))
        .map(|s| s.content.clone())
}

fn snippet_matches_issue(snippet: &XmlCodeSnippet, file: &str, start: u32, end: u32) -> bool {
    file_matches(&snippet.file, file)
        && ranges_overlap(snippet.line_start, snippet.line_end, start, end)
}

fn file_matches(snippet_file: &str, issue_file: &str) -> bool {
    let snippet_norm = normalize_path_for_match(snippet_file);
    let issue_norm = normalize_path_for_match(issue_file);
    if snippet_norm == issue_norm {
        return true;
    }

    // Be tolerant of differing prefixes (e.g. `./src/lib.rs` vs `src/lib.rs`),
    // and of callers emitting paths rooted at a sub-crate (`ralph-workflow/src/...`).
    let snippet_suffix = format!("/{}", issue_norm);
    if snippet_norm.ends_with(&snippet_suffix) {
        return true;
    }

    let issue_suffix = format!("/{}", snippet_norm);
    issue_norm.ends_with(&issue_suffix)
}

fn normalize_path_for_match(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn ranges_overlap(a_start: u32, a_end: u32, b_start: u32, b_end: u32) -> bool {
    a_start <= b_end && b_start <= a_end
}

fn parse_issue(issue: &str) -> ParsedIssue {
    let original = issue.to_string();
    let trimmed = issue.trim();

    let severity_re = Regex::new(r"(?i)^\[(critical|high|medium|low)\]\s*").unwrap();
    let location_re = Regex::new(
        r"(?m)(?P<file>[-_./A-Za-z0-9]+\.[A-Za-z0-9]+):(?P<start>\d+)(?:[-–—](?P<end>\d+))?(?::(?P<col>\d+))?",
    )
    .unwrap();
    let gh_location_re = Regex::new(
        r"(?m)(?P<file>[-_./A-Za-z0-9]+\.[A-Za-z0-9]+)#L(?P<start>\d+)(?:-L(?P<end>\d+))?",
    )
    .unwrap();
    let snippet_re = Regex::new(r"(?s)```(?:[A-Za-z0-9_-]+)?\s*(?P<code>.*?)\s*```").unwrap();

    let mut working = trimmed.to_string();

    let severity = severity_re
        .captures(&working)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_ascii_lowercase()))
        .map(|s| match s.as_str() {
            "critical" => "Critical".to_string(),
            "high" => "High".to_string(),
            "medium" => "Medium".to_string(),
            "low" => "Low".to_string(),
            _ => s,
        });
    if severity.is_some() {
        working = severity_re.replace(&working, "").to_string();
    }

    let snippet = snippet_re
        .captures(&working)
        .and_then(|cap| cap.name("code").map(|m| m.as_str().to_string()));
    if snippet.is_some() {
        working = snippet_re.replace(&working, "").to_string();
    }

    let (file, line_start, line_end) = if let Some(cap) = location_re.captures(&working) {
        let file = cap.name("file").map(|m| m.as_str().to_string());
        let start = cap
            .name("start")
            .and_then(|m| m.as_str().parse::<u32>().ok());
        let end = cap
            .name("end")
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .or(start);
        (file, start, end)
    } else if let Some(cap) = gh_location_re.captures(&working) {
        let file = cap.name("file").map(|m| m.as_str().to_string());
        let start = cap
            .name("start")
            .and_then(|m| m.as_str().parse::<u32>().ok());
        let end = cap
            .name("end")
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .or(start);
        (file, start, end)
    } else {
        (
            extract_file_from_issue(&working).map(|s| s.to_string()),
            None,
            None,
        )
    };

    let description = working
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<&str>>()
        .join(" ");

    ParsedIssue {
        original,
        file,
        line_start,
        line_end,
        severity,
        snippet,
        description,
    }
}

/// Try to extract file path from issue text using common patterns.
/// Returns None if no clear file path is found.
fn extract_file_from_issue(issue: &str) -> Option<&str> {
    // Common patterns: "in src/file.rs", "at src/file.rs:123", "File: src/file.rs"
    // This is best-effort heuristic parsing
    for pattern in ["in ", "at ", "File: ", "file "] {
        if let Some(idx) = issue.find(pattern) {
            let start = idx + pattern.len();
            let rest = &issue[start..];
            // Find end of path (space, comma, colon for line number, or end of string)
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ',')
                .unwrap_or(rest.len());
            // Handle colon followed by line number (e.g., src/file.rs:123)
            let path_with_line = &rest[..end];
            let path = path_with_line
                .find(':')
                .map_or(path_with_line, |colon_pos| &path_with_line[..colon_pos]);
            if path.contains('/') || path.contains('.') {
                return Some(path);
            }
        }
    }
    None
}

/// Render fix result XML with semantic formatting.
///
/// Shows:
/// - Box-drawing header with pass number
/// - Status with emoji indicator and friendly label
/// - Summary with proper multiline formatting
fn render_fix_result(content: &str, context: &Option<XmlOutputContext>) -> String {
    let mut output = String::new();

    if let Some(ctx) = context {
        if let Some(pass) = ctx.pass {
            output.push_str(&format!("\n╔═══ Fix Pass {} ═══╗\n\n", pass));
        }
    }

    match validate_fix_result_xml(content) {
        Ok(elements) => {
            let (emoji, label): (&str, &str) = match elements.status.as_str() {
                "all_issues_addressed" => ("✅", "All Issues Addressed"),
                "issues_remain" => ("🔄", "Issues Remain"),
                "no_issues_found" => ("✨", "No Issues Found"),
                _ => ("❓", elements.status.as_str()),
            };
            output.push_str(&format!("{} Status: {}\n", emoji, label));

            if let Some(ref summary) = elements.summary {
                output.push_str("\n📋 Summary:\n");
                if summary.contains("diff --git ") {
                    let sections = parse_unified_diff_files(summary);
                    output.push_str(&render_diff_sections("   Changes", &sections));
                } else {
                    for line in summary.lines() {
                        output.push_str(&format!("   {}\n", line));
                    }
                }
            }
        }
        Err(_) => {
            output.push_str("⚠️  Unable to parse fix result XML\n\n");
            output.push_str(content);
        }
    }

    output
}

/// Render commit message XML with semantic formatting.
///
/// Shows:
/// - Box-drawing header
/// - Subject line prominently
/// - Body text with proper indentation
fn render_commit(content: &str) -> String {
    let mut output = String::new();

    output.push_str("\n╔═══ Commit Message ═══╗\n\n");

    // Extract subject and body from commit XML
    // Note: Commit XML uses ralph-subject and ralph-body tags
    let subject = extract_tag_content(content, "ralph-subject")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let body = extract_tag_content(content, "ralph-body")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if subject.is_none() && body.is_none() {
        output.push_str("⚠️  Unable to parse commit message XML\n\n");
        output.push_str(content);
        return output;
    }

    if let Some(subject) = subject {
        output.push_str(&format!("📝 {}\n", subject));
    }

    if let Some(body) = body {
        output.push('\n');
        for line in wrap_commit_body(&body, 80).lines() {
            output.push_str(&format!("   {}\n", line));
        }
    }

    output
}

fn wrap_commit_body(body: &str, max_width: usize) -> String {
    let indent = 3usize;
    let wrap_width = max_width.saturating_sub(indent);

    body.lines()
        .map(|line| {
            let line = line.trim_end();
            if line.is_empty() {
                return String::new();
            }
            let trimmed = line.trim_start();
            let is_listish = trimmed.starts_with('-')
                || trimmed.starts_with('*')
                || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit());
            if is_listish || trimmed.len() <= wrap_width {
                return trimmed.to_string();
            }

            let mut out_lines: Vec<String> = Vec::new();
            let mut current = String::new();
            for word in trimmed.split_whitespace() {
                if current.is_empty() {
                    current.push_str(word);
                    continue;
                }
                if current.len() + 1 + word.len() > wrap_width {
                    out_lines.push(current);
                    current = word.to_string();
                } else {
                    current.push(' ');
                    current.push_str(word);
                }
            }
            if !current.is_empty() {
                out_lines.push(current);
            }
            out_lines.join("\n")
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Extract text content from an XML tag.
///
/// Simple extraction for well-formed tags. Returns None if tag not found.
fn extract_tag_content(content: &str, tag_name: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag_name);
    let end_tag = format!("</{}>", tag_name);

    let start_pos = content.find(&start_tag)?;
    let content_start = start_pos + start_tag.len();
    let end_pos = content[content_start..].find(&end_tag)?;

    Some(content[content_start..content_start + end_pos].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Development Result Renderer Tests
    // =========================================================================

    #[test]
    fn test_render_development_result_completed() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Implemented feature X</ralph-summary>
<ralph-files-changed>src/main.rs
src/lib.rs</ralph-files-changed>
</ralph-development-result>"#;

        let output = render_development_result(xml, &None);

        assert!(output.contains("✅"), "Should have completed emoji");
        assert!(
            output.contains("Completed"),
            "Should show friendly status label"
        );
        assert!(
            output.contains("Implemented feature X"),
            "Should show summary"
        );
        assert!(output.contains("src/main.rs"), "Should list files");
    }

    #[test]
    fn test_render_development_result_renders_diff_like_view_per_file_when_diff_present() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Updated two files</ralph-summary>
<ralph-files-changed>diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,2 @@
-fn main() { println!("old"); }
+fn main() { println!("new"); }
diff --git a/src/lib.rs b/src/lib.rs
new file mode 100644
--- /dev/null
+++ b/src/lib.rs
@@ -0,0 +1,1 @@
+pub fn hello() {}
</ralph-files-changed>
</ralph-development-result>"#;

        let output = render_development_result(xml, &None);

        assert!(
            output.contains("Modified 2 file") || output.contains("2 file"),
            "Should include file count summary"
        );
        assert!(
            output.contains("src/main.rs") && output.contains("src/lib.rs"),
            "Should include per-file headers"
        );
        assert!(
            output.contains("--- a/src/main.rs") && output.contains("+++ b/src/main.rs"),
            "Should include diff markers"
        );
        assert!(
            output.contains("+pub fn hello") || output.contains("pub fn hello"),
            "Should include diff content"
        );
    }

    #[test]
    fn test_render_development_result_partial() {
        let xml = r#"<ralph-development-result>
<ralph-status>partial</ralph-status>
<ralph-summary>Started work on feature</ralph-summary>
<ralph-next-steps>Continue with implementation</ralph-next-steps>
</ralph-development-result>"#;

        let output = render_development_result(xml, &None);

        assert!(output.contains("🔄"), "Should have partial emoji");
        assert!(
            output.contains("Continue with implementation"),
            "Should show next steps"
        );
    }

    #[test]
    fn test_render_development_result_with_iteration() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Done</ralph-summary>
</ralph-development-result>"#;

        let ctx = Some(XmlOutputContext {
            iteration: Some(2),
            pass: None,
            snippets: Vec::new(),
        });
        let output = render_development_result(xml, &ctx);

        assert!(
            output.contains("Development Iteration 2"),
            "Should show iteration number"
        );
    }

    #[test]
    fn test_render_development_result_malformed_fallback() {
        let bad_xml = "not valid xml at all";
        let output = render_development_result(bad_xml, &None);

        assert!(output.contains("⚠️"), "Should show warning");
        assert!(
            output.contains("not valid xml"),
            "Should include raw content"
        );
    }

    // =========================================================================
    // Plan Renderer Tests
    // =========================================================================

    #[test]
    fn test_render_plan_basic_structure() {
        // Use a minimal valid plan structure
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Adding a new feature to the codebase</context>
<scope-items>
<scope-item count="3">files to modify</scope-item>
<scope-item count="1">new file to create</scope-item>
<scope-item>documentation updates</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Add new module</title>
<target-files>
<file path="src/new.rs" action="create"/>
</target-files>
<content>
<paragraph>Create the new module with basic structure.</paragraph>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files>
<file path="src/new.rs" action="create"/>
</primary-files>
<reference-files>
<file path="src/lib.rs" purpose="module registration"/>
</reference-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair severity="low">
<risk>May conflict with existing code</risk>
<mitigation>Review for conflicts</mitigation>
</risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification>
<method>Run tests</method>
<expected-outcome>All tests pass</expected-outcome>
</verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let output = render_plan(xml);

        assert!(
            output.contains("Implementation Plan"),
            "Should have plan header"
        );
        assert!(output.contains("Context:"), "Should show context section");
        assert!(
            output.contains("Adding a new feature"),
            "Should show context text"
        );
        assert!(output.contains("Scope:"), "Should show scope section");
        assert!(
            output.contains("3 files to modify"),
            "Should show scope items"
        );
        assert!(
            output.contains("Implementation Steps"),
            "Should show steps section"
        );
        assert!(
            output.contains("1. Add new module"),
            "Should show step title"
        );
        assert!(
            output.contains("Risks & Mitigations"),
            "Should show risks section"
        );
    }

    #[test]
    fn test_render_plan_malformed_fallback() {
        let bad_xml = "<ralph-plan><incomplete>";
        let output = render_plan(bad_xml);

        assert!(output.contains("⚠️"), "Should show warning");
        assert!(
            output.contains("<ralph-plan>"),
            "Should include raw content"
        );
    }

    // =========================================================================
    // Issues Renderer Tests
    // =========================================================================

    #[test]
    fn test_render_issues_with_issues() {
        let xml = r#"<ralph-issues>
<ralph-issue>Variable unused in src/main.rs</ralph-issue>
<ralph-issue>Missing error handling</ralph-issue>
</ralph-issues>"#;

        let ctx = Some(XmlOutputContext {
            iteration: None,
            pass: Some(1),
            snippets: Vec::new(),
        });
        let output = render_issues(xml, &ctx);

        assert!(output.contains("Review Pass 1"), "Should show pass number");
        assert!(output.contains("2 issue"), "Should show issue count");
        assert!(output.contains("Variable unused"), "Should list issues");
        assert!(
            output.contains("📄 src/main.rs"),
            "Should group issues under extracted file"
        );
        assert!(
            output.contains("Missing error handling"),
            "Should include issues without file"
        );
    }

    #[test]
    fn test_render_issues_groups_by_file_and_renders_line_ranges_and_snippets() {
        let xml = r#"<ralph-issues>
<ralph-issue>[High] src/main.rs:12-18 - Avoid unwrap in production code
```rust
let x = foo().unwrap();
```
</ralph-issue>
<ralph-issue>src/lib.rs:44:3 - Rename variable for clarity</ralph-issue>
<ralph-issue>General suggestion with no file</ralph-issue>
</ralph-issues>"#;

        let output = render_issues(xml, &None);

        assert!(
            output.contains("📄 src/main.rs") && output.contains("📄 src/lib.rs"),
            "Should render grouped file headers"
        );
        assert!(
            output.contains("L12") && output.contains("L18"),
            "Should include parsed line range in Lx-Ly form"
        );
        assert!(output.contains("[High]"), "Should include severity badge");
        assert!(
            output.contains("let x = foo().unwrap()"),
            "Should include extracted snippet"
        );
        assert!(
            output.contains("General suggestion"),
            "Should not drop issues without file"
        );
    }

    #[test]
    fn test_render_issues_uses_context_snippets_when_issue_has_location_but_no_fenced_code() {
        let xml = r#"<ralph-issues>
<ralph-issue>./src/lib.rs:44-44 - Rename variable for clarity</ralph-issue>
</ralph-issues>"#;

        let ctx = Some(XmlOutputContext {
            iteration: None,
            pass: Some(1),
            snippets: vec![XmlCodeSnippet {
                file: "src/lib.rs".to_string(),
                line_start: 42,
                line_end: 46,
                content: "42 | let old_name = 1;\n43 | let x = old_name;\n44 | let clearer = old_name;\n45 | println!(\"{}\", clearer);".to_string(),
            }],
        });

        let output = render_issues(xml, &ctx);

        assert!(
            output.contains("let clearer"),
            "Should render snippet from context even when file path differs by prefix"
        );
    }

    #[test]
    fn test_render_issues_no_issues() {
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>The code looks good, no issues detected</ralph-no-issues-found>
</ralph-issues>"#;

        let output = render_issues(xml, &None);

        assert!(output.contains("✅"), "Should show approval emoji");
        assert!(
            output.contains("no issues detected"),
            "Should show no-issues message"
        );
    }

    #[test]
    fn test_render_issues_malformed_fallback() {
        let bad_xml = "random text";
        let output = render_issues(bad_xml, &None);

        assert!(output.contains("⚠️"), "Should show warning");
    }

    // =========================================================================
    // Fix Result Renderer Tests
    // =========================================================================

    #[test]
    fn test_render_fix_result_all_addressed() {
        let xml = r#"<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
<ralph-summary>Fixed all 3 reported issues</ralph-summary>
</ralph-fix-result>"#;

        let ctx = Some(XmlOutputContext {
            iteration: None,
            pass: Some(2),
            snippets: Vec::new(),
        });
        let output = render_fix_result(xml, &ctx);

        assert!(output.contains("Fix Pass 2"), "Should show pass number");
        assert!(output.contains("✅"), "Should show success emoji");
        assert!(
            output.contains("All Issues Addressed"),
            "Should show friendly status label"
        );
        assert!(output.contains("Fixed all 3"), "Should show summary");
    }

    #[test]
    fn test_render_fix_result_renders_diff_like_view_when_summary_contains_diff() {
        let xml = r#"<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
<ralph-summary>Applied fix:
diff --git a/src/a.rs b/src/a.rs
deleted file mode 100644
--- a/src/a.rs
+++ /dev/null
@@ -1 +0,0 @@
-fn a() {}
</ralph-summary>
</ralph-fix-result>"#;

        let output = render_fix_result(xml, &None);

        assert!(
            output.contains("src/a.rs"),
            "Should include per-file header derived from diff"
        );
        assert!(
            output.contains("deleted") || output.contains("Deleted"),
            "Should include action context for deleted file"
        );
        assert!(
            output.contains("--- a/src/a.rs") && output.contains("+++ /dev/null"),
            "Should include diff markers"
        );
    }

    #[test]
    fn test_render_fix_result_issues_remain() {
        let xml = r#"<ralph-fix-result>
<ralph-status>issues_remain</ralph-status>
</ralph-fix-result>"#;

        let output = render_fix_result(xml, &None);

        assert!(output.contains("🔄"), "Should show partial emoji");
        assert!(
            output.contains("Issues Remain"),
            "Should show friendly status label"
        );
    }

    #[test]
    fn test_render_fix_result_no_issues() {
        let xml = r#"<ralph-fix-result>
<ralph-status>no_issues_found</ralph-status>
</ralph-fix-result>"#;

        let output = render_fix_result(xml, &None);

        assert!(output.contains("✨"), "Should show sparkle emoji");
    }

    // =========================================================================
    // Commit Message Renderer Tests
    // =========================================================================

    #[test]
    fn test_render_commit_with_subject_and_body() {
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add new authentication system</ralph-subject>
<ralph-body>This commit introduces a new JWT-based authentication system.

- Added auth middleware
- Created user session management
- Updated API endpoints</ralph-body>
</ralph-commit>"#;

        let output = render_commit(xml);

        assert!(
            output.contains("Commit Message"),
            "Should have commit header"
        );
        assert!(
            output.contains("feat: add new authentication"),
            "Should show subject"
        );
        assert!(
            output.contains("JWT-based authentication"),
            "Should show body"
        );
        assert!(
            output.contains("Added auth middleware"),
            "Should show body details"
        );
    }

    #[test]
    fn test_render_commit_subject_only() {
        let xml = r#"<ralph-commit>
<ralph-subject>fix: resolve null pointer exception</ralph-subject>
</ralph-commit>"#;

        let output = render_commit(xml);

        assert!(
            output.contains("fix: resolve null pointer"),
            "Should show subject"
        );
    }

    #[test]
    fn test_render_commit_falls_back_to_raw_with_warning_when_subject_is_blank() {
        let xml = r#"<ralph-commit>
<ralph-subject>   </ralph-subject>
</ralph-commit>"#;

        let output = render_commit(xml);

        assert!(output.contains("⚠️"), "Should warn on parse failure");
        assert!(
            output.contains("<ralph-commit>"),
            "Should include raw XML fallback"
        );
        assert!(
            !output.contains("📝 \n"),
            "Should not render an empty subject line"
        );
    }

    // =========================================================================
    // Render XML Router Tests
    // =========================================================================

    #[test]
    fn test_render_xml_routes_correctly() {
        let dev_result = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Done</ralph-summary>
</ralph-development-result>"#;

        let output = render_xml(&XmlOutputType::DevelopmentResult, dev_result, &None);
        assert!(
            output.contains("✅"),
            "Should route to development result renderer"
        );

        let issues = r#"<ralph-issues>
<ralph-issue>Test issue</ralph-issue>
</ralph-issues>"#;

        let output = render_xml(&XmlOutputType::ReviewIssues, issues, &None);
        assert!(
            output.contains("1 issue"),
            "Should route to issues renderer"
        );
    }

    // =========================================================================
    // Extract Tag Content Tests
    // =========================================================================

    #[test]
    fn test_extract_tag_content_found() {
        let xml = "<ralph-subject>Hello World</ralph-subject>";
        let result = extract_tag_content(xml, "ralph-subject");
        assert_eq!(result, Some("Hello World".to_string()));
    }

    #[test]
    fn test_extract_tag_content_not_found() {
        let xml = "<other>content</other>";
        let result = extract_tag_content(xml, "ralph-subject");
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_tag_content_nested() {
        let xml = "<outer><ralph-subject>Nested</ralph-subject></outer>";
        let result = extract_tag_content(xml, "ralph-subject");
        assert_eq!(result, Some("Nested".to_string()));
    }

    // =========================================================================
    // Enhanced Plan Renderer Tests
    // =========================================================================

    #[test]
    fn test_render_plan_shows_step_priorities() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test context</context>
<scope-items>
<scope-item count="1">item 1</scope-item>
<scope-item count="2">item 2</scope-item>
<scope-item count="3">item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" priority="critical" type="file-change">
<title>Critical step</title>
<target-files><file path="src/main.rs" action="modify"/></target-files>
<content><paragraph>Do something critical</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="src/main.rs" action="modify"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair severity="high"><risk>Test risk</risk><mitigation>Test mitigation</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Run tests</method><expected-outcome>All pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let output = render_plan(xml);
        assert!(output.contains("critical"), "Should show priority badge");
        assert!(output.contains("🔴"), "Should show critical icon");
    }

    #[test]
    fn test_render_plan_shows_step_dependencies() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test context</context>
<scope-items>
<scope-item count="1">item 1</scope-item>
<scope-item count="2">item 2</scope-item>
<scope-item count="3">item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>First step</title>
<target-files><file path="src/a.rs" action="create"/></target-files>
<content><paragraph>Create file A</paragraph></content>
</step>
<step number="2" type="file-change">
<title>Second step</title>
<target-files><file path="src/b.rs" action="create"/></target-files>
<depends-on step="1"/>
<content><paragraph>Create file B</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="src/a.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>None</risk><mitigation>N/A</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Run tests</method><expected-outcome>Pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let output = render_plan(xml);
        assert!(output.contains("Depends on"), "Should show dependencies");
        assert!(output.contains("Step 1"), "Should list dependent step");
    }

    #[test]
    fn test_render_plan_shows_verification_strategy() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test context</context>
<scope-items>
<scope-item count="1">item 1</scope-item>
<scope-item count="2">item 2</scope-item>
<scope-item count="3">item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Test step</title>
<target-files><file path="src/main.rs" action="modify"/></target-files>
<content><paragraph>Modify</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="src/main.rs" action="modify"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>None</risk><mitigation>N/A</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>cargo test</method><expected-outcome>All tests pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let output = render_plan(xml);
        assert!(
            output.contains("Verification Strategy"),
            "Should show verification section"
        );
        assert!(output.contains("cargo test"), "Should show method");
        assert!(output.contains("Expected"), "Should show expected outcome");
    }

    // =========================================================================
    // Enhanced Issues Renderer Tests
    // =========================================================================

    #[test]
    fn test_extract_file_from_issue_pattern_in() {
        let issue = "Unused variable in src/main.rs";
        let file = extract_file_from_issue(issue);
        assert_eq!(file, Some("src/main.rs"));
    }

    #[test]
    fn test_extract_file_from_issue_pattern_at() {
        let issue = "Error at src/lib.rs:42 - missing semicolon";
        let file = extract_file_from_issue(issue);
        assert_eq!(file, Some("src/lib.rs"));
    }

    #[test]
    fn test_extract_file_from_issue_no_file() {
        let issue = "General code quality concern";
        let file = extract_file_from_issue(issue);
        assert!(file.is_none());
    }

    #[test]
    fn test_render_issues_celebration_on_approval() {
        let xml = r#"<ralph-issues>
<ralph-no-issues-found>All code looks great!</ralph-no-issues-found>
</ralph-issues>"#;

        let output = render_issues(xml, &None);
        assert!(output.contains("🎉"), "Should celebrate approval");
        assert!(
            output.contains("Code Approved"),
            "Should show approval message"
        );
    }

    #[test]
    fn test_render_issues_shows_snippet_from_context_when_not_in_issue_text() {
        let xml = r#"<ralph-issues>
<ralph-issue>[High] src/lib.rs:2 Missing semicolon</ralph-issue>
</ralph-issues>"#;

        let ctx = Some(XmlOutputContext {
            iteration: None,
            pass: Some(1),
            snippets: vec![XmlCodeSnippet {
                file: "src/lib.rs".to_string(),
                line_start: 1,
                line_end: 3,
                content: "fn example() {\n    let x = 1\n}\n".to_string(),
            }],
        });

        let output = render_issues(xml, &ctx);

        assert!(
            output.contains("fn example()"),
            "Should render snippet content when provided via context: {}",
            output
        );
        assert!(
            output.contains("src/lib.rs"),
            "Should show file context: {}",
            output
        );
    }

    // =========================================================================
    // Visual Consistency Tests
    // =========================================================================

    #[test]
    fn test_all_renderers_have_header_boxes() {
        // Verify consistent visual structure across all renderers
        let plan_output = render_plan("<ralph-plan>invalid</ralph-plan>");
        let issues_output = render_issues("<ralph-issues>invalid</ralph-issues>", &None);
        let commit_output = render_commit("<ralph-commit>invalid</ralph-commit>");

        // All should have box-drawing characters for headers
        assert!(plan_output.contains("═"), "Plan should have box header");
        assert!(issues_output.contains("═"), "Issues should have box header");
        assert!(commit_output.contains("═"), "Commit should have box header");
    }

    #[test]
    fn test_development_result_multiline_summary() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>First line of summary
Second line of summary
Third line of summary</ralph-summary>
</ralph-development-result>"#;

        let output = render_development_result(xml, &None);
        assert!(
            output.contains("First line"),
            "Should show first line of summary"
        );
        assert!(
            output.contains("Second line"),
            "Should show second line of summary"
        );
        assert!(
            output.contains("Third line"),
            "Should show third line of summary"
        );
    }

    #[test]
    fn test_development_result_file_action_icons() {
        let xml = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Changes made</ralph-summary>
<ralph-files-changed>src/new_file.rs (created)
src/existing.rs
src/old.rs (deleted)</ralph-files-changed>
</ralph-development-result>"#;

        let output = render_development_result(xml, &None);
        assert!(
            output.contains("src/new_file.rs") && output.contains("Action: created"),
            "Should show created action for new file"
        );
        assert!(
            output.contains("src/old.rs") && output.contains("Action: deleted"),
            "Should show deleted action for removed file"
        );
        assert!(
            output.contains("src/existing.rs") && output.contains("Action: modified"),
            "Should show modified action for existing file"
        );
    }

    #[test]
    fn test_render_plan_file_action_icons() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item count="1">create</scope-item>
<scope-item count="1">modify</scope-item>
<scope-item count="1">delete</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Create file</title>
<target-files><file path="src/new.rs" action="create"/></target-files>
<content><paragraph>Create</paragraph></content>
</step>
<step number="2" type="file-change">
<title>Modify file</title>
<target-files><file path="src/existing.rs" action="modify"/></target-files>
<content><paragraph>Modify</paragraph></content>
</step>
<step number="3" type="file-change">
<title>Delete file</title>
<target-files><file path="src/old.rs" action="delete"/></target-files>
<content><paragraph>Delete</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="src/new.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>None</risk><mitigation>N/A</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Test</method><expected-outcome>Pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let output = render_plan(xml);
        assert!(output.contains("➕"), "Should show create icon");
        assert!(output.contains("📝"), "Should show modify icon");
        assert!(output.contains("🗑️"), "Should show delete icon");
    }
}
