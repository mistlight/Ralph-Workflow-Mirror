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
}

#[test]
fn test_parse_resolved_issue() {
    let content = "- [x] High: [lib.rs:10] Fixed null pointer";
    let metrics = ReviewMetrics::from_issues_content(content);

    assert_eq!(metrics.total_issues, 1);
    assert_eq!(metrics.high_issues, 1);
    assert_eq!(metrics.resolved_issues, 1);
}

#[test]
fn test_parse_multiple_issues() {
    let content = "# Issues

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
}

#[test]
fn test_issue_without_file_reference() {
    let content = "- [ ] Medium: General code quality issue";
    let metrics = ReviewMetrics::from_issues_content(content);

    assert_eq!(metrics.total_issues, 1);
}

#[test]
fn test_issue_with_only_file_no_line() {
    let content = "- [ ] Low: [README.md] Documentation needs update";
    let metrics = ReviewMetrics::from_issues_content(content);

    assert_eq!(metrics.total_issues, 1);
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
}

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
    let content = "
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
    let content = "
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
fn test_whitespace_handling() {
    let content = "
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
    assert!(!metrics.issues_file_found);
    assert!(!metrics.no_issues_declared);
}
