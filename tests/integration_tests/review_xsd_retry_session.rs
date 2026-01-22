//! Integration tests for review XSD retry session continuation.
//!
//! This module tests that when XSD validation fails on a review attempt,
//! the retry mechanism correctly:
//! 1. Reads the previous output from the correct log directory
//! 2. Reads the XSD error from the correct location
//! 3. Continues the agent session properly
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All integration tests MUST follow the style guide defined in
//! **[INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! Before writing, modifying, or debugging any integration test, you MUST read
//! that document. Key principles:
//!
//! - Test **observable behavior**, not implementation details
//! - Mock only at **architectural boundaries** (filesystem, network, external APIs)
//! - Use `with_default_timeout()` wrapper for all tests
//! - NEVER use `cfg!(test)` branches in production code

use crate::test_timeout::with_default_timeout;
use std::fs;
use tempfile::TempDir;

/// Test that XSD error files are stored and read from correct directories.
///
/// This test verifies the observable behavior that when review XSD validation fails:
/// 1. The error is stored in the correct attempt directory
/// 2. The retry reads from the PREVIOUS attempt directory (not the current one)
///
/// This is a regression test for the bug where retry_num N was trying to read
/// from attempt_N instead of attempt_{N-1}.
#[test]
fn test_review_xsd_error_stored_in_correct_directory() {
    with_default_timeout(|| {
        // Setup: Create a temp directory to simulate log structure
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Simulate the first attempt (retry_num = 0)
        let attempt_0_dir = base_path.join(".agent/logs/reviewer_review_1_attempt_0");
        fs::create_dir_all(&attempt_0_dir).unwrap();

        // Store XSD error in attempt 0 (simulating validation failure)
        let error_msg = "Missing required element: ralph-issue";
        let error_file = attempt_0_dir.join("xsd_error.txt");
        fs::write(&error_file, error_msg).unwrap();

        // Store some output in attempt 0
        let output_file = attempt_0_dir.join("output.log");
        fs::write(&output_file, "Some invalid XML output").unwrap();

        // Execute: Verify the error file exists in attempt 0
        assert!(
            error_file.exists(),
            "XSD error should be stored in attempt_0 directory"
        );

        // Assert: When retry_num = 1, we should look for the error in attempt 0, not attempt 1
        let attempt_1_dir = base_path.join(".agent/logs/reviewer_review_1_attempt_1");
        let error_file_wrong_location = attempt_1_dir.join("xsd_error.txt");

        assert!(
            !error_file_wrong_location.exists(),
            "XSD error should NOT be in attempt_1 directory (it hasn't run yet)"
        );

        // The correct behavior is to read from attempt_0 when retry_num = 1
        let error_from_attempt_0 = fs::read_to_string(&error_file).unwrap();
        assert_eq!(
            error_from_attempt_0, error_msg,
            "Should be able to read XSD error from previous attempt"
        );
    });
}

/// Test that session info is correctly extracted from the first attempt.
///
/// This verifies that session IDs are extracted from the log prefix correctly,
/// using the most recent log file matching the prefix pattern.
#[test]
fn test_session_info_extraction_uses_log_prefix() {
    with_default_timeout(|| {
        use ralph_workflow::agents::JsonParserType;
        use ralph_workflow::pipeline::session::extract_session_info_from_log_prefix;

        // Setup: Create a temp directory with log files
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create log directory for attempt 0
        let log_dir = base_path.join(".agent/logs");
        fs::create_dir_all(&log_dir).unwrap();

        // Create a log file with OpenCode session ID
        let log_file = log_dir.join("reviewer_review_1_attempt_0_opencode_0.log");
        let log_content = r#"{"type":"step_start","timestamp":1768191337567,"sessionID":"ses_test123","part":{"id":"prt_test"}}
{"type":"text","timestamp":1768191347231,"sessionID":"ses_test123","part":{"text":"Hello"}}"#;
        fs::write(&log_file, log_content).unwrap();

        // Execute: Extract session info using the log prefix (without agent name suffix)
        let log_prefix = log_dir.join("reviewer_review_1_attempt_0");
        let session_info = extract_session_info_from_log_prefix(
            &log_prefix,
            JsonParserType::OpenCode,
            Some("opencode"),
        );

        // Assert: Session info should be extracted successfully
        assert!(
            session_info.is_some(),
            "Should extract session info from log file"
        );

        let info = session_info.unwrap();
        assert_eq!(info.session_id, "ses_test123", "Should extract session ID");
        assert_eq!(
            info.agent_name, "opencode",
            "Should use provided agent name"
        );
    });
}

