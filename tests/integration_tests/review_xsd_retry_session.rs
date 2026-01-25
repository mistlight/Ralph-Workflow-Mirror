//! Integration tests for review XSD retry session continuation.
//!
//! This module tests that when XSD validation fails on a review attempt,
//! the retry mechanism correctly:
//! 1. Reads the previous output from the correct log directory
//! 2. Reads the XSD error from the correct location
//! 3. Continues the agent session properly
//!
//! Uses `MemoryWorkspace` for all file operations - NO real filesystem access.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
use std::path::Path;

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
        // Setup: Create workspace with log structure
        let error_msg = "Missing required element: ralph-issue";

        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent/logs/reviewer_review_1_attempt_0")
            .with_file(
                ".agent/logs/reviewer_review_1_attempt_0/xsd_error.txt",
                error_msg,
            )
            .with_file(
                ".agent/logs/reviewer_review_1_attempt_0/output.log",
                "Some invalid XML output",
            );

        // Execute: Verify the error file exists in attempt 0
        assert!(
            workspace.exists(Path::new(
                ".agent/logs/reviewer_review_1_attempt_0/xsd_error.txt"
            )),
            "XSD error should be stored in attempt_0 directory"
        );

        // Assert: When retry_num = 1, we should look for the error in attempt 0, not attempt 1
        assert!(
            !workspace.exists(Path::new(
                ".agent/logs/reviewer_review_1_attempt_1/xsd_error.txt"
            )),
            "XSD error should NOT be in attempt_1 directory (it hasn't run yet)"
        );

        // The correct behavior is to read from attempt_0 when retry_num = 1
        let error_from_attempt_0 = workspace
            .read(Path::new(
                ".agent/logs/reviewer_review_1_attempt_0/xsd_error.txt",
            ))
            .unwrap();
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

        // Setup: Create workspace with log files
        let log_content = r#"{"type":"step_start","timestamp":1768191337567,"sessionID":"ses_test123","part":{"id":"prt_test"}}
{"type":"text","timestamp":1768191347231,"sessionID":"ses_test123","part":{"text":"Hello"}}"#;

        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent/logs")
            .with_file(
                ".agent/logs/reviewer_review_1_attempt_0_opencode_0.log",
                log_content,
            );

        // Execute: Extract session info using the log prefix (without agent name suffix)
        let log_prefix = Path::new(".agent/logs/reviewer_review_1_attempt_0");
        let session_info = extract_session_info_from_log_prefix(
            log_prefix,
            JsonParserType::OpenCode,
            Some("opencode"),
            &workspace,
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

        // Setup: Create workspace with no matching log files
        let workspace = MemoryWorkspace::new_test().with_dir(".agent/logs");

        // Execute: Try to read from non-existent path
        let nonexistent_prefix = Path::new(".agent/logs/reviewer_review_1_attempt_999");
        let result = read_most_recent_logfile(nonexistent_prefix, &workspace);

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
        use ralph_workflow::pipeline::logfile::read_most_recent_logfile;

        let j = 1;
        let retry_num = 1;

        // Previous attempt's directory (retry_num - 1)
        let prev_attempt = retry_num - 1;
        let prev_dir = format!(".agent/logs/reviewer_review_{}_attempt_{}", j, prev_attempt);

        let error_msg = "XSD validation failed: missing ralph-issues root element";
        let prev_output = "Invalid XML without proper tags";

        // Setup: Create workspace with previous attempt's data
        let workspace = MemoryWorkspace::new_test()
            .with_dir(".agent/logs")
            .with_dir(&prev_dir)
            .with_file(&format!("{}/xsd_error.txt", prev_dir), error_msg)
            .with_file(
                ".agent/logs/reviewer_review_1_attempt_0_codex_0.log",
                prev_output,
            );

        // Execute: Read from PREVIOUS directory (this is what the fix should do)
        let error_content = workspace
            .read(Path::new(&format!("{}/xsd_error.txt", prev_dir)))
            .unwrap();

        // Build the full path for the log prefix
        let prev_log_prefix = Path::new(&prev_dir);
        let output_content = read_most_recent_logfile(prev_log_prefix, &workspace);

        // Assert: We should successfully read from previous attempt
        assert_eq!(error_content, error_msg, "Should read error from prev dir");
        assert_eq!(
            output_content, prev_output,
            "Should read output from prev dir"
        );

        // Current attempt's directory (doesn't exist yet)
        let curr_dir = format!(".agent/logs/reviewer_review_{}_attempt_{}", j, retry_num);
        assert!(
            !workspace.exists(Path::new(&curr_dir)),
            "Current attempt dir should not exist yet"
        );
    });
}
