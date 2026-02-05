//! Comprehensive regression test for CCS streaming delta spam bug.
//!
//! This test verifies that CCS agents (ccs/glm and ccs/codex) do not spam repeated
//! prefixed lines in non-TTY output modes for ANY delta type (text, thinking, tool input).
//!
//! Bug reproduction: When CCS agents emit streaming deltas (text/thinking/tool input),
//! each delta was printed as a fresh line instead of updating in-place in non-TTY modes,
//! causing repeated "[ccs/glm]" or "[ccs/codex]" lines in logs.
//!
//! Fix: Suppress per-delta output in non-TTY modes (TerminalMode::Basic and TerminalMode::None)
//! and flush accumulated content ONCE at completion boundaries.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::claude::ClaudeParser;
use ralph_workflow::json_parser::codex::CodexParser;
use ralph_workflow::json_parser::printer::TestPrinter;
use ralph_workflow::json_parser::terminal::TerminalMode;
use ralph_workflow::logger::Colors;
use ralph_workflow::workspace::MemoryWorkspace;
use std::cell::RefCell;
use std::io::BufReader;
use std::rc::Rc;

// ============================================================================
// CCS/GLM (ClaudeParser) Tests
// ============================================================================

#[test]
fn test_ccs_glm_text_deltas_no_spam_in_none_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = ClaudeParser::with_printer(colors, verbosity, test_printer.clone())
            .with_display_name("ccs/glm")
            .with_terminal_mode(TerminalMode::None);

        // Simulate many text deltas for same block
        let stream = r#"
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","content":[]}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" This"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" is"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" a"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" test"}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"message_stop"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // Count prefix occurrences - should be AT MOST 1 for the text block
        let prefix_count = output.matches("[ccs/glm]").count();
        assert!(
            prefix_count <= 1,
            "Expected <= 1 '[ccs/glm]' prefix in None mode for text deltas, found {}.\n\nOutput:\n{}",
            prefix_count,
            output
        );

        // Verify the content is present (not lost)
        assert!(
            output.contains("Hello World! This is a test") || output.contains("Hello World!"),
            "Expected accumulated text content to be present. Output:\n{}",
            output
        );
    });
}

#[test]
fn test_ccs_glm_text_deltas_no_spam_in_basic_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = ClaudeParser::with_printer(colors, verbosity, test_printer.clone())
            .with_display_name("ccs/glm")
            .with_terminal_mode(TerminalMode::Basic);

        let stream = r#"
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","content":[]}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"message_stop"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        let prefix_count = output.matches("[ccs/glm]").count();
        assert!(
            prefix_count <= 1,
            "Expected <= 1 '[ccs/glm]' prefix in Basic mode for text deltas, found {}.\n\nOutput:\n{}",
            prefix_count,
            output
        );
    });
}

#[test]
fn test_ccs_glm_thinking_deltas_no_spam_in_none_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = ClaudeParser::with_printer(colors, verbosity, test_printer.clone())
            .with_display_name("ccs/glm")
            .with_terminal_mode(TerminalMode::None);

        // Simulate many thinking deltas for same block
        let stream = r#"
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","content":[]}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"I"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" need"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" to"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" think"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" about"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" this"}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"message_stop"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // Count thinking prefix occurrences - should be AT MOST 1.
        // If colors are forced on in CI, the line may include ANSI sequences between
        // "]" and "Thinking:". Match on the prefix and "Thinking:" separately.
        let thinking_prefix_count = output.matches("[ccs/glm]").count();
        let thinking_label_count = output.matches("Thinking:").count();
        assert!(
            thinking_prefix_count <= 1,
            "Expected <= 1 '[ccs/glm]' prefix in None mode for thinking-only stream, found {}.\n\nOutput:\n{}",
            thinking_prefix_count,
            output
        );
        assert!(
            thinking_label_count <= 1,
            "Expected <= 1 'Thinking:' label in None mode, found {}.\n\nOutput:\n{}",
            thinking_label_count,
            output
        );

        // Verify thinking content is present (not lost)
        if thinking_label_count > 0 {
            assert!(
                output.contains("I need to think about this") || output.contains("think about"),
                "Expected accumulated thinking content to be present. Output:\n{}",
                output
            );
        }
    });
}

