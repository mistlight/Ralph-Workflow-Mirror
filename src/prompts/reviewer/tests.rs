use super::*;
use crate::language_detector::ProjectStack;
use crate::prompts::types::ContextLevel;
use crate::guidelines::ReviewGuidelines;

#[test]
fn prompt_reviewer_review_mentions_fresh_eyes_in_minimal() {
    let result = prompt_reviewer_review(ContextLevel::Minimal);
    assert!(result.contains("fresh eyes"));
    assert!(result.contains("REVIEW MODE"));
    assert!(result.contains(".agent/ISSUES.md"));
    assert!(!result.contains("Priority Guide"));
}

#[test]
fn prompt_detailed_review_without_guidelines_is_actionable() {
    let result = prompt_detailed_review_without_guidelines(ContextLevel::Minimal);
    assert!(result.contains("DETAILED REVIEW MODE"));
    assert!(result.contains("prioritized checklist"));
    assert!(result.contains("- [ ] Critical"));
    assert!(!result.contains("exactly ONE vague sentence"));
}

#[test]
fn prompt_reviewer_review_with_guidelines_includes_guideline_section() {
    let stack = ProjectStack {
        primary_language: "Rust".to_string(),
        frameworks: vec!["Actix".to_string()],
        has_tests: true,
        test_framework: Some("cargo test".to_string()),
        package_manager: Some("Cargo".to_string()),
        ..Default::default()
    };
    let guidelines = ReviewGuidelines::for_stack(&stack);

    let result = prompt_reviewer_review_with_guidelines(ContextLevel::Minimal, &guidelines);
    assert!(result.contains("Language-Specific"));
    assert!(result.contains("SECURITY"));
}

#[test]
fn prompt_security_review_contains_owasp_terms() {
    let guidelines = ReviewGuidelines::default();
    let result = prompt_security_focused_review(ContextLevel::Minimal, &guidelines);
    assert!(result.contains("OWASP TOP 10"));
    assert!(result.contains("Injection"));
}

#[test]
fn prompt_incremental_review_mentions_git_diff() {
    let result = prompt_incremental_review(ContextLevel::Minimal);
    assert!(result.contains("git diff HEAD~1"));
    assert!(result.contains("INCREMENTAL REVIEW MODE"));
}
