use crate::agents::JsonParserType;

/// Generate minimal valid agent output for mock testing.
///
/// This creates a minimal valid NDJSON output that the streaming parser can
/// successfully parse without hanging. The output format depends on the parser
/// type being used.
pub(super) fn generate_mock_agent_output(parser_type: JsonParserType, _command: &str) -> String {
    let commit_message = r#"<ralph-commit>
 <ralph-subject>test: commit message</ralph-subject>
 <ralph-body>Test commit message for integration tests.</ralph-body>
 </ralph-commit>"#;

    match parser_type {
        JsonParserType::Claude => format!(
            r#"{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"ses_mock_session_12345\"}}
{{\"type\":\"result\",\"result\":\"{}\"}}
"#,
            commit_message.replace('\n', "\\n").replace('"', "\\\"")
        ),
        JsonParserType::Codex => format!(
            r#"{{\"type\":\"turn_started\",\"turn_id\":\"test_turn\"}}
{{\"type\":\"item_started\",\"item\":{{\"type\":\"agent_message\",\"text\":\"{}\"}}}}
{{\"type\":\"item_completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"{}\"}}}}
{{\"type\":\"turn_completed\"}}
{{\"type\":\"completion\",\"reason\":\"stop\"}}
"#,
            commit_message, commit_message
        ),
        JsonParserType::Gemini => format!(
            r#"{{\"type\":\"message\",\"role\":\"assistant\",\"content\":\"{}\"}}
{{\"type\":\"result\",\"status\":\"success\"}}
"#,
            commit_message.replace('\n', "\\n")
        ),
        JsonParserType::OpenCode => format!(
            r#"{{\"type\":\"text\",\"content\":\"{}\"}}
{{\"type\":\"end\",\"success\":true}}
"#,
            commit_message.replace('\n', "\\n")
        ),
        JsonParserType::Generic => format!("{}\n", commit_message),
    }
}
