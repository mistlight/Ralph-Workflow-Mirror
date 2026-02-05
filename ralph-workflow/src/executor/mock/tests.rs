use super::*;
use crate::executor::ProcessExecutor;
use std::io;

#[test]
fn test_mock_executor_captures_calls() {
    let mock = MockProcessExecutor::new();
    let _ = mock.execute("echo", &["hello"], &[], None);

    assert_eq!(mock.execute_count(), 1);
    let calls = mock.execute_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "echo");
    assert_eq!(calls[0].1, vec!["hello"]);
}

#[test]
fn test_mock_executor_returns_output() {
    let mock = MockProcessExecutor::new().with_output("git", "git version 2.40.0");

    let result = mock.execute("git", &["--version"], &[], None).unwrap();
    assert_eq!(result.stdout, "git version 2.40.0");
    assert!(result.status.success());
}

#[test]
fn test_mock_executor_returns_error() {
    let mock =
        MockProcessExecutor::new().with_io_error("git", io::ErrorKind::NotFound, "git not found");

    let result = mock.execute("git", &["--version"], &[], None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    assert_eq!(err.to_string(), "git not found");
}

#[test]
fn test_mock_executor_can_be_reset() {
    let mock = MockProcessExecutor::new();
    let _ = mock.execute("echo", &["test"], &[], None);

    assert_eq!(mock.execute_count(), 1);
    mock.reset_calls();
    assert_eq!(mock.execute_count(), 0);
}

#[test]
fn test_mock_agent_output_is_valid_ndjson_for_json_parsers() {
    use crate::agents::JsonParserType;

    fn assert_all_lines_are_json(output: &str) {
        for line in output.lines().map(str::trim).filter(|l| !l.is_empty()) {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|e| panic!("expected valid JSON line, got error {e}: {line}"));
        }
    }

    // Claude: system init + assistant content.
    let claude = super::agent_output::generate_mock_agent_output(JsonParserType::Claude, "test");
    assert_all_lines_are_json(&claude);
    assert!(
        claude.contains(r#""session_id":"#),
        "expected session_id in Claude mock output"
    );

    // Codex: dot-separated event types with item payload.
    let codex = super::agent_output::generate_mock_agent_output(JsonParserType::Codex, "test");
    assert_all_lines_are_json(&codex);
    assert!(
        codex.contains(r#""type":"thread.started""#),
        "expected thread.started event in Codex mock output"
    );

    // Gemini: init event should include session_id.
    let gemini = super::agent_output::generate_mock_agent_output(JsonParserType::Gemini, "test");
    assert_all_lines_are_json(&gemini);
    assert!(
        gemini.contains(r#""type":"init""#),
        "expected init event in Gemini mock output"
    );
    assert!(
        gemini.contains(r#""session_id":"#),
        "expected session_id in Gemini mock output"
    );

    // OpenCode: requires sessionID and a part object with text.
    let opencode =
        super::agent_output::generate_mock_agent_output(JsonParserType::OpenCode, "test");
    assert_all_lines_are_json(&opencode);
    assert!(
        opencode.contains(r#""sessionID":"#),
        "expected sessionID in OpenCode mock output"
    );
    assert!(
        opencode.contains(r#""part":{"#) && opencode.contains(r#""text":"#),
        "expected part.text in OpenCode mock output"
    );
}
