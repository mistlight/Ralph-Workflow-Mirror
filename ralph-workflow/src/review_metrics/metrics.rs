//! Review Metrics
//!
//! Core `ReviewMetrics` struct and parsing logic for extracting
//! issue counts and resolution rates from ISSUES.md.

use std::fs;
use std::io;
use std::path::Path;

use super::severity::IssueSeverity;

/// Parse header-based issue format: `#### [ ] Critical: description`
///
/// Returns the text after the checkbox if it matches, or None if not a header issue format.
fn parse_header_issue_format(line: &str) -> Option<&str> {
    // Strip leading # characters
    let stripped = line.trim_start_matches('#');
    if stripped.len() == line.len() {
        // No # characters found, not a header
        return None;
    }

    let stripped = stripped.trim_start();

    // Check for checkbox format in header
    if let Some(rest) = stripped.strip_prefix("[ ]") {
        return Some(rest.trim_start());
    }
    if let Some(rest) = stripped
        .strip_prefix("[x]")
        .or_else(|| stripped.strip_prefix("[X]"))
    {
        return Some(rest.trim_start());
    }

    None
}

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

        // Check for explicit "no issues" declaration
        // We use line-by-line matching to avoid false positives from phrases like
        // "No critical security issues found" in review text that does contain issues.
        // Only lines that are clearly declarations count.
        let content_lower = content.to_lowercase();
        for line in content_lower.lines() {
            let trimmed = line.trim();
            // Only match explicit declarations, not text that happens to contain the phrase
            // A declaration is typically: "No issues found" or "No issues" at the start of a line
            // or as the entire line (possibly after list markers)
            let cleaned = trimmed
                .trim_start_matches('-')
                .trim_start_matches('*')
                .trim();

            if cleaned == "no issues found"
                || cleaned == "no issues found."
                || cleaned == "no issues"
                || cleaned == "no issues."
                || cleaned == "all issues resolved"
                || cleaned == "all issues resolved."
                // Handle "all issues resolved. <additional text>" pattern
                || cleaned.starts_with("all issues resolved.")
                // Handle "no issues found" at start without severity markers
                || cleaned.starts_with("no issues found")
                    && !cleaned.contains("critical")
                    && !cleaned.contains("high")
                    && !cleaned.contains("medium")
                    && !cleaned.contains("low")
            {
                metrics.no_issues_declared = true;
                break;
            }
        }

        // Parse issue lines
        // Format: - [ ] Critical: [file:line] Description
        // Format: - [x] High: [file:line] Description
        // Format: #### [ ] Critical: [file:line] Description (header-based format)
        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }

            // Try header-based format first (e.g., "#### [ ] Critical:")
            // Some agents output issues with markdown headers containing checkboxes
            if let Some(rest) = parse_header_issue_format(trimmed) {
                if let Some(severity) = IssueSeverity::from_str(rest) {
                    metrics.total_issues += 1;
                    match severity {
                        IssueSeverity::Critical => metrics.critical_issues += 1,
                        IssueSeverity::High => metrics.high_issues += 1,
                        IssueSeverity::Medium => metrics.medium_issues += 1,
                        IssueSeverity::Low => metrics.low_issues += 1,
                    }
                }
                continue;
            }

            // Skip headers that don't contain issue format
            if trimmed.starts_with('#') {
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

        // If we found actual issues, the "no issues declared" flag is invalid
        // This handles cases where review text contains "No issues found" somewhere
        // but the review actually lists issues elsewhere.
        if metrics.total_issues > 0 {
            metrics.no_issues_declared = false;
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
