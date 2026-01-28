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

use super::ui_event::{XmlOutputContext, XmlOutputType};
use crate::files::llm_output_extraction::xsd_validation_plan::{FileAction, Priority, Severity};
use crate::files::llm_output_extraction::{
    validate_development_result_xml, validate_fix_result_xml, validate_issues_xml,
    validate_plan_xml,
};

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

            // Files changed with inferred action icons
            if let Some(ref files) = elements.files_changed {
                output.push_str("\n📁 Files Changed:\n");
                for file in files.lines().filter(|l| !l.trim().is_empty()) {
                    let file = file.trim();
                    // Infer action from common patterns (best effort)
                    let icon = if file.contains("(created)") || file.contains("(new)") {
                        "➕"
                    } else if file.contains("(deleted)") || file.contains("(removed)") {
                        "🗑️"
                    } else {
                        "📝"
                    };
                    output.push_str(&format!("   {} {}\n", icon, file));
                }
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
                for (i, issue) in elements.issues.iter().enumerate() {
                    // Try to extract file path from issue text (common patterns)
                    let file_indicator = extract_file_from_issue(issue)
                        .map(|f| format!("📄 {}\n   ", f))
                        .unwrap_or_default();

                    output.push_str(&format!("   {}. {}{}\n", i + 1, file_indicator, issue));
                    if i < elements.issues.len() - 1 {
                        output.push_str("   ───\n");
                    }
                }
            }
        }
        Err(_) => {
            output.push_str("⚠️  Unable to parse issues XML\n\n");
            output.push_str(content);
        }
    }

    output
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
                for line in summary.lines() {
                    output.push_str(&format!("   {}\n", line));
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
    let subject = extract_tag_content(content, "ralph-subject");
    let body = extract_tag_content(content, "ralph-body");

    if let Some(subject) = subject {
        output.push_str(&format!("📝 {}\n", subject.trim()));
    }

    if let Some(body) = body {
        let body = body.trim();
        if !body.is_empty() {
            output.push('\n');
            for line in body.lines() {
                output.push_str(&format!("   {}\n", line));
            }
        }
    }

    // If no content was extracted, show raw
    if output.len() <= 35 {
        output.push_str(content);
    }

    output
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
        assert!(output.contains("Completed"), "Should show friendly status label");
        assert!(
            output.contains("Implemented feature X"),
            "Should show summary"
        );
        assert!(output.contains("src/main.rs"), "Should list files");
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
        assert!(output.contains("Implementation Steps"), "Should show steps section");
        assert!(
            output.contains("1. Add new module"),
            "Should show step title"
        );
        assert!(output.contains("Risks & Mitigations"), "Should show risks section");
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
        });
        let output = render_issues(xml, &ctx);

        assert!(output.contains("Review Pass 1"), "Should show pass number");
        assert!(output.contains("2 issue"), "Should show issue count");
        assert!(output.contains("Variable unused"), "Should list issues");
        assert!(output.contains("1."), "Should number issues");
        assert!(output.contains("2."), "Should number second issue");
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
            output.contains("➕"),
            "Should show create icon for new file"
        );
        assert!(
            output.contains("🗑️"),
            "Should show delete icon for removed file"
        );
        assert!(
            output.contains("📝"),
            "Should show modify icon for existing file"
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
