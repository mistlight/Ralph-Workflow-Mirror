//! Review Metrics
//!
//! Core `ReviewMetrics` struct and parsing logic for extracting
//! issue counts and resolution rates from ISSUES.md.

use std::fs;
use std::io;
use std::path::Path;

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
}
