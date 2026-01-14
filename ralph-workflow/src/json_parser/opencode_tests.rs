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
fn test_opencode_streaming_with_tool_use_events() {
    let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);

    // Simulate streaming tool_use events
    let input = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"read","state":{"status":"started","input":{"filePath":"/test.rs"}}}}
{"type":"tool_use","timestamp":1768191346713,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/test.rs"}}}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    // Verify the parse succeeds
    let result = parser.parse_stream(reader, &mut writer);
    assert!(
        result.is_ok(),
        "parse_stream should succeed for OpenCode events"
    );
}
