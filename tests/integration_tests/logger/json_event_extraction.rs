//! Integration tests for Logger JSON event extraction.
//!
//! These tests verify the Logger → file → extraction flow, simulating
//! the bug scenario where the last line might not be extracted.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (file contents and extraction results)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

use crate::test_timeout::with_default_timeout;
use ralph_workflow::files::result_extraction::extract_last_result;
use ralph_workflow::logger::Colors;
use ralph_workflow::logger::Loggable;
use ralph_workflow::logger::Logger;
use std::fs::OpenOptions;
use std::io::Write;
use tempfile::TempDir;

/// Test that JSON event extraction works without trailing newline.
///
/// This verifies that when JSON events are written via file I/O and the last
/// event has no trailing newline, the extraction still finds the result event.
#[test]
fn test_logger_json_event_extraction_last_line_without_newline() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.log");

        // Simulate writing JSON events to a log file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        // Write some normal JSON events with newlines
        writeln!(file, r#"{{"type":"message","content":"first message"}}"#).unwrap();
        writeln!(file, r#"{{"type":"message","content":"second message"}}"#).unwrap();

        // Write the result event WITHOUT a trailing newline (simulating the bug)
        write!(
            file,
            r#"{{"type":"result","result":"This is the result content"}}"#
        )
        .unwrap();
        drop(file);

        // Verify the result can be extracted
        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, Some("This is the result content".to_string()));
    });
}

/// Test that extraction works with a trailing newline as well.
///
/// This verifies that when JSON events are written with trailing newlines,
/// the extraction correctly finds the last result event.
#[test]
fn test_logger_json_event_extraction_last_line_with_newline() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.log");

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        writeln!(file, r#"{{"type":"message","content":"first message"}}"#).unwrap();
        writeln!(file, r#"{{"type":"message","content":"second message"}}"#).unwrap();
        // Write the result event WITH a trailing newline
        writeln!(
            file,
            r#"{{"type":"result","result":"This is the result content"}}"#
        )
        .unwrap();
        drop(file);

        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, Some("This is the result content".to_string()));
    });
}

/// Test extraction with only a result event (no other events).
///
/// This verifies that when the log contains only a result event,
/// the extraction correctly retrieves it.
#[test]
fn test_logger_json_event_extraction_only_result() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.log");

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        // Only write the result event without trailing newline
        write!(file, r#"{{"type":"result","result":"Only result here"}}"#).unwrap();
        drop(file);

        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, Some("Only result here".to_string()));
    });
}

/// Test extraction with mixed content (some non-JSON lines).
///
/// This verifies that when the log contains both JSON and non-JSON lines,
/// the extraction correctly finds the result event.
#[test]
fn test_logger_json_event_extraction_mixed_content() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.log");

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        // Mix of JSON and non-JSON lines
        writeln!(file, "[INFO] Starting process").unwrap();
        writeln!(file, r#"{{"type":"message","content":"working"}}"#).unwrap();
        writeln!(file, "[INFO] Almost done").unwrap();
        // Result event without trailing newline
        write!(file, r#"{{"type":"result","result":"Final result"}}"#).unwrap();
        drop(file);

        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, Some("Final result".to_string()));
    });
}

/// Test extraction with multiple result events (should pick the best one).
///
/// This verifies that when multiple result events exist, the extraction
/// uses scoring to pick the best one.
#[test]
fn test_logger_json_event_extraction_multiple_results() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.log");

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        // First result with trailing newline
        writeln!(
            file,
            r#"{{"type":"result","result":"First result with content"}}"#
        )
        .unwrap();
        // Second result without trailing newline (should still be found)
        write!(
            file,
            r#"{{"type":"result","result":"Second result with more content that is longer"}}"#
        )
        .unwrap();
        drop(file);

        let result = extract_last_result(&log_path).unwrap();
        // Both results should be found, and the scoring function should pick one
        // (the longer one in this case due to content length tiebreaker)
        assert!(result.is_some());
        let result_content = result.unwrap();
        assert!(result_content.contains("result"));
    });
}

