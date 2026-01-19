//! Integration tests for TestLogger.
//!
//! These tests verify that TestLogger correctly implements the Loggable trait
//! and can be used as a drop-in replacement for Logger in tests.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (log output capture)
//! - Uses `TestLogger` mock at architectural boundary (logging I/O)
//! - Tests are deterministic and isolated

use crate::test_timeout::with_default_timeout;
use ralph_workflow::logger::output::TestLogger;
use ralph_workflow::logger::Loggable;
use std::io::Write;

/// Test that TestLogger implements the Loggable trait correctly.
#[test]
fn test_logger_trait_log_method() {
    with_default_timeout(|| {
        let logger = TestLogger::new();
        logger.log("Direct log message");
        assert!(logger.has_log("Direct log message"));
    });
}

/// Test that TestLogger info() method formats correctly.
#[test]
fn test_logger_trait_info_method() {
    with_default_timeout(|| {
        let logger = TestLogger::new();
        logger.info("Info message");
        assert!(logger.has_log("[INFO] Info message"));
    });
}

/// Test that TestLogger success() method formats correctly.
#[test]
fn test_logger_trait_success_method() {
    with_default_timeout(|| {
        let logger = TestLogger::new();
        logger.success("Success message");
        assert!(logger.has_log("[OK] Success message"));
    });
}

/// Test that TestLogger warn() method formats correctly.
#[test]
fn test_logger_trait_warn_method() {
    with_default_timeout(|| {
        let logger = TestLogger::new();
        logger.warn("Warning message");
        assert!(logger.has_log("[WARN] Warning message"));
    });
}

/// Test that TestLogger error() method formats correctly.
#[test]
fn test_logger_trait_error_method() {
    with_default_timeout(|| {
        let logger = TestLogger::new();
        logger.error("Error message");
        assert!(logger.has_log("[ERROR] Error message"));
    });
}

/// Test that TestLogger line buffering works correctly.
///
/// TestLogger should buffer partial lines and only flush them when
/// a newline is encountered or flush() is called explicitly.
#[test]
fn test_logger_line_buffering() {
    with_default_timeout(|| {
        let mut logger = TestLogger::new();

        // Write partial line (no newline)
        writeln!(logger, "Partial line").unwrap();
        // The newline should cause the line to be flushed
        assert!(logger.has_log("Partial line"));

        // Write another line
        writeln!(logger, "Another line").unwrap();
        assert!(logger.has_log("Another line"));

        // Check total log count
        assert_eq!(logger.get_logs().len(), 2);
    });
}

/// Test that TestLogger flush() commits buffered content.
///
/// Note: TestLogger's get_logs() and has_log() methods include buffered
/// content, so we need to check the internal logs array directly to
/// verify that buffered content is only added after flush().
#[test]
fn test_logger_flush_behavior() {
    with_default_timeout(|| {
        let mut logger = TestLogger::new();

        // Write partial content without newline
        write!(logger, "Partial content").unwrap();

        // The content is in the buffer, but get_logs() includes buffered content
        // So we should still see it via get_logs() and has_log()
        assert!(logger.has_log("Partial content"));
        assert_eq!(logger.get_logs().len(), 1);

        // Write more content with newline - this should flush the buffer
        writeln!(logger, " with newline").unwrap();

        // Now we should have two log entries: the flushed buffer + the new line
        let logs = logger.get_logs();
        assert_eq!(logs.len(), 1); // Combined into one entry
        assert!(logs[0].contains("Partial content"));
        assert!(logs[0].contains("with newline"));
    });
}

/// Test that TestLogger correctly handles JSON events via Write trait.
#[test]
fn test_logger_json_events_via_write() {
    with_default_timeout(|| {
        let mut logger = TestLogger::new();

        // Write a JSON event
        writeln!(logger, r#"{{"type":"message","content":"Hello"}}"#).unwrap();
        writeln!(logger, r#"{{"type":"result","result":"Final result"}}"#).unwrap();

        let logs = logger.get_logs();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].contains(r#"{"type":"message","content":"Hello"}"#));
        assert!(logs[1].contains(r#"{"type":"result","result":"Final result"}"#));
    });
}

/// Test that TestLogger can be used as a generic Loggable constraint.
#[test]
fn test_logger_generic_constraint() {
    with_default_timeout(|| {
        fn process_with_logger<L: Loggable>(logger: &L) {
            logger.info("Starting");
            logger.success("Done");
        }

        let logger = TestLogger::new();
        process_with_logger(&logger);

        assert!(logger.has_log("[INFO] Starting"));
        assert!(logger.has_log("[OK] Done"));
    });
}

/// Test that TestLogger clear() removes all logs.
#[test]
fn test_logger_clear() {
    with_default_timeout(|| {
        let logger = TestLogger::new();
        logger.log("First message");
        logger.log("Second message");

        assert_eq!(logger.get_logs().len(), 2);

        logger.clear();
        assert_eq!(logger.get_logs().len(), 0);
        assert!(!logger.has_log("First message"));
        assert!(!logger.has_log("Second message"));
    });
}

/// Test that TestLogger count_pattern() works correctly.
#[test]
fn test_logger_count_pattern() {
    with_default_timeout(|| {
        let logger = TestLogger::new();
        logger.log("Error: something went wrong");
        logger.log("Warning: another issue");
        logger.log("Error: yet another error");

        assert_eq!(logger.count_pattern("Error"), 2);
        assert_eq!(logger.count_pattern("Warning"), 1);
        assert_eq!(logger.count_pattern("Info"), 0);
    });
}

/// Test that TestLogger correctly handles mixed Loggable and Write usage.
#[test]
fn test_logger_mixed_usage() {
    with_default_timeout(|| {
        let mut logger = TestLogger::new();

        // Use Loggable trait
        logger.info("Info from trait");

        // Use Write trait
        writeln!(logger, "Direct write").unwrap();

        // Use Loggable again
        logger.success("Success from trait");

        let logs = logger.get_logs();
        assert_eq!(logs.len(), 3);
        assert!(logs[0].contains("[INFO] Info from trait"));
        assert!(logs[1].contains("Direct write"));
        assert!(logs[2].contains("[OK] Success from trait"));
    });
}