#[test]
fn test_ccs_glm_thinking_deltas_no_spam_in_basic_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = ClaudeParser::with_printer(colors, verbosity, test_printer.clone())
            .with_display_name("ccs/glm")
            .with_terminal_mode(TerminalMode::Basic);

        let stream = r#"
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","content":[]}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Analyzing"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" the"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" problem"}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"message_stop"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        let thinking_label_count = output.matches("Thinking:").count();
        assert!(
            thinking_label_count <= 1,
            "Expected <= 1 'Thinking:' label in Basic mode, found {}.\n\nOutput:\n{}",
            thinking_label_count,
            output
        );
    });
}

#[test]
fn test_ccs_glm_tool_input_deltas_no_spam_in_none_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = ClaudeParser::with_printer(colors, verbosity, test_printer.clone())
            .with_display_name("ccs/glm")
            .with_terminal_mode(TerminalMode::None);

        // Simulate many tool input deltas
        let stream = r#"
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","content":[]}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool1","name":"bash"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"tool_use_delta","tool_use":{"input":"ls"}}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"tool_use_delta","tool_use":{"input":" -la"}}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"tool_use_delta","tool_use":{"input":" /tmp"}}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"message_stop"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // Count tool input prefix occurrences - should be AT MOST 1-2
        // (one for tool start, at most one for tool input flush)
        let prefix_count = output.matches("[ccs/glm]").count();
        assert!(
            prefix_count <= 2,
            "Expected <= 2 '[ccs/glm]' prefixes in None mode for tool deltas (tool start + optional input flush), found {}.\n\nOutput:\n{}",
            prefix_count,
            output
        );
    });
}

#[test]
fn test_ccs_glm_tool_input_deltas_no_spam_in_basic_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = ClaudeParser::with_printer(colors, verbosity, test_printer.clone())
            .with_display_name("ccs/glm")
            .with_terminal_mode(TerminalMode::Basic);

        let stream = r#"
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","content":[]}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool1","name":"bash"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"tool_use_delta","tool_use":{"input":"echo"}}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"tool_use_delta","tool_use":{"input":" hello"}}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"message_stop"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        let prefix_count = output.matches("[ccs/glm]").count();
        assert!(
            prefix_count <= 2,
            "Expected <= 2 '[ccs/glm]' prefixes in Basic mode for tool deltas, found {}.\n\nOutput:\n{}",
            prefix_count,
            output
        );
    });
}

#[test]
fn test_ccs_glm_mixed_deltas_no_spam_in_none_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = ClaudeParser::with_printer(colors, verbosity, test_printer.clone())
            .with_display_name("ccs/glm")
            .with_terminal_mode(TerminalMode::None);

        // Simulate a message with thinking, then text, then tool
        let stream = r#"
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","content":[]}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" me"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" think"}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":" there"}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":1}}
{"type":"stream_event","event":{"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"tool1","name":"bash"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":2,"delta":{"type":"tool_use_delta","tool_use":{"input":"ls"}}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":2,"delta":{"type":"tool_use_delta","tool_use":{"input":" -la"}}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":2}}
{"type":"stream_event","event":{"type":"message_stop"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // Count total prefix occurrences
        // Should be AT MOST: 1 thinking + 1 text + 2 tool (start + input) = 4
        let prefix_count = output.matches("[ccs/glm]").count();
        assert!(
            prefix_count <= 4,
            "Expected <= 4 '[ccs/glm]' prefixes in None mode for mixed deltas (1 thinking + 1 text + 2 tool), found {}.\n\nOutput:\n{}",
            prefix_count,
            output
        );
    });
}

