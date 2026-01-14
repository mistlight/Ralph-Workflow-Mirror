//! Tests for Review Metrics
//!
//! Unit tests for issue parsing, severity detection, and metrics calculation.

use super::metrics::ReviewMetrics;
use super::severity::IssueSeverity;

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
    let content = r"# Issues

- [ ] Critical: [main.rs:1] SQL injection vulnerability
- [x] High: [auth.rs:50] Password hash weakness
- [ ] Medium: [api.rs:100] Missing rate limiting
- [x] Low: [utils.rs:30] Unused import
";
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
    let content = r"
- [ ] Critical: Unresolved critical
- [x] Critical: Resolved critical
- [ ] High: Unresolved high
- [ ] Medium: Unresolved medium
";
    let metrics = ReviewMetrics::from_issues_content(content);

    assert_eq!(metrics.unresolved_blocking_issues(), 2);
    assert!(metrics.has_blocking_issues());
}

#[test]
fn test_summary_format() {
    let content = r"
- [ ] Critical: Issue 1
- [x] High: Issue 2
";
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
    let content = r"
# Issues

- Critical: Security vulnerability
- High: Memory leak
";
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
    let content = r"
- [ ] Critical: Unresolved critical issue
- [x] High: Resolved high issue
- Medium: Plain list medium issue
";
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
    let content = r"
- [ ] Critical: Issue 1
- [x] High: Issue 2
- [ ] Medium: Issue 3
- [x] Low: Issue 4
";
    let metrics = ReviewMetrics::from_issues_content(content);

    assert_eq!(metrics.unresolved_issues(), 2);
}

#[test]
fn test_has_blocking_issues_only_critical_high() {
    // Medium and low issues shouldn't be blocking
    let content = r"
- [ ] Medium: Code style issue
- [ ] Low: Minor improvement needed
";
    let metrics = ReviewMetrics::from_issues_content(content);

    assert!(!metrics.has_blocking_issues());
    assert_eq!(metrics.unresolved_blocking_issues(), 0);
}

#[test]
fn test_has_blocking_issues_with_resolved_critical() {
    // Resolved critical/high issues shouldn't be blocking
    let content = r"
- [x] Critical: Fixed security issue
- [x] High: Fixed memory leak
- [ ] Medium: Pending style fix
";
    let metrics = ReviewMetrics::from_issues_content(content);

    assert!(!metrics.has_blocking_issues());
}

#[test]
fn test_resolution_rate_partial() {
    let content = r"
- [x] Critical: Fixed
- [ ] High: Pending
- [x] Medium: Fixed
- [ ] Low: Pending
";
    let metrics = ReviewMetrics::from_issues_content(content);

    // 2 out of 4 = 50%
    assert!((metrics.resolution_rate() - 50.0).abs() < 0.01);
}

#[test]
fn test_resolution_rate_all_resolved() {
    let content = r"
- [x] Critical: Fixed
- [x] High: Fixed
";
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
    let content = r"
- [ ] Critical: Security issue
- [ ] High: Bug
- [x] Medium: Fixed style
";
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
    let content = r"
    - [ ]   Critical:   [  file.rs:10  ]   Spaced issue

";
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
