//! Tests for reviewer prompt generation.
//!
//! Verifies that generated prompts contain expected content for various
//! review modes (standard, detailed, security, incremental).

use super::*;
use crate::guidelines::ReviewGuidelines;
use crate::language_detector::ProjectStack;
use crate::prompts::types::ContextLevel;

const SAMPLE_DIFF: &str = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
     println!("Hello, world!");
+    println!("New line");
 }
"#;

#[test]
fn prompt_reviewer_review_with_diff_mentions_fresh_eyes_in_minimal() {
    let result =
        prompt_detailed_review_without_guidelines_with_diff(ContextLevel::Minimal, SAMPLE_DIFF);
    assert!(result.contains("fresh eyes"));
    assert!(result.contains("DETAILED REVIEW MODE"));
    // Issues are now returned as structured output, not written to file
    assert!(!result.contains("Priority Guide"));
}

#[test]
fn prompt_detailed_review_without_guidelines_with_diff_is_actionable() {
    let result =
        prompt_detailed_review_without_guidelines_with_diff(ContextLevel::Minimal, SAMPLE_DIFF);
    assert!(result.contains("DETAILED REVIEW MODE"));
    assert!(result.contains("Review ONLY the changes in the DIFF"));
    assert!(result.contains("prioritized checklist"));
    assert!(result.contains("- [ ] Critical"));
    assert!(!result.contains("exactly ONE vague sentence"));
    assert!(result.contains("```diff"));
    assert!(result.contains("fn main()"));
}

#[test]
fn prompt_reviewer_review_with_guidelines_and_diff_includes_guideline_section() {
    let stack = ProjectStack {
        primary_language: "Rust".to_string(),
        frameworks: vec!["Actix".to_string()],
        has_tests: true,
        test_framework: Some("cargo test".to_string()),
        package_manager: Some("Cargo".to_string()),
        ..Default::default()
    };
    let guidelines = ReviewGuidelines::for_stack(&stack);

    let result = prompt_reviewer_review_with_guidelines_and_diff(
        ContextLevel::Minimal,
        &guidelines,
        SAMPLE_DIFF,
    );
    assert!(result.contains("Language-Specific"));
    assert!(result.contains("SECURITY"));
    assert!(result.contains("Review ONLY the changes in the DIFF"));
    assert!(result.contains("```diff"));
}

#[test]
fn prompt_security_review_with_diff_contains_owasp_terms() {
    let guidelines = ReviewGuidelines::default();
    let result =
        prompt_security_focused_review_with_diff(ContextLevel::Minimal, &guidelines, SAMPLE_DIFF);
    assert!(result.contains("OWASP TOP 10"));
    assert!(result.contains("Injection"));
    assert!(result.contains("Review ONLY the changes in the DIFF"));
    assert!(result.contains("```diff"));
}

#[test]
fn prompt_incremental_review_with_diff_provides_diff_inline() {
    use super::unguided::prompt_incremental_review_with_diff;

    let result = prompt_incremental_review_with_diff(ContextLevel::Minimal, SAMPLE_DIFF);

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
fn all_reviewer_prompts_with_diff_isolate_agents_from_git() {
    // Verify none of the active reviewer prompts tell agents to run git commands
    let prompts = vec![
        prompt_detailed_review_without_guidelines_with_diff(ContextLevel::Minimal, SAMPLE_DIFF),
        prompt_detailed_review_without_guidelines_with_diff(ContextLevel::Normal, SAMPLE_DIFF),
        prompt_universal_review_with_diff(ContextLevel::Minimal, SAMPLE_DIFF),
        prompt_universal_review_with_diff(ContextLevel::Normal, SAMPLE_DIFF),
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

#[test]
fn all_reviewer_prompts_with_diff_include_diff_content() {
    // Verify all diff-based prompts include the diff and tell agent to review only the diff
    let guidelines = ReviewGuidelines::default();
    let prompts = vec![
        prompt_detailed_review_without_guidelines_with_diff(ContextLevel::Minimal, SAMPLE_DIFF),
        prompt_reviewer_review_with_guidelines_and_diff(
            ContextLevel::Minimal,
            &guidelines,
            SAMPLE_DIFF,
        ),
        prompt_comprehensive_review_with_diff(ContextLevel::Minimal, &guidelines, SAMPLE_DIFF),
        prompt_security_focused_review_with_diff(ContextLevel::Minimal, &guidelines, SAMPLE_DIFF),
        prompt_universal_review_with_diff(ContextLevel::Minimal, SAMPLE_DIFF),
    ];

    for prompt in prompts {
        assert!(
            prompt.contains("Review ONLY the changes in the DIFF")
                || prompt.contains("REVIEW ONLY the changes in the DIFF"),
            "Prompt should tell agent to review only the diff: {}",
            &prompt[..200.min(prompt.len())]
        );
        assert!(
            prompt.contains("```diff"),
            "Prompt should include diff in code block: {}",
            &prompt[..200.min(prompt.len())]
        );
        assert!(
            prompt.contains("fn main()"),
            "Prompt should include the actual diff content: {}",
            &prompt[..200.min(prompt.len())]
        );
    }
}
