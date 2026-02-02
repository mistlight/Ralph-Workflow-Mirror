// Cross-parser behavior tests.
//
// Tests for verbosity settings, display names, and tool use behavior
// across different parser types.

#[test]
fn test_verbosity_affects_output() {
    let quiet_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Quiet);
    let full_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Full);

    let long_text = "a".repeat(200);
    let json = format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{long_text}"}}]}}}}"#
    );

    let quiet_output = quiet_parser.parse_event(&json).unwrap();
    let full_output = full_parser.parse_event(&json).unwrap();

    // Quiet output should be truncated (shorter)
    assert!(quiet_output.len() < full_output.len());
}

#[test]
fn test_tool_use_shows_input_in_verbose_mode() {
    let verbose_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose)
        .with_terminal_mode(TerminalMode::Full);
    let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
    let output = verbose_parser.parse_event(json).unwrap();
    assert!(output.contains("Tool"));
    assert!(output.contains("Read"));
    assert!(output.contains("file_path=/test.rs"));
}

#[test]
fn test_tool_use_shows_input_in_normal_mode() {
    // Tool inputs are now shown at Normal level for better usability
    let normal_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);
    let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
    let output = normal_parser.parse_event(json).unwrap();
    assert!(output.contains("Tool"));
    assert!(output.contains("Read"));
    // Tool inputs are now visible at Normal level
    assert!(output.contains("file_path=/test.rs"));
}

#[test]
fn test_tool_use_hides_input_in_quiet_mode() {
    // Only Quiet mode hides tool inputs
    let quiet_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Quiet);
    let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
    let output = quiet_parser.parse_event(json).unwrap();
    assert!(output.contains("Tool"));
    assert!(output.contains("Read"));
    // In Quiet mode, input details should not be shown
    assert!(!output.contains("file_path=/test.rs"));
}

#[test]
fn test_parser_uses_custom_display_name_prefix() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full)
        .with_display_name("ccs-glm");
    let json = r#"{"type":"system","subtype":"init","session_id":"abc123"}"#;
    let output = parser.parse_event(json).unwrap();
    assert!(output.contains("[ccs-glm]"));
}

#[test]
fn test_debug_verbosity_is_recognized() {
    let debug_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Debug)
        .with_terminal_mode(TerminalMode::Full);
    // Debug mode should be detectable via is_debug()
    assert!(debug_parser.verbosity.is_debug());
}
