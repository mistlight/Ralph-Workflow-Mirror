//! Integration tests for Logger JSON event extraction.
//!
//! These tests verify the Logger → file → extraction flow, simulating
//! the bug scenario where the last line might not be extracted.

use ralph_workflow::files::result_extraction::json_extraction::extract_result_from_file;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

/// Test that simulates the bug scenario: writing JSON events via file I/O
/// and then extracting them, verifying that the last line is found even
/// without a trailing newline.
#[test]
fn test_logger_json_event_extraction_last_line_without_newline() {
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
    let result = extract_result_from_file(&log_path).unwrap();
    assert_eq!(result, Some("This is the result content".to_string()));
}

/// Test that extraction works with a trailing newline as well.
#[test]
fn test_logger_json_event_extraction_last_line_with_newline() {
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

    let result = extract_result_from_file(&log_path).unwrap();
    assert_eq!(result, Some("This is the result content".to_string()));
}

/// Test extraction with only a result event (no other events).
#[test]
fn test_logger_json_event_extraction_only_result() {
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

    let result = extract_result_from_file(&log_path).unwrap();
    assert_eq!(result, Some("Only result here".to_string()));
}

/// Test extraction with mixed content (some non-JSON lines).
#[test]
fn test_logger_json_event_extraction_mixed_content() {
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

    let result = extract_result_from_file(&log_path).unwrap();
    assert_eq!(result, Some("Final result".to_string()));
}

/// Test extraction with multiple result events (should pick the best one).
#[test]
fn test_logger_json_event_extraction_multiple_results() {
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

    let result = extract_result_from_file(&log_path).unwrap();
    // Both results should be found, and the scoring function should pick one
    // (the longer one in this case due to content length tiebreaker)
    assert!(result.is_some());
    let result_content = result.unwrap();
    assert!(result_content.contains("result"));
}

/// Test the exact bug scenario: JSON events with the last one being
/// a result event without a trailing newline.
#[test]
fn test_bug_scenario_last_line_result_extraction() {
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
    write!(
        file,
        "{{\"type\":\"result\",\"result\":\"## Summary\n\nAfter thorough investigation, I found that BufReader::lines() correctly reads the last line even without a trailing newline.\"}}"
    ).unwrap();
    drop(file);

    // Verify the result event is extracted correctly
    let result = extract_result_from_file(&log_path).unwrap();

    // The result should be found
    assert!(
        result.is_some(),
        "Expected to find result event but got None. This indicates the last-line extraction bug."
    );

    let result_content = result.unwrap();
    assert!(result_content.contains("BufReader::lines()"));
    assert!(result_content.contains("correctly reads the last line"));
}

/// Test that extraction handles empty files gracefully.
#[test]
fn test_logger_json_event_extraction_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("empty.log");

    // Create an empty file
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(&log_path)
        .unwrap();

    let result = extract_result_from_file(&log_path).unwrap();
    assert_eq!(result, None);
}

/// Test that extraction handles non-existent files gracefully.
#[test]
fn test_logger_json_event_extraction_nonexistent_file() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("nonexistent.log");

    let result = extract_result_from_file(&log_path).unwrap();
    assert_eq!(result, None);
}