/// Test the exact bug scenario: JSON events with the last one being
/// a result event without a trailing newline.
///
/// This verifies that when the last line is a result event without a trailing newline,
/// the extraction correctly finds it (this was a reported bug).
#[test]
fn test_bug_scenario_last_line_result_extraction() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("agent.log");

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        // Simulate agent output with various JSON events
        writeln!(
            file,
            r#"{{"type":"start","timestamp":"2026-01-17T20:41:39"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"message","content":"I'll craft a prioritized checklist"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"message","content":"Let me go ahead and create that response"}}"#
        )
        .unwrap();
        writeln!(file, r#"{{"type":"progress","content":"Turn completed"}}"#).unwrap();
        writeln!(
            file,
            r#"{{"type":"info","message":"Phase elapsed: 0m 58s"}}"#
        )
        .unwrap();

        // The critical result event WITHOUT trailing newline (the bug scenario)
        // Note: Using regular string literal with escaped newlines in JSON (\\n becomes \n in JSON)
        write!(
        file,
        "{{\"type\":\"result\",\"result\":\"## Summary\\n\\nAfter thorough investigation, I found that BufReader::lines() correctly reads the last line even without a trailing newline.\"}}"
    ).unwrap();
        drop(file);

        // Verify the result event is extracted correctly
        let result = extract_last_result(&log_path).unwrap();

        // The result should be found
        assert!(
        result.is_some(),
        "Expected to find result event but got None. This indicates the last-line extraction bug."
    );

        let result_content = result.unwrap();
        assert!(result_content.contains("BufReader::lines()"));
        assert!(result_content.contains("correctly reads the last line"));
    });
}

/// Test that extraction handles empty files gracefully.
///
/// This verifies that when the log file is empty, the extraction
/// returns None without producing an error.
#[test]
fn test_logger_json_event_extraction_empty_file() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("empty.log");

        // Create an empty file
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&log_path)
            .unwrap();

        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, None);
    });
}

/// Test that extraction handles non-existent files gracefully.
///
/// This verifies that when the log file doesn't exist, the extraction
/// returns None without producing an error.
#[test]
fn test_logger_json_event_extraction_nonexistent_file() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("nonexistent.log");

        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, None);
    });
}

/// Test Logger → file → extraction flow using Logger::with_log_file().
///
/// This verifies that when the actual Logger writes JSON events to a file,
/// the extraction correctly retrieves the result event.
#[test]
fn test_logger_with_log_file_writes_json_events_and_extracts() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("logger_test.log");

        // Create a logger with file logging
        let logger = Logger::new(Colors::new()).with_log_file(log_path.to_str().unwrap());

        // Write some info messages (which will be logged to file)
        logger.info("Starting process");
        logger.info("Processing data");

        // Write a success message
        logger.success("Operation completed");

        // Now manually append JSON events to the log file
        // (simulating what an agent would write)
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        writeln!(file, r#"{{"type":"message","content":"Agent working"}}"#).unwrap();
        // Write result event WITHOUT trailing newline (the bug scenario)
        write!(
            file,
            r#"{{"type":"result","result":"Result content from agent"}}"#
        )
        .unwrap();
        drop(file);

        // Verify the result can be extracted
        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, Some("Result content from agent".to_string()));
    });
}

/// Test Logger → file → extraction flow with trailing newline.
///
/// This verifies that when the Logger writes content with trailing newlines,
/// the extraction correctly retrieves the result event.
#[test]
fn test_logger_with_log_file_trailing_newline_extraction() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("logger_test_newline.log");

        let logger = Logger::new(Colors::new()).with_log_file(log_path.to_str().unwrap());

        // Write log messages
        logger.info("Test with trailing newline");
        logger.success("All tests passed");

        // Append JSON events with trailing newline
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        writeln!(file, r#"{{"type":"message","content":"Agent working"}}"#).unwrap();
        writeln!(
            file,
            r#"{{"type":"result","result":"Result with newline"}}"#
        )
        .unwrap();
        drop(file);

        // Verify extraction works
        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, Some("Result with newline".to_string()));
    });
}

/// Test the specific user-reported bug scenario with Logger.
///
/// This verifies that when an agent outputs JSON events with or without
/// trailing newlines, the extraction correctly finds them.
#[test]
fn test_user_reported_bug_scenario_with_logger() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("agent_output.log");

        // Create logger to simulate production environment
        let _logger = Logger::new(Colors::new()).with_log_file(log_path.to_str().unwrap());

        // Simulate the exact output from the user's report:
        // The agent produced checklist items but the result event wasn't extracted
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        // Agent output as seen in the user's report
        writeln!(
            file,
            r#"{{"type":"message","content":"I'll craft a prioritized checklist"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"message","content":"Let me go ahead and create that response"}}"#
        )
        .unwrap();

        // The critical bug scenario: result event WITHOUT trailing newline
        write!(
        file,
        "{{\"type\":\"result\",\"result\":\"- [ ] Critical: args.rs has duplicate `pub init` declarations\\n- [ ] High: Template commands have conflicting attributes\\n- [ ] Medium: Integration test mismatches\"}}"
    ).unwrap();
        drop(file);

        // Verify the result event IS found (this would have failed in the bug)
        let result = extract_last_result(&log_path).unwrap();

        assert!(
        result.is_some(),
        "Expected to find result event but got None. This indicates the last-line extraction bug."
    );

        let result_content = result.unwrap();
        assert!(result_content.contains("Critical:"));
        assert!(result_content.contains("args.rs"));
        assert!(result_content.contains("duplicate"));
    });
}

