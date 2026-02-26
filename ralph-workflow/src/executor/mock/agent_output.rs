use crate::agents::JsonParserType;

/// Generate minimal valid agent output for mock testing.
///
/// This creates a minimal valid NDJSON output that the streaming parser can
/// successfully parse without hanging. The output format depends on the parser
/// type being used.
pub(super) fn generate_mock_agent_output(parser_type: JsonParserType, command: &str) -> String {
    let primary_text = if command.contains("claude") {
        // The claude parser uses assistant text blocks directly.
        // Include a recognizable analysis-task header so tests can assert the analysis prompt
        // was materialized (rather than a stale continuation prompt).
        "ANALYSIS TASK\nYou are an independent, objective code analysis agent."
    } else {
        r"<ralph-commit>
 <ralph-subject>test: commit message</ralph-subject>
 <ralph-body>Test commit message for integration tests.</ralph-body>
 </ralph-commit>"
    };

    fn line(value: serde_json::Value) -> String {
        let json = serde_json::to_string(&value).expect("mock JSON must serialize");
        format!("{json}\n")
    }

    match parser_type {
        JsonParserType::Claude => {
            let mut out = String::new();
            out.push_str(&line(serde_json::json!({
                "type": "system",
                "subtype": "init",
                "session_id": "ses_mock_session_12345"
            })));
            out.push_str(&line(serde_json::json!({
                "type": "assistant",
                "message": {
                    "content": [
                        {"type": "text", "text": primary_text}
                    ]
                }
            })));
            out.push_str(&line(serde_json::json!({
                "type": "result",
                "subtype": "success"
            })));
            out
        }
        JsonParserType::Codex => {
            let mut out = String::new();
            out.push_str(&line(serde_json::json!({
                "type": "thread.started",
                "thread_id": "thr_mock"
            })));
            out.push_str(&line(serde_json::json!({
                "type": "item.started",
                "item": {"type": "agent_message", "text": ""}
            })));
            out.push_str(&line(serde_json::json!({
                "type": "item.completed",
                "item": {"type": "agent_message", "text": primary_text}
            })));
            out.push_str(&line(serde_json::json!({
                "type": "turn.completed",
                "usage": {"input_tokens": 1, "output_tokens": 1}
            })));
            out
        }
        JsonParserType::Gemini => {
            let mut out = String::new();
            out.push_str(&line(serde_json::json!({
                "type": "init",
                "timestamp": "2025-10-10T12:00:00.000Z",
                "session_id": "ses_mock_session_12345",
                "model": "gemini-mock"
            })));
            out.push_str(&line(serde_json::json!({
                "type": "message",
                "role": "assistant",
                "content": primary_text,
                "timestamp": "2025-10-10T12:00:01.000Z"
            })));
            out
        }
        JsonParserType::OpenCode => {
            let session_id = "ses_mock_session_12345";
            let mut out = String::new();
            out.push_str(&line(serde_json::json!({
                "type": "step_start",
                "timestamp": 1u64,
                "sessionID": session_id,
                "part": {
                    "id": "prt_1",
                    "sessionID": session_id,
                    "messageID": "msg_1",
                    "type": "step-start",
                    "snapshot": "deadbeef"
                }
            })));
            out.push_str(&line(serde_json::json!({
                "type": "text",
                "timestamp": 2u64,
                "sessionID": session_id,
                "part": {
                    "id": "prt_2",
                    "sessionID": session_id,
                    "messageID": "msg_1",
                    "type": "text",
                    "text": primary_text
                }
            })));
            out.push_str(&line(serde_json::json!({
                "type": "step_finish",
                "timestamp": 3u64,
                "sessionID": session_id,
                "part": {
                    "id": "prt_3",
                    "sessionID": session_id,
                    "messageID": "msg_1",
                    "type": "step-finish",
                    "reason": "end_turn"
                }
            })));
            out
        }
        JsonParserType::Generic => format!("{}\n", primary_text),
    }
}
