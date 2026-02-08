//! Types for representing parsed issues XML.
//!
//! This module defines the `IssuesElements` type, which represents the parsed
//! result of validating issues XML content.

/// Parsed issues elements from valid XML.
///
/// This type represents the result of successfully validating issues XML content.
/// It contains either a list of issues or a "no issues found" message.
///
/// # XML Format
///
/// The XML can take two forms:
///
/// **With issues:**
/// ```xml
/// <ralph-issues>
///   <ralph-issue>First issue description</ralph-issue>
///   <ralph-issue>Second issue description</ralph-issue>
/// </ralph-issues>
/// ```
///
/// **Without issues:**
/// ```xml
/// <ralph-issues>
///   <ralph-no-issues-found>No issues were found during review</ralph-no-issues-found>
/// </ralph-issues>
/// ```
///
/// # Examples
///
/// ```rust
/// use ralph_workflow::files::llm_output_extraction::xsd_validation_issues::IssuesElements;
///
/// // Issues found
/// let issues = IssuesElements {
///     issues: vec!["First issue".to_string(), "Second issue".to_string()],
///     no_issues_found: None,
/// };
/// assert_eq!(issues.issue_count(), 2);
///
/// // No issues found
/// let no_issues = IssuesElements {
///     issues: vec![],
///     no_issues_found: Some("All good".to_string()),
/// };
/// assert!(no_issues.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuesElements {
    /// List of issues (if any)
    pub issues: Vec<String>,
    /// No issues found message (if no issues)
    pub no_issues_found: Option<String>,
}

impl IssuesElements {
    /// Returns true if there are no issues.
    ///
    /// This is true when the issues list is empty and a "no issues found" message exists.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty() && self.no_issues_found.is_some()
    }

    /// Returns the number of issues.
    ///
    /// This is the count of issues in the issues list (does not include "no issues found").
    #[cfg(any(test, feature = "test-utils"))]
    pub fn issue_count(&self) -> usize {
        self.issues.len()
    }
}
