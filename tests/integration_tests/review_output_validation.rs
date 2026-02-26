//! Integration tests for review agent output validation.
//!
//! This module tests that the output validator correctly identifies valid output
//! when the agent writes XML to .agent/tmp/issues.xml (file-based mode).
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All integration tests MUST follow the style guide defined in
//! **[`INTEGRATION_TESTS.md`](../INTEGRATION_TESTS.md)**.
//!
//! Before writing, modifying, or debugging any integration test, you MUST read
//! that document. Key principles:
//!
//! - Test **observable behavior**, not implementation details
//! - Mock only at **architectural boundaries** (filesystem, network, external APIs)
//! - Use `with_default_timeout()` wrapper for all tests
//! - NEVER use `cfg!(test)` branches in production code

use crate::test_timeout::with_default_timeout;
use ralph_workflow::files::llm_output_extraction::file_based_extraction::{
    has_valid_xml_output, paths,
};
use ralph_workflow::workspace::MemoryWorkspace;
use std::path::Path;

/// Test that `has_valid_xml_output` returns true when issues.xml exists with valid XML.
///
/// This is the core bug fix test: when an agent writes XML to the designated
/// file path (.agent/tmp/issues.xml), the validator should detect it as valid
/// output even if no JSON result events are in the logs.
#[test]
fn test_has_valid_xml_output_detects_file_based_xml() {
    with_default_timeout(|| {
        // Setup: Create workspace with valid XML in the expected location
        let workspace = MemoryWorkspace::new_test().with_file(
            paths::ISSUES_XML,
            r"<ralph-issues>
<ralph-no-issues-found>No issues found during review.</ralph-no-issues-found>
</ralph-issues>",
        );

        // Execute: Check if the file-based validator detects the XML
        let result = has_valid_xml_output(&workspace, Path::new(paths::ISSUES_XML));

        // Assert: File should exist and be detected as valid XML
        assert!(result, "Valid XML file should be detected");
    });
}

/// Test that `has_valid_xml_output` returns true for XML with issues.
///
/// This verifies that the validator correctly identifies XML containing
/// actual issues (not just no-issues-found).
#[test]
fn test_has_valid_xml_output_detects_issues_xml() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_file(
            paths::ISSUES_XML,
            r"<ralph-issues>
<ralph-issue>Variable unused in src/main.rs</ralph-issue>
<ralph-issue>Missing error handling in src/utils.rs</ralph-issue>
</ralph-issues>",
        );

        let result = has_valid_xml_output(&workspace, Path::new(paths::ISSUES_XML));

        assert!(result, "XML with issues should be detected as valid");
    });
}

/// Test that empty XML file is not considered valid output.
///
/// This verifies that the validator correctly rejects empty files,
/// which indicate the agent didn't produce any output.
#[test]
fn test_has_valid_xml_output_rejects_empty_file() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_file(paths::ISSUES_XML, "");

        let result = has_valid_xml_output(&workspace, Path::new(paths::ISSUES_XML));

        // Empty content should not be considered valid
        assert!(!result, "Empty file should not be considered valid XML");
    });
}

/// Test that whitespace-only XML file is not considered valid output.
///
/// This verifies that the validator correctly rejects files containing
/// only whitespace, which indicate the agent didn't produce meaningful output.
#[test]
fn test_has_valid_xml_output_rejects_whitespace_only() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_file(paths::ISSUES_XML, "   \n\n  \t  ");

        let result = has_valid_xml_output(&workspace, Path::new(paths::ISSUES_XML));

        assert!(
            !result,
            "Whitespace-only file should not be considered valid XML"
        );
    });
}

/// Test that non-XML file is not considered valid output.
///
/// This verifies that the validator correctly rejects files that don't
/// contain XML content (don't start with '<').
#[test]
fn test_has_valid_xml_output_rejects_non_xml() {
    with_default_timeout(|| {
        let workspace =
            MemoryWorkspace::new_test().with_file(paths::ISSUES_XML, "This is plain text, not XML");

        let result = has_valid_xml_output(&workspace, Path::new(paths::ISSUES_XML));

        // Non-XML content should not be considered valid
        assert!(!result, "Plain text should not be considered valid XML");
    });
}

/// Test that missing file is not considered valid output.
///
/// This verifies that the validator correctly returns false when
/// the expected XML file doesn't exist.
#[test]
fn test_has_valid_xml_output_returns_false_for_missing_file() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();

        let result = has_valid_xml_output(&workspace, Path::new(paths::ISSUES_XML));

        assert!(!result, "Missing file should return false");
    });
}

/// Test that `fix_result.xml` is also detected.
///
/// This verifies that the validator works for both issues.xml and `fix_result.xml`,
/// since both use the same file-based output mechanism.
#[test]
fn test_has_valid_xml_output_detects_fix_result_xml() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_file(
            paths::FIX_RESULT_XML,
            r"<ralph-fix-result>
<ralph-status>all_issues_addressed</ralph-status>
<ralph-summary>Fixed all issues.</ralph-summary>
</ralph-fix-result>",
        );

        let result = has_valid_xml_output(&workspace, Path::new(paths::FIX_RESULT_XML));

        assert!(result, "Valid fix result XML should be detected");
    });
}

/// Test that XML with leading whitespace is still detected.
///
/// This verifies that the validator correctly handles XML files that
/// have leading whitespace before the opening tag.
#[test]
fn test_has_valid_xml_output_handles_leading_whitespace() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_file(
            paths::ISSUES_XML,
            r"  
   <ralph-issues>
<ralph-no-issues-found>No issues</ralph-no-issues-found>
</ralph-issues>",
        );

        let result = has_valid_xml_output(&workspace, Path::new(paths::ISSUES_XML));

        assert!(
            result,
            "XML with leading whitespace should still be detected"
        );
    });
}

/// Test that file-based XML extraction works even without JSON logs.
///
/// This is a regression test for the bug where `extract_and_validate_review_output_xml`
/// would return "No review output captured" when:
/// 1. No JSON result events in logs (e.g., opencode parser)
/// 2. No ISSUES.md file exists
/// 3. But valid XML exists in .agent/tmp/issues.xml
///
/// The fix ensures .agent/tmp/issues.xml is checked FIRST, before JSON/ISSUES.md.
#[test]
fn test_file_based_xml_extraction_without_json_logs() {
    with_default_timeout(|| {
        use ralph_workflow::files::llm_output_extraction::{
            has_valid_xml_output, try_extract_from_file_with_workspace,
        };

        // Setup: Valid XML in issues.xml, NO JSON logs, NO ISSUES.md
        let valid_xml = r"<ralph-issues>
<ralph-no-issues-found>All code conforms to the architecture requirements.</ralph-no-issues-found>
</ralph-issues>";

        let workspace = MemoryWorkspace::new_test()
            .with_file(paths::ISSUES_XML, valid_xml)
            // Explicitly NO .agent/logs/ files
            // Explicitly NO .agent/ISSUES.md
            .with_dir(".agent/logs");

        // Assert: XML file should be detected as valid
        assert!(
            has_valid_xml_output(&workspace, Path::new(paths::ISSUES_XML)),
            "Should detect valid XML in issues.xml"
        );

        // Assert: Direct extraction should work
        let extracted =
            try_extract_from_file_with_workspace(&workspace, Path::new(paths::ISSUES_XML));
        assert!(
            extracted.is_some(),
            "Should extract XML from issues.xml without JSON logs"
        );
        assert!(
            extracted.unwrap().contains("<ralph-no-issues-found>"),
            "Extracted XML should contain the no-issues-found element"
        );
    });
}
