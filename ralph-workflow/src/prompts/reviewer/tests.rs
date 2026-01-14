//! Tests for reviewer prompt generation.
//!
//! Verifies that generated prompts contain expected content for various
//! review modes (standard, detailed, security, incremental).

use super::*;
use crate::guidelines::ReviewGuidelines;
use crate::language_detector::ProjectStack;
use crate::prompts::types::ContextLevel;

#[test]
fn prompt_reviewer_review_mentions_fresh_eyes_in_minimal() {
    let result = prompt_reviewer_review(ContextLevel::Minimal);
    assert!(result.contains("fresh eyes"));
    assert!(result.contains("REVIEW MODE"));
    // Issues are now returned as structured output, not written to file
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
fn prompt_incremental_review_with_diff_provides_diff_inline() {
    use super::unguided::prompt_incremental_review_with_diff;

    let diff = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
     println!("Hello, world!");
+    println!("New line");
 }
"#;

    let result = prompt_incremental_review_with_diff(ContextLevel::Minimal, diff);

    // The diff should be included inline in the prompt
    assert!(result.contains("```diff"));
    assert!(result.contains("fn main()"));
    assert!(result.contains("+    println!(\"New line\");"));

    // Should NOT tell the agent to run any git commands
    assert!(!result.contains("git diff"));
    assert!(!result.contains("git status"));
    assert!(!result.contains("run git"));
    assert!(!result.contains("execute git"));

    // Should indicate this is for reviewing changes
    assert!(result.contains("INCREMENTAL REVIEW MODE"));
    assert!(result.contains("DIFF TO REVIEW"));
}

#[test]
fn all_reviewer_prompts_isolate_agents_from_git() {
    // Verify none of the active reviewer prompts tell agents to run git commands
    let prompts = vec![
        prompt_reviewer_review(ContextLevel::Minimal),
        prompt_reviewer_review(ContextLevel::Normal),
        prompt_detailed_review_without_guidelines(ContextLevel::Minimal),
        prompt_detailed_review_without_guidelines(ContextLevel::Normal),
        prompt_universal_review(ContextLevel::Minimal),
        prompt_universal_review(ContextLevel::Normal),
    ];

    for prompt in prompts {
        assert!(
            !prompt.contains("git diff"),
            "Prompt should not tell agent to run git diff: {}",
            &prompt[..100.min(prompt.len())]
        );
        assert!(
            !prompt.contains("git status"),
            "Prompt should not tell agent to run git status"
        );
        assert!(
            !prompt.contains("run git"),
            "Prompt should not tell agent to run git commands"
        );
    }
}
