//! Review Metrics
//!
//! Core `ReviewMetrics` struct and parsing logic for extracting
//! issue counts and resolution rates from ISSUES.md.

use std::fs;
use std::io;
use std::path::Path;
use std::vec::Vec;

use super::severity::IssueSeverity;

/// Review metrics collected from a pipeline run
#[derive(Debug, Clone, Default)]
pub struct ReviewMetrics {
    /// Total number of issues found
    pub(crate) total_issues: u32,
    /// Issues by severity
    pub(crate) critical_issues: u32,
    pub(crate) high_issues: u32,
    pub(crate) medium_issues: u32,
    pub(crate) low_issues: u32,
    /// Number of resolved issues
    pub(crate) resolved_issues: u32,
    /// Whether the issues file was found
    pub(crate) issues_file_found: bool,
    /// Whether no issues were found (explicit statement)
    pub(crate) no_issues_declared: bool,
}

impl ReviewMetrics {
    /// Create new empty metrics
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Parse metrics from ISSUES.md content
    pub(crate) fn from_issues_content(content: &str) -> Self {
        let mut metrics = Self::new();
        metrics.issues_file_found = true;

        // Check for "no issues" declaration
        let content_lower = content.to_lowercase();
        if content_lower.contains("no issues found")
            || content_lower.contains("all issues resolved")
            || content_lower.contains("no issues")
        {
            metrics.no_issues_declared = true;
        }

        // Parse issue lines
        // Format: - [ ] Critical: [file:line] Description
        // Format: - [x] High: [file:line] Description
        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines and headers
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Check for checkbox format
            let (resolved, rest) = if let Some(rest) = trimmed
                .strip_prefix("- [x]")
                .or_else(|| trimmed.strip_prefix("- [X]"))
            {
                (true, rest)
            } else if let Some(rest) = trimmed.strip_prefix("- [ ]") {
                (false, rest)
            } else if let Some(rest) = trimmed.strip_prefix("-") {
                // Plain list item without checkbox
                (false, rest)
            } else {
                continue;
            };

            let rest = rest.trim();

            // Try to extract severity
            if let Some(severity) = IssueSeverity::from_str(rest) {
                // Update counts
                metrics.total_issues += 1;
                if resolved {
                    metrics.resolved_issues += 1;
                }
                match severity {
                    IssueSeverity::Critical => metrics.critical_issues += 1,
                    IssueSeverity::High => metrics.high_issues += 1,
                    IssueSeverity::Medium => metrics.medium_issues += 1,
                    IssueSeverity::Low => metrics.low_issues += 1,
                }
            }
        }

        metrics
    }

    /// Load metrics from the ISSUES.md file
    pub(crate) fn from_issues_file() -> io::Result<Self> {
        let path = Path::new(".agent/ISSUES.md");
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path)?;
        Ok(Self::from_issues_content(&content))
    }

    /// Returns a summary string of the review metrics
    pub(crate) fn summary(&self) -> String {
        if self.no_issues_declared && self.total_issues == 0 {
            "No issues found".to_string()
        } else if self.resolved_issues == self.total_issues {
            format!("All {} issues resolved", self.total_issues)
        } else {
            format!(
                "{}/{} issues resolved",
                self.resolved_issues, self.total_issues
            )
        }
    }

    /// Returns the number of unresolved issues
    pub(crate) fn unresolved_issues(&self) -> u32 {
        self.total_issues.saturating_sub(self.resolved_issues)
    }

    /// Returns a detailed summary of issues by severity
    pub(crate) fn detailed_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.critical_issues > 0 {
            parts.push(format!("{} critical", self.critical_issues));
        }
        if self.high_issues > 0 {
            parts.push(format!("{} high", self.high_issues));
        }
        if self.medium_issues > 0 {
            parts.push(format!("{} medium", self.medium_issues));
        }
        if self.low_issues > 0 {
            parts.push(format!("{} low", self.low_issues));
        }
        if parts.is_empty() {
            "No issues".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// Returns summaries of unresolved issues
    pub(crate) fn unresolved_issue_summaries(&self, limit: usize) -> Vec<String> {
        // This is a simplified version - in a full implementation, this would
        // read the actual issues from the ISSUES.md file and extract summaries
        Vec::new()
    }

    /// Returns whether there are blocking (critical or high) unresolved issues
    pub(crate) fn has_blocking_issues(&self) -> bool {
        let unresolved_critical = self
            .critical_issues
            .saturating_sub(self.critical_issues.min(self.resolved_issues));
        let unresolved_high = self.high_issues.saturating_sub(
            self.high_issues
                .min((self.resolved_issues.saturating_sub(self.critical_issues))),
        );
        unresolved_critical > 0 || unresolved_high > 0
    }

    /// Returns the number of unresolved blocking issues
    pub(crate) fn unresolved_blocking_issues(&self) -> u32 {
        // Simplified: count all critical and high as blocking
        self.critical_issues + self.high_issues
    }
}
