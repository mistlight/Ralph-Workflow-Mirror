//! Semantic XML renderers for user-friendly output.
//!
//! This module routes XML rendering to type-specific modules.
//! Each XML output type (`DevelopmentResult`, `DevelopmentPlan`, etc.) has
//! a dedicated renderer that transforms raw XML into user-friendly
//! terminal output.
//!
//! # Graceful Degradation
//!
//! If XML parsing fails, renderers fall back to displaying the raw XML
//! with a warning message. This ensures users always see output even if
//! the format is unexpected.

mod commit_message;
mod development_plan;
mod development_result;
mod fix_result;
mod helpers;
mod review_issues;

use crate::reducer::ui_event::{XmlOutputContext, XmlOutputType};

/// Render XML content based on its type.
///
/// Returns formatted string for terminal display.
/// Falls back to raw XML with warning if parsing fails.
#[must_use]
pub fn render_xml(
    xml_type: &XmlOutputType,
    content: &str,
    output_context: &Option<XmlOutputContext>,
) -> String {
    match xml_type {
        XmlOutputType::DevelopmentResult => development_result::render(content, output_context),
        XmlOutputType::DevelopmentPlan => development_plan::render(content),
        XmlOutputType::ReviewIssues => review_issues::render(content, output_context),
        XmlOutputType::FixResult => fix_result::render(content, output_context),
        XmlOutputType::CommitMessage => commit_message::render(content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_xml_routes_to_development_result() {
        let content = r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Done</ralph-summary>
</ralph-development-result>"#;

        let output = render_xml(&XmlOutputType::DevelopmentResult, content, &None);
        assert!(
            output.contains("✅"),
            "Should route to development result renderer"
        );
    }

    #[test]
    fn test_render_xml_routes_to_review_issues() {
        let content = r#"<ralph-issues>
<ralph-issue>Test issue</ralph-issue>
</ralph-issues>"#;

        let output = render_xml(&XmlOutputType::ReviewIssues, content, &None);
        assert!(
            output.contains("1 issue"),
            "Should route to issues renderer"
        );
    }

    #[test]
    fn test_render_xml_routes_to_commit_message() {
        let content = r#"<ralph-commit>
<ralph-subject>feat: add feature</ralph-subject>
</ralph-commit>"#;

        let output = render_xml(&XmlOutputType::CommitMessage, content, &None);
        assert!(
            output.contains("feat: add feature"),
            "Should route to commit renderer"
        );
    }
}