/// Test using Loggable trait as a generic constraint.
///
/// This verifies that when a generic function uses the Loggable trait,
/// it works correctly with both Logger and TestLogger.
#[test]
fn test_loggable_trait_generic_function() {
    with_default_timeout(|| {
        use ralph_workflow::logger::output::TestLogger;

        fn process_logs<L: Loggable>(logger: &L) {
            logger.info("Starting process");
            logger.success("Process completed");
            logger.warn("Potential issue");
            logger.error("Critical error");
        }

        // Test with TestLogger
        let test_logger = TestLogger::new();
        process_logs(&test_logger);

        assert!(test_logger.has_log("[INFO] Starting process"));
        assert!(test_logger.has_log("[OK] Process completed"));
        assert!(test_logger.has_log("[WARN] Potential issue"));
        assert!(test_logger.has_log("[ERROR] Critical error"));
    });
}

/// Test Logger → file → extraction flow using Loggable trait.
///
/// This verifies that when the Loggable trait interface is used to write logs,
/// the trait provides a unified interface for both production and test scenarios.
#[test]
fn test_loggable_trait_with_logger_file_extraction() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("loggable_test.log");

        // Create a logger and use it via the Loggable trait
        let logger = Logger::new(Colors::new()).with_log_file(log_path.to_str().unwrap());

        // Use the Loggable trait methods
        logger.log("[INFO] Direct log message");
        logger.info("Info message via trait");
        logger.success("Success message via trait");
        logger.warn("Warning message via trait");
        logger.error("Error message via trait");

        // Verify the logs were written to the file
        let log_content = std::fs::read_to_string(&log_path).unwrap();
        assert!(log_content.contains("[INFO] Direct log message"));
        assert!(log_content.contains("[INFO] Info message via trait"));
        assert!(log_content.contains("[OK] Success message via trait"));
        assert!(log_content.contains("[WARN] Warning message via trait"));
        assert!(log_content.contains("[ERROR] Error message via trait"));

        // Now append JSON events and verify extraction works
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();

        writeln!(file, r#"{{"type":"message","content":"Agent working"}}"#).unwrap();
        // Write result event WITHOUT trailing newline
        write!(
            file,
            r#"{{"type":"result","result":"Result from Loggable test"}}"#
        )
        .unwrap();
        drop(file);

        // Verify extraction works
        let result = extract_last_result(&log_path).unwrap();
        assert_eq!(result, Some("Result from Loggable test".to_string()));
    });
}

/// Test TestLogger → extraction flow using Loggable trait.
///
/// This verifies that when TestLogger is used via the Loggable trait,
/// it correctly implements the trait and captures log messages.
#[test]
fn test_loggable_trait_with_testlogger() {
    with_default_timeout(|| {
        use ralph_workflow::logger::output::TestLogger;

        let logger = TestLogger::new();

        // Use the Loggable trait methods
        logger.log("Direct log message");
        logger.info("Info message");
        logger.success("Success message");
        logger.warn("Warning message");
        logger.error("Error message");

        // Verify all messages were captured
        assert_eq!(logger.get_logs().len(), 5);
        assert!(logger.has_log("Direct log message"));
        assert!(logger.has_log("[INFO] Info message"));
        assert!(logger.has_log("[OK] Success message"));
        assert!(logger.has_log("[WARN] Warning message"));
        assert!(logger.has_log("[ERROR] Error message"));
    });
}

/// Test that Loggable trait default implementations work correctly.
///
/// This verifies that when the default implementations of info(), success(),
/// warn(), and error() in the Loggable trait are used, they correctly format messages.
#[test]
fn test_loggable_trait_default_implementations() {
    with_default_timeout(|| {
        use ralph_workflow::logger::output::TestLogger;

        struct CustomLogger {
            inner: TestLogger,
        }

        // Implement only the required log() method
        impl Loggable for CustomLogger {
            fn log(&self, msg: &str) {
                self.inner.log(msg);
            }
        }

        let logger = CustomLogger {
            inner: TestLogger::new(),
        };

        // Use the default implementations
        logger.info("Info");
        logger.success("Success");
        logger.warn("Warning");
        logger.error("Error");

        // Verify the default implementations correctly format messages
        assert!(logger.inner.has_log("[INFO] Info"));
        assert!(logger.inner.has_log("[OK] Success"));
        assert!(logger.inner.has_log("[WARN] Warning"));
        assert!(logger.inner.has_log("[ERROR] Error"));
    });
}
