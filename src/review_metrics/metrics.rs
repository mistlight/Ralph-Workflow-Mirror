//! Review Metrics
//!
//! Core `ReviewMetrics` struct and parsing logic for extracting
//! issue counts and resolution rates from ISSUES.md.

use std::fs;
use std::io;
use std::path::Path;

use super::issue::Issue;
use super::parser::{extract_description, extract_file_line};
use super::severity::IssueSeverity;

/// Review metrics collected from a pipeline run
#[derive(Debug, Clone, Default)]
pub(crate) struct ReviewMetrics {
    /// Total number of issues found
    pub(crate) total_issues: u32,
    /// Issues by severity
    pub(crate) critical_issues: u32,
    pub(crate) high_issues: u32,
    pub(crate) medium_issues: u32,
    pub(crate) low_issues: u32,
    /// Number of resolved issues
    pub(crate) resolved_issues: u32,
    /// Individual issues (for detailed reporting)
    pub(crate) issues: Vec<Issue>,
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
                // Find the description (after severity marker)
                let description = extract_description(rest, &severity.to_string());

                // Try to extract file:line reference
                let (file_path, line_number) = extract_file_line(rest);

                let issue = Issue {
                    severity,
                    resolved,
                    file_path,
                    line_number,
                    description,
                };

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

                metrics.issues.push(issue);
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

    /// Calculate the resolution rate as a percentage
    pub(crate) fn resolution_rate(&self) -> f64 {
        if self.total_issues == 0 {
            100.0
        } else {
            (self.resolved_issues as f64 / self.total_issues as f64) * 100.0
        }
    }

    /// Get unresolved issues count
    pub(crate) fn unresolved_issues(&self) -> u32 {
        self.total_issues.saturating_sub(self.resolved_issues)
    }

    /// Get unresolved critical/high issues count
    pub(crate) fn unresolved_blocking_issues(&self) -> u32 {
        self.issues
            .iter()
            .filter(|i| {
                !i.resolved
                    && (i.severity == IssueSeverity::Critical || i.severity == IssueSeverity::High)
            })
            .count() as u32
    }

    /// Format as a summary string
    pub(crate) fn summary(&self) -> String {
        if !self.issues_file_found {
            return "No ISSUES.md found".to_string();
        }

        if self.no_issues_declared && self.total_issues == 0 {
            return "No issues found".to_string();
        }

        format!(
            "{} issues ({} critical, {} high, {} medium, {} low) | {} resolved ({:.0}%)",
            self.total_issues,
            self.critical_issues,
            self.high_issues,
            self.medium_issues,
            self.low_issues,
            self.resolved_issues,
            self.resolution_rate()
        )
    }

    /// Format detailed breakdown for display
    pub(crate) fn detailed_summary(&self) -> String {
        let mut lines = vec![];

        if !self.issues_file_found {
            return "  No ISSUES.md found".to_string();
        }

        if self.no_issues_declared && self.total_issues == 0 {
            return "  No issues found".to_string();
        }

        if self.critical_issues > 0 {
            lines.push(format!("  Critical: {}", self.critical_issues));
        }
        if self.high_issues > 0 {
            lines.push(format!("  High:     {}", self.high_issues));
        }
        if self.medium_issues > 0 {
            lines.push(format!("  Medium:   {}", self.medium_issues));
        }
        if self.low_issues > 0 {
            lines.push(format!("  Low:      {}", self.low_issues));
        }

        if !lines.is_empty() {
            lines.push(format!(
                "  Resolved: {}/{} ({:.0}%)",
                self.resolved_issues,
                self.total_issues,
                self.resolution_rate()
            ));
        }

        if lines.is_empty() {
            "  No categorized issues found".to_string()
        } else {
            lines.join("\n")
        }
    }

    /// Check if review found any blocking issues (critical or high severity unresolved)
    pub(crate) fn has_blocking_issues(&self) -> bool {
        self.unresolved_blocking_issues() > 0
    }

    /// Return up to `limit` unresolved issues as human-readable one-liners.
    pub(crate) fn unresolved_issue_summaries(&self, limit: usize) -> Vec<String> {
        self.issues
            .iter()
            .filter(|i| !i.resolved)
            .take(limit)
            .map(Issue::summary)
            .collect()
    }
}
