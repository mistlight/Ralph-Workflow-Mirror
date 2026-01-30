//! Tests documenting intentional handler behaviors that are NOT reducer-driven.
//!
//! These tests act as architectural documentation for behaviors that appear
//! "hidden" but are explicitly allowed because they are idempotent preparation
//! or validation helpers with no reducer control-flow impact.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use crate::test_timeout::with_default_timeout;

/// Test that handler cleanup operations are intentional and non-policy decisions.
#[test]
fn test_handler_cleanup_is_documented_intentional() {
    with_default_timeout(|| {
        // This test documents that handler-level cleanup is intentional:
        // - clear_stale_development_result_xml
        // - clear_stale_review_issues_xml
        // - cleanup_continuation_context_file
        //
        // These are idempotent preparation steps that do not affect reducer policy.
    });
}

/// Test that XSD retry loop is an intentional handler optimization.
#[test]
fn test_xsd_retry_loop_is_documented_intentional() {
    with_default_timeout(|| {
        // The XSD retry loop in run_xsd_retry_with_session is intentional because:
        // - Session continuation requires in-process state
        // - Each retry shares session context with the agent
        // - The loop is bounded and surfaces final results via reducer events
    });
}

/// Test that marker file checks are validation helpers, not control flow.
#[test]
fn test_marker_file_check_is_documented_intentional() {
    with_default_timeout(|| {
        // Marker file checks (e.g., ISSUES.md validation markers) are
        // validation helpers. Reducer decides retry vs success.
    });
}