// ============================================================================
// CCS/Codex (CodexParser) Tests
// ============================================================================

#[test]
fn test_ccs_codex_reasoning_deltas_no_spam_in_none_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, test_printer.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::None);

        // Simulate many reasoning deltas
        let stream = r#"
{"type":"item.started","item":{"type":"reasoning","text":"First"}}
{"type":"item.started","item":{"type":"reasoning","text":" chunk"}}
{"type":"item.started","item":{"type":"reasoning","text":" second"}}
{"type":"item.started","item":{"type":"reasoning","text":" third"}}
{"type":"item.started","item":{"type":"reasoning","text":" fourth"}}
{"type":"item.started","item":{"type":"reasoning","text":" fifth"}}
{"type":"item.completed","item":{"type":"reasoning"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // Count reasoning label occurrences - should be AT MOST 1.
        // If colors are forced on in CI, ANSI sequences may appear between the prefix and label.
        let thinking_label_count = output.matches("Thinking:").count();
        assert!(
            thinking_label_count <= 1,
            "Expected <= 1 'Thinking:' label in None mode, found {}.\n\nOutput:\n{}",
            thinking_label_count,
            output
        );

        // Verify reasoning content is present (not lost)
        if thinking_label_count > 0 {
            assert!(
                output.contains("First chunk") || output.contains("fifth"),
                "Expected accumulated reasoning content to be present. Output:\n{}",
                output
            );
        }
    });
}

#[test]
fn test_ccs_codex_reasoning_deltas_no_spam_in_basic_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, test_printer.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::Basic);

        let stream = r#"
{"type":"item.started","item":{"type":"reasoning","text":"Analyzing"}}
{"type":"item.started","item":{"type":"reasoning","text":" the"}}
{"type":"item.started","item":{"type":"reasoning","text":" problem"}}
{"type":"item.completed","item":{"type":"reasoning"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        let thinking_label_count = output.matches("Thinking:").count();
        assert!(
            thinking_label_count <= 1,
            "Expected <= 1 'Thinking:' label in Basic mode, found {}.\n\nOutput:\n{}",
            thinking_label_count,
            output
        );
    });
}

#[test]
fn test_ccs_codex_agent_message_deltas_no_spam_in_none_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, test_printer.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::None);

        // Simulate many agent_message deltas
        let stream = r#"
{"type":"item.started","item":{"type":"agent_message","text":"Hello"}}
{"type":"item.started","item":{"type":"agent_message","text":" World"}}
{"type":"item.started","item":{"type":"agent_message","text":"!"}}
{"type":"item.completed","item":{"type":"agent_message","text":"Hello World!"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // Count prefix occurrences - should be AT MOST 1
        let prefix_count = output.matches("[ccs/codex]").count();
        assert!(
            prefix_count <= 1,
            "Expected <= 1 '[ccs/codex]' prefix in None mode for agent_message deltas, found {}.\n\nOutput:\n{}",
            prefix_count,
            output
        );

        // Verify the content is present (not lost)
        assert!(
            output.contains("Hello World!"),
            "Expected accumulated agent_message content to be present. Output:\n{}",
            output
        );
    });
}

#[test]
fn test_ccs_codex_agent_message_deltas_no_spam_in_basic_mode() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, test_printer.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::Basic);

        let stream = r#"
{"type":"item.started","item":{"type":"agent_message","text":"Test"}}
{"type":"item.started","item":{"type":"agent_message","text":" message"}}
{"type":"item.completed","item":{"type":"agent_message","text":"Test message"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        let prefix_count = output.matches("[ccs/codex]").count();
        assert!(
            prefix_count <= 1,
            "Expected <= 1 '[ccs/codex]' prefix in Basic mode for agent_message deltas, found {}.\n\nOutput:\n{}",
            prefix_count,
            output
        );

        assert!(
            output.contains("Test message"),
            "Expected accumulated agent_message content to be present. Output:\n{}",
            output
        );
    });
}