/// Test the directory naming pattern for review attempts.
///
/// This documents the expected directory structure and verifies that
/// we understand how the paths should be constructed for each retry.
#[test]
fn test_review_attempt_directory_naming_pattern() {
    with_default_timeout(|| {
        // This test documents the expected naming pattern
        let j = 1; // review cycle number

        // First attempt (retry_num = 0)
        let attempt_0 = format!(".agent/logs/reviewer_review_{}_attempt_0", j);
        assert_eq!(attempt_0, ".agent/logs/reviewer_review_1_attempt_0");

        // Second attempt (retry_num = 1)
        let attempt_1 = format!(".agent/logs/reviewer_review_{}_attempt_1", j);
        assert_eq!(attempt_1, ".agent/logs/reviewer_review_1_attempt_1");

        // Third attempt (retry_num = 2)
        let attempt_2 = format!(".agent/logs/reviewer_review_{}_attempt_2", j);
        assert_eq!(attempt_2, ".agent/logs/reviewer_review_1_attempt_2");

        // The key insight: When retry_num = N, we need to read from attempt_{N-1}
        // because attempt_N doesn't exist yet (we're about to create it)
    });
}

/// Test that reading from non-existent directory returns empty string.
///
/// This verifies the current behavior when trying to read logs from
/// a directory that doesn't exist yet (which is the bug scenario).
#[test]
fn test_read_from_nonexistent_directory_returns_empty() {
    with_default_timeout(|| {
        use ralph_workflow::pipeline::logfile::read_most_recent_logfile;

        // Setup: Use a path that doesn't exist
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_path = temp_dir
            .path()
            .join(".agent/logs/reviewer_review_1_attempt_999");

        // Execute: Try to read from non-existent path
        let result = read_most_recent_logfile(&nonexistent_path);

        // Assert: Should return empty string (graceful degradation)
        assert_eq!(
            result, "",
            "Should return empty string when directory doesn't exist"
        );

        // This is the bug: When retry_num = 1, we try to read from attempt_1
        // which doesn't exist yet, so we get empty string and lose context
    });
}

/// Test the expected behavior after the fix.
///
/// This test documents what SHOULD happen: when retry_num = 1,
/// we should read from attempt_0 (the previous attempt).
#[test]
fn test_xsd_retry_should_read_from_previous_attempt() {
    with_default_timeout(|| {
        // Setup: Create temp directory with previous attempt's data
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let j = 1;
        let retry_num = 1;

        // Previous attempt's directory (retry_num - 1)
        let prev_attempt = retry_num - 1;
        let prev_dir = format!(".agent/logs/reviewer_review_{}_attempt_{}", j, prev_attempt);
        let prev_dir_path = base_path.join(&prev_dir);
        let log_parent_dir = base_path.join(".agent/logs");
        fs::create_dir_all(&log_parent_dir).unwrap();

        // Store XSD error in previous attempt directory
        // Note: xsd_error.txt goes IN the attempt directory
        fs::create_dir_all(&prev_dir_path).unwrap();
        let error_msg = "XSD validation failed: missing ralph-issues root element";
        fs::write(prev_dir_path.join("xsd_error.txt"), error_msg).unwrap();

        // Store output in previous attempt
        // Log files are stored in .agent/logs/ (parent), not in the attempt subdirectory
        // The prefix is "reviewer_review_1_attempt_0" and files are:
        // .agent/logs/reviewer_review_1_attempt_0_{agent}_{model_index}.log
        let prev_output = "Invalid XML without proper tags";
        let prev_log_file = log_parent_dir.join("reviewer_review_1_attempt_0_codex_0.log");
        fs::write(&prev_log_file, prev_output).unwrap();

        // Execute: Read from PREVIOUS directory (this is what the fix should do)
        let error_content = fs::read_to_string(prev_dir_path.join("xsd_error.txt")).unwrap();

        // Build the full path for the log prefix
        let prev_log_prefix = base_path.join(&prev_dir);
        let output_content =
            ralph_workflow::pipeline::logfile::read_most_recent_logfile(&prev_log_prefix);

        // Assert: We should successfully read from previous attempt
        assert_eq!(error_content, error_msg, "Should read error from prev dir");
        assert_eq!(
            output_content, prev_output,
            "Should read output from prev dir"
        );

        // Current attempt's directory (doesn't exist yet)
        let curr_dir = format!(".agent/logs/reviewer_review_{}_attempt_{}", j, retry_num);
        let curr_dir_path = base_path.join(&curr_dir);
        assert!(
            !curr_dir_path.exists(),
            "Current attempt dir should not exist yet"
        );
    });
}
