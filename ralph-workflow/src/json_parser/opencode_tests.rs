//! Tests for `OpenCode` JSON parser.

use super::*;
use crate::config::Verbosity;
use crate::logger::Colors;
use std::io::Cursor;

#[test]
fn test_parse_opencode_tool_output_object_payload() {
    let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Verbose);
    let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/test.rs"},"output":{"ok":true,"bytes":123}}}}"#;
    let output = parser.parse_event(json).unwrap();
    assert!(output.contains("Output"));
    assert!(output.contains("ok"));
}

#[test]
#[cfg(feature = "test-utils")]
fn test_opencode_streaming_with_tool_use_events() {
    use crate::json_parser::printer::{SharedPrinter, TestPrinter};
    use std::cell::RefCell;
    use std::rc::Rc;

    // Create a TestPrinter to capture output
    let test_printer: SharedPrinter = Rc::new(RefCell::new(TestPrinter::new()));
    let parser =
        OpenCodeParser::with_printer(Colors { enabled: false }, Verbosity::Normal, test_printer);

    // Simulate streaming tool_use events
    let input = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"read","state":{"status":"started","input":{"filePath":"/test.rs"}}}}
{"type":"tool_use","timestamp":1768191346713,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool","read","state":{"status":"completed","input":{"filePath":"/test.rs"}}}}"#;

    let reader = Cursor::new(input);

    // Verify the parse succeeds
    let result = parser.parse_stream(reader);
    assert!(
        result.is_ok(),
        "parse_stream should succeed for OpenCode events"
    );
}

/// Test that `with_terminal_mode` method works correctly
#[test]
#[cfg(feature = "test-utils")]
fn test_with_terminal_mode() {
    use crate::json_parser::terminal::TerminalMode;

    let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);

    // Verify the parser was created successfully
    let json = r#"{"type":"text","timestamp":1768191347231,"sessionID":"test","part":{"id":"prt_001","type":"text","text":"Hello"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
}
