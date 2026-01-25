//! Integration tests for JSON event extraction from log files.
//!
//! These tests verify `extract_last_result` correctly extracts result events
//! from log files with various edge cases (missing newlines, mixed content, etc.).
//!
//! Uses `MemoryWorkspace` for all file operations - NO real filesystem access.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::files::result_extraction::extract_last_result;
use ralph_workflow::logger::Loggable;
use ralph_workflow::workspace::MemoryWorkspace;
use std::path::Path;

/// Test that JSON event extraction works without trailing newline.
#[test]
fn test_json_extraction_last_line_without_newline() {
    with_default_timeout(|| {
        let log_content = r#"{"type":"message","content":"first message"}
{"type":"message","content":"second message"}
{"type":"result","result":"This is the result content"}"#;

        let workspace = MemoryWorkspace::new_test().with_file("/test/logs/test.log", log_content);
        let log_path = Path::new("/test/logs/test.log");

        let result = extract_last_result(&workspace, log_path).unwrap();
        assert_eq!(result, Some("This is the result content".to_string()));
    });
}

/// Test that extraction works with a trailing newline as well.
#[test]
fn test_json_extraction_last_line_with_newline() {
    with_default_timeout(|| {
        let log_content = r#"{"type":"message","content":"first message"}
{"type":"message","content":"second message"}
{"type":"result","result":"This is the result content"}
"#;

        let workspace = MemoryWorkspace::new_test().with_file("/test/logs/test.log", log_content);
        let log_path = Path::new("/test/logs/test.log");

        let result = extract_last_result(&workspace, log_path).unwrap();
        assert_eq!(result, Some("This is the result content".to_string()));
    });
}

/// Test extraction with only a result event (no other events).
#[test]
fn test_json_extraction_only_result() {
    with_default_timeout(|| {
        let log_content = r#"{"type":"result","result":"Only result here"}"#;

        let workspace = MemoryWorkspace::new_test().with_file("/test/logs/test.log", log_content);
        let log_path = Path::new("/test/logs/test.log");

        let result = extract_last_result(&workspace, log_path).unwrap();
        assert_eq!(result, Some("Only result here".to_string()));
    });
}

/// Test extraction with mixed content (some non-JSON lines).
#[test]
fn test_json_extraction_mixed_content() {
    with_default_timeout(|| {
        let log_content = r#"[INFO] Starting process
{"type":"message","content":"working"}
[INFO] Almost done
{"type":"result","result":"Final result"}"#;

        let workspace = MemoryWorkspace::new_test().with_file("/test/logs/test.log", log_content);
        let log_path = Path::new("/test/logs/test.log");

        let result = extract_last_result(&workspace, log_path).unwrap();
        assert_eq!(result, Some("Final result".to_string()));
    });
}

/// Test extraction with multiple result events (should pick the best one).
#[test]
fn test_json_extraction_multiple_results() {
    with_default_timeout(|| {
        let log_content = r#"{"type":"result","result":"First result with content"}
{"type":"result","result":"Second result with more content that is longer"}"#;

        let workspace = MemoryWorkspace::new_test().with_file("/test/logs/test.log", log_content);
        let log_path = Path::new("/test/logs/test.log");

        let result = extract_last_result(&workspace, log_path).unwrap();
        assert!(result.is_some());
        let result_content = result.unwrap();
        assert!(result_content.contains("result"));
    });
}

/// Test the exact bug scenario: last line result event without trailing newline.
#[test]
fn test_bug_scenario_last_line_result_extraction() {
    with_default_timeout(|| {
        let log_content = r###"{"type":"start","timestamp":"2026-01-17T20:41:39"}
{"type":"message","content":"I'll craft a prioritized checklist"}
{"type":"message","content":"Let me go ahead and create that response"}
{"type":"progress","content":"Turn completed"}
{"type":"info","message":"Phase elapsed: 0m 58s"}
{"type":"result","result":"## Summary\n\nAfter thorough investigation, I found that BufReader::lines() correctly reads the last line even without a trailing newline."}"###;

        let workspace = MemoryWorkspace::new_test().with_file("/test/logs/agent.log", log_content);
        let log_path = Path::new("/test/logs/agent.log");

        let result = extract_last_result(&workspace, log_path).unwrap();

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
#[test]
fn test_json_extraction_empty_file() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test().with_file("/test/logs/empty.log", "");
        let log_path = Path::new("/test/logs/empty.log");

        let result = extract_last_result(&workspace, log_path).unwrap();
        assert_eq!(result, None);
    });
}

/// Test that extraction handles non-existent files gracefully.
#[test]
fn test_json_extraction_nonexistent_file() {
    with_default_timeout(|| {
        let workspace = MemoryWorkspace::new_test();
        let log_path = Path::new("/test/logs/nonexistent.log");

        let result = extract_last_result(&workspace, log_path).unwrap();
        assert_eq!(result, None);
    });
}

/// Test the user-reported bug scenario.
#[test]
fn test_user_reported_bug_scenario() {
    with_default_timeout(|| {
        let log_content = r#"{"type":"message","content":"I'll craft a prioritized checklist"}
{"type":"message","content":"Let me go ahead and create that response"}
{"type":"result","result":"- [ ] Critical: args.rs has duplicate `pub init` declarations\n- [ ] High: Template commands have conflicting attributes\n- [ ] Medium: Integration test mismatches"}"#;

        let workspace =
            MemoryWorkspace::new_test().with_file("/test/logs/agent_output.log", log_content);
        let log_path = Path::new("/test/logs/agent_output.log");

        let result = extract_last_result(&workspace, log_path).unwrap();

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

        let test_logger = TestLogger::new();
        process_logs(&test_logger);

        assert!(test_logger.has_log("[INFO] Starting process"));
        assert!(test_logger.has_log("[OK] Process completed"));
        assert!(test_logger.has_log("[WARN] Potential issue"));
        assert!(test_logger.has_log("[ERROR] Critical error"));
    });
}

/// Test TestLogger captures all messages via Loggable trait.
#[test]
fn test_loggable_trait_with_testlogger() {
    with_default_timeout(|| {
        use ralph_workflow::logger::output::TestLogger;

        let logger = TestLogger::new();

        logger.log("Direct log message");
        logger.info("Info message");
        logger.success("Success message");
        logger.warn("Warning message");
        logger.error("Error message");

        assert_eq!(logger.get_logs().len(), 5);
        assert!(logger.has_log("Direct log message"));
        assert!(logger.has_log("[INFO] Info message"));
        assert!(logger.has_log("[OK] Success message"));
        assert!(logger.has_log("[WARN] Warning message"));
        assert!(logger.has_log("[ERROR] Error message"));
    });
}

/// Test that Loggable trait default implementations work correctly.
#[test]
fn test_loggable_trait_default_implementations() {
    with_default_timeout(|| {
        use ralph_workflow::logger::output::TestLogger;

        struct CustomLogger {
            inner: TestLogger,
        }

        impl Loggable for CustomLogger {
            fn log(&self, msg: &str) {
                self.inner.log(msg);
            }
        }

        let logger = CustomLogger {
            inner: TestLogger::new(),
        };

        logger.info("Info");
        logger.success("Success");
        logger.warn("Warning");
        logger.error("Error");

        assert!(logger.inner.has_log("[INFO] Info"));
        assert!(logger.inner.has_log("[OK] Success"));
        assert!(logger.inner.has_log("[WARN] Warning"));
        assert!(logger.inner.has_log("[ERROR] Error"));
    });
}
