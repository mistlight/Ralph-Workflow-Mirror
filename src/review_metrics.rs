//! Review Quality Metrics Module
//!
//! Tracks and reports on review quality and pipeline effectiveness.
//! Parses `.agent/ISSUES.md` to extract issue counts by severity,
//! measures fix success rate, and provides summary statistics.

#![deny(unsafe_code)]

use std::fs;
use std::io;
use std::path::Path;

/// Issue severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum IssueSeverity {
    Critical,
    High,
    Medium,
    Low,
}

impl IssueSeverity {
    /// Parse severity from a string
    fn from_str(s: &str) -> Option<Self> {
        let lower = s.to_lowercase();
        if lower.contains("critical") {
            Some(IssueSeverity::Critical)
        } else if lower.contains("high") {
            Some(IssueSeverity::High)
        } else if lower.contains("medium") {
            Some(IssueSeverity::Medium)
        } else if lower.contains("low") {
            Some(IssueSeverity::Low)
        } else {
            None
        }
    }
}

impl std::fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueSeverity::Critical => write!(f, "Critical"),
            IssueSeverity::High => write!(f, "High"),
            IssueSeverity::Medium => write!(f, "Medium"),
            IssueSeverity::Low => write!(f, "Low"),
        }
    }
}

/// A single issue extracted from ISSUES.md
#[derive(Debug, Clone)]
pub(crate) struct Issue {
    /// Severity of the issue
    pub(crate) severity: IssueSeverity,
    /// Whether the issue has been resolved (checked off)
    pub(crate) resolved: bool,
    /// File path mentioned in the issue (if any)
    /// Note: populated during parsing for future use (e.g., detailed issue display)
    #[allow(dead_code)]
    pub(crate) file_path: Option<String>,
    /// Line number mentioned in the issue (if any)
    /// Note: populated during parsing for future use (e.g., detailed issue display)
    #[allow(dead_code)]
    pub(crate) line_number: Option<u32>,
    /// Description of the issue
    /// Note: populated during parsing for future use (e.g., detailed issue display)
    #[allow(dead_code)]
    pub(crate) description: String,
}

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
}

/// Extract description from an issue line
fn extract_description(line: &str, severity_str: &str) -> String {
    // Find where severity marker ends
    let lower = line.to_lowercase();
    if let Some(pos) = lower.find(&severity_str.to_lowercase()) {
        let after_severity = &line[pos + severity_str.len()..];
        // Skip any : or whitespace
        let desc = after_severity.trim_start_matches(':').trim();
        // Remove file:line reference if present
        if let Some(start) = desc.find('[') {
            if let Some(end) = desc.find(']') {
                if start < end {
                    let before = desc[..start].trim();
                    let after = desc[end + 1..].trim();
                    return format!("{}{}", before, after).trim().to_string();
                }
            }
        }
        desc.to_string()
    } else {
        line.to_string()
    }
}

