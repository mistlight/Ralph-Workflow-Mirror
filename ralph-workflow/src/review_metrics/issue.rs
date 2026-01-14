//! Issue Type
//!
//! Defines the `Issue` struct representing a single issue from ISSUES.md.

use super::severity::IssueSeverity;

/// A single issue extracted from ISSUES.md
#[derive(Debug, Clone)]
pub struct Issue {
    /// Severity of the issue
    pub(crate) severity: IssueSeverity,
    /// Whether the issue has been resolved (checked off)
    pub(crate) resolved: bool,
    /// File path mentioned in the issue (if any)
    pub(crate) file_path: Option<String>,
    /// Line number mentioned in the issue (if any)
    pub(crate) line_number: Option<u32>,
    /// Description of the issue
    pub(crate) description: String,
}

impl Issue {
    /// Generate a human-readable summary of this issue.
    pub(crate) fn summary(&self) -> String {
        let location = match (&self.file_path, self.line_number) {
            (Some(path), Some(line)) => format!("{path}:{line}"),
            (Some(path), None) => path.clone(),
            (None, Some(line)) => format!("line {line}"),
            (None, None) => "unknown location".to_string(),
        };
        format!(
            "{}: {} ({})",
            self.severity,
            self.description.trim(),
            location
        )
    }
}