/// Extract file path and line number from issue line
fn extract_file_line(line: &str) -> (Option<String>, Option<u32>) {
    // Look for [file:line] or [file] pattern
    if let Some(start) = line.find('[') {
        if let Some(end) = line.find(']') {
            if start < end {
                let reference = &line[start + 1..end];
                if let Some(colon_pos) = reference.rfind(':') {
                    let file = reference[..colon_pos].trim();
                    let line_str = reference[colon_pos + 1..].trim();
                    if let Ok(line_num) = line_str.parse::<u32>() {
                        return (Some(file.to_string()), Some(line_num));
                    }
                }
                // Just file, no line number
                if !reference.is_empty() {
                    return (Some(reference.to_string()), None);
                }
            }
        }
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_content() {
        let metrics = ReviewMetrics::from_issues_content("");
        assert_eq!(metrics.total_issues, 0);
        assert!(metrics.issues_file_found);
    }

    #[test]
    fn test_parse_no_issues_declaration() {
        let content = "# Review Results\n\nNo issues found.";
        let metrics = ReviewMetrics::from_issues_content(content);
        assert!(metrics.no_issues_declared);
        assert_eq!(metrics.total_issues, 0);
    }

    #[test]
    fn test_parse_single_issue() {
        let content = "- [ ] Critical: [src/main.rs:42] Memory leak in handler";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 1);
        assert_eq!(metrics.critical_issues, 1);
        assert_eq!(metrics.resolved_issues, 0);
        assert_eq!(metrics.issues.len(), 1);

        let issue = &metrics.issues[0];
        assert_eq!(issue.severity, IssueSeverity::Critical);
        assert!(!issue.resolved);
        assert_eq!(issue.file_path, Some("src/main.rs".to_string()));
        assert_eq!(issue.line_number, Some(42));
    }

    #[test]
    fn test_parse_resolved_issue() {
        let content = "- [x] High: [lib.rs:10] Fixed null pointer";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 1);
        assert_eq!(metrics.high_issues, 1);
        assert_eq!(metrics.resolved_issues, 1);
        assert!(metrics.issues[0].resolved);
    }

    #[test]
    fn test_parse_multiple_issues() {
        let content = r#"# Issues

- [ ] Critical: [main.rs:1] SQL injection vulnerability
- [x] High: [auth.rs:50] Password hash weakness
- [ ] Medium: [api.rs:100] Missing rate limiting
- [x] Low: [utils.rs:30] Unused import
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 4);
        assert_eq!(metrics.critical_issues, 1);
        assert_eq!(metrics.high_issues, 1);
        assert_eq!(metrics.medium_issues, 1);
        assert_eq!(metrics.low_issues, 1);
        assert_eq!(metrics.resolved_issues, 2);
        assert!((metrics.resolution_rate() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_unresolved_blocking_issues() {
        let content = r#"
- [ ] Critical: Unresolved critical
- [x] Critical: Resolved critical
- [ ] High: Unresolved high
- [ ] Medium: Unresolved medium
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.unresolved_blocking_issues(), 2);
        assert!(metrics.has_blocking_issues());
    }

    #[test]
    fn test_summary_format() {
        let content = r#"
- [ ] Critical: Issue 1
- [x] High: Issue 2
"#;
        let metrics = ReviewMetrics::from_issues_content(content);
        let summary = metrics.summary();

        assert!(summary.contains("2 issues"));
        assert!(summary.contains("1 critical"));
        assert!(summary.contains("1 high"));
        assert!(summary.contains("1 resolved"));
        assert!(summary.contains("50%"));
    }

    #[test]
    fn test_detailed_summary_format() {
        let content = "- [ ] High: Test issue";
        let metrics = ReviewMetrics::from_issues_content(content);
        let detailed = metrics.detailed_summary();

        assert!(detailed.contains("High"));
        assert!(detailed.contains("Resolved"));
    }

    #[test]
    fn test_issue_without_file_reference() {
        let content = "- [ ] Medium: General code quality issue";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 1);
        assert_eq!(metrics.issues[0].file_path, None);
        assert_eq!(metrics.issues[0].line_number, None);
    }

    #[test]
    fn test_issue_with_only_file_no_line() {
        let content = "- [ ] Low: [README.md] Documentation needs update";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 1);
        assert_eq!(metrics.issues[0].file_path, Some("README.md".to_string()));
        assert_eq!(metrics.issues[0].line_number, None);
    }

    #[test]
    fn test_resolution_rate_no_issues() {
        let metrics = ReviewMetrics::new();
        assert!((metrics.resolution_rate() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_severity_from_str() {
        assert_eq!(
            IssueSeverity::from_str("Critical"),
            Some(IssueSeverity::Critical)
        );
        assert_eq!(IssueSeverity::from_str("HIGH"), Some(IssueSeverity::High));
        assert_eq!(
            IssueSeverity::from_str("medium issue"),
            Some(IssueSeverity::Medium)
        );
        assert_eq!(IssueSeverity::from_str("low:"), Some(IssueSeverity::Low));
        assert_eq!(IssueSeverity::from_str("unknown"), None);
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(format!("{}", IssueSeverity::Critical), "Critical");
        assert_eq!(format!("{}", IssueSeverity::High), "High");
        assert_eq!(format!("{}", IssueSeverity::Medium), "Medium");
        assert_eq!(format!("{}", IssueSeverity::Low), "Low");
    }

    #[test]
    fn test_parse_uppercase_checkbox() {
        let content = "- [X] Low: Fixed issue";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 1);
        assert!(metrics.issues[0].resolved);
    }

    // ============================================================================
    // Additional Edge Case Tests
    // ============================================================================

    #[test]
    fn test_parse_all_issues_resolved_declaration() {
        let content = "# Review Complete\n\nAll issues resolved. Great work!";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert!(metrics.no_issues_declared);
        assert_eq!(metrics.total_issues, 0);
    }

    #[test]
    fn test_parse_plain_list_items() {
        // Test parsing list items without checkboxes
        let content = r#"
# Issues

- Critical: Security vulnerability
- High: Memory leak
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 2);
        assert_eq!(metrics.critical_issues, 1);
        assert_eq!(metrics.high_issues, 1);
        // Without checkbox, should default to unresolved
        assert_eq!(metrics.resolved_issues, 0);
    }

    #[test]
    fn test_parse_mixed_format() {
        // Test mixed checkbox and plain list items
        let content = r#"
- [ ] Critical: Unresolved critical issue
- [x] High: Resolved high issue
- Medium: Plain list medium issue
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 3);
        assert_eq!(metrics.critical_issues, 1);
        assert_eq!(metrics.high_issues, 1);
        assert_eq!(metrics.medium_issues, 1);
        assert_eq!(metrics.resolved_issues, 1);
    }

    #[test]
    fn test_parse_nested_file_paths() {
        let content = "- [ ] High: [src/handlers/api/v2/users.rs:142] Potential SQL injection";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 1);
        let issue = &metrics.issues[0];
        assert_eq!(
            issue.file_path,
            Some("src/handlers/api/v2/users.rs".to_string())
        );
        assert_eq!(issue.line_number, Some(142));
    }

    #[test]
    fn test_unresolved_issues_count() {
        let content = r#"
- [ ] Critical: Issue 1
- [x] High: Issue 2
- [ ] Medium: Issue 3
- [x] Low: Issue 4
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.unresolved_issues(), 2);
    }

    #[test]
    fn test_has_blocking_issues_only_critical_high() {
        // Medium and low issues shouldn't be blocking
        let content = r#"
- [ ] Medium: Code style issue
- [ ] Low: Minor improvement needed
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        assert!(!metrics.has_blocking_issues());
        assert_eq!(metrics.unresolved_blocking_issues(), 0);
    }

    #[test]
    fn test_has_blocking_issues_with_resolved_critical() {
        // Resolved critical/high issues shouldn't be blocking
        let content = r#"
- [x] Critical: Fixed security issue
- [x] High: Fixed memory leak
- [ ] Medium: Pending style fix
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        assert!(!metrics.has_blocking_issues());
    }

    #[test]
    fn test_resolution_rate_partial() {
        let content = r#"
- [x] Critical: Fixed
- [ ] High: Pending
- [x] Medium: Fixed
- [ ] Low: Pending
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        // 2 out of 4 = 50%
        assert!((metrics.resolution_rate() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_resolution_rate_all_resolved() {
        let content = r#"
- [x] Critical: Fixed
- [x] High: Fixed
"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        assert!((metrics.resolution_rate() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_detailed_summary_no_file() {
        let metrics = ReviewMetrics::new();
        let summary = metrics.detailed_summary();
        assert!(summary.contains("No ISSUES.md found"));
    }

    #[test]
    fn test_detailed_summary_no_issues() {
        let content = "# Review\n\nNo issues found.";
        let metrics = ReviewMetrics::from_issues_content(content);
        let summary = metrics.detailed_summary();
        assert!(summary.contains("No issues found"));
    }

    #[test]
    fn test_detailed_summary_with_issues() {
        let content = r#"
- [ ] Critical: Security issue
- [ ] High: Bug
- [x] Medium: Fixed style
"#;
        let metrics = ReviewMetrics::from_issues_content(content);
        let summary = metrics.detailed_summary();

        assert!(summary.contains("Critical"));
        assert!(summary.contains("High"));
        assert!(summary.contains("Medium"));
        assert!(summary.contains("Resolved"));
    }

    #[test]
    fn test_summary_no_file() {
        let metrics = ReviewMetrics::new();
        let summary = metrics.summary();
        assert!(summary.contains("No ISSUES.md found"));
    }

    #[test]
    fn test_malformed_file_reference() {
        // Brackets without proper format
        let content = "- [ ] High: [malformed] Some issue";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 1);
        // Should still parse but treat as file without line number
        let issue = &metrics.issues[0];
        assert_eq!(issue.file_path, Some("malformed".to_string()));
        assert_eq!(issue.line_number, None);
    }

    #[test]
    fn test_whitespace_handling() {
        let content = r#"
    - [ ]   Critical:   [  file.rs:10  ]   Spaced issue

"#;
        let metrics = ReviewMetrics::from_issues_content(content);

        // Should handle extra whitespace gracefully
        assert_eq!(metrics.total_issues, 1);
        assert_eq!(metrics.critical_issues, 1);
    }

    #[test]
    fn test_multiple_severity_keywords_takes_first() {
        // If multiple severity keywords, should take the first match
        let content = "- [ ] Critical issue with High impact and Medium priority";
        let metrics = ReviewMetrics::from_issues_content(content);

        assert_eq!(metrics.total_issues, 1);
        assert_eq!(metrics.critical_issues, 1);
        assert_eq!(metrics.high_issues, 0);
        assert_eq!(metrics.medium_issues, 0);
    }

    #[test]
    fn test_issue_severity_equality() {
        assert_eq!(IssueSeverity::Critical, IssueSeverity::Critical);
        assert_ne!(IssueSeverity::Critical, IssueSeverity::High);
    }

    #[test]
    fn test_review_metrics_default() {
        let metrics = ReviewMetrics::default();

        assert_eq!(metrics.total_issues, 0);
        assert_eq!(metrics.critical_issues, 0);
        assert_eq!(metrics.high_issues, 0);
        assert_eq!(metrics.medium_issues, 0);
        assert_eq!(metrics.low_issues, 0);
        assert_eq!(metrics.resolved_issues, 0);
        assert!(metrics.issues.is_empty());
        assert!(!metrics.issues_file_found);
        assert!(!metrics.no_issues_declared);
    }

    #[test]
    fn test_extract_file_line_various_formats() {
        // Test the extract_file_line helper with various formats
        let content1 = "- [ ] High: [src/main.rs:100] Issue";
        let metrics1 = ReviewMetrics::from_issues_content(content1);
        assert_eq!(
            metrics1.issues[0].file_path,
            Some("src/main.rs".to_string())
        );
        assert_eq!(metrics1.issues[0].line_number, Some(100));

        // Windows-style path
        let content2 = "- [ ] High: [src\\main.rs:50] Issue";
        let metrics2 = ReviewMetrics::from_issues_content(content2);
        assert_eq!(
            metrics2.issues[0].file_path,
            Some("src\\main.rs".to_string())
        );
        assert_eq!(metrics2.issues[0].line_number, Some(50));
    }
}
