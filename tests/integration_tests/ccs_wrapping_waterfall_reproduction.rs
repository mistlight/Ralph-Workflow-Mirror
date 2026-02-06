//! Test that streaming with wrapping does NOT cause multi-line waterfall.
//!
//! Bug: Current Full mode uses `\n\x1b[1A` pattern which fails when content wraps:
//! - Content exceeds terminal width
//! - Terminal wraps to multiple rows
//! - `\x1b[2K` only clears current row, not wrapped rows above
//! - Result: multiple visible lines instead of in-place update
//!
//! Expected: ChatGPT-style streaming with append-only pattern produces only 1 visible line.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::claude::ClaudeParser;
use ralph_workflow::json_parser::codex::CodexParser;
use ralph_workflow::json_parser::printer::VirtualTerminal;
use ralph_workflow::json_parser::terminal::TerminalMode;
use ralph_workflow::logger::Colors;
use ralph_workflow::workspace::MemoryWorkspace;
use std::cell::RefCell;
use std::io::BufReader;
use std::rc::Rc;

#[test]
fn test_wrapping_no_waterfall_claude() {
    with_default_timeout(|| {
        // Use a narrow terminal (40 cols) to force wrapping
        let terminal = Rc::new(RefCell::new(VirtualTerminal::new_with_geometry(40, 24)));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = ClaudeParser::with_printer(colors, verbosity, terminal.clone())
            .with_display_name("ccs/glm")
            .with_terminal_mode(TerminalMode::Full);

        // Stream text that will definitely wrap (80+ chars, terminal is 40 cols)
        let stream = r#"
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg1","content":[]}}}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"This is a very long message that will definitely wrap across multiple lines in a narrow terminal"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" and even more text to ensure wrapping"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" final delta"}}}
{"type":"stream_event","event":{"type":"content_block_stop","index":0}}
{"type":"stream_event","event":{"type":"message_stop"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream(reader, &workspace).unwrap();

        let term = terminal.borrow();
        let screen_content = term.get_visible_output();

        // In ChatGPT-style append-only streaming, we expect:
        // 1. The prefix appears exactly ONCE (not repeated for each delta)
        // 2. The full content is present
        // 3. Content may wrap to multiple rows (this is expected with narrow terminal)

        let prefix_count = screen_content.matches("[ccs/glm]").count();
        assert_eq!(
            prefix_count, 1,
            "Expected prefix to appear exactly once, found {} times. \
             This indicates waterfall bug where each delta repeats the prefix.\n\
             Screen content:\n{}",
            prefix_count, screen_content
        );

        // Verify the full content is present
        assert!(
            screen_content.contains("This is a very long message"),
            "Content should be visible. Screen:\n{}",
            screen_content
        );
        assert!(
            screen_content.contains("final delta"),
            "Final delta content should be visible. Screen:\n{}",
            screen_content
        );

        // Verify the content is actually present (not lost)
        let screen_content = term.get_visible_output();
        assert!(
            screen_content.contains("This is a very long message"),
            "Content should be visible. Screen:\n{}",
            screen_content
        );
    });
}

#[test]
fn test_wrapping_no_waterfall_codex() {
    with_default_timeout(|| {
        // Similar test for Codex parser
        let terminal = Rc::new(RefCell::new(VirtualTerminal::new_with_geometry(40, 24)));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, terminal.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::Full);

        // Stream text with reasoning that will wrap
        let stream = r#"
{"id":"init","event":"session.created"}
{"id":"msg1","event":"conversation.item.created","item":{"id":"item1","type":"message","role":"assistant","content":[]}}
{"id":"msg1","event":"response.created","response":{"id":"resp1","status":"in_progress"}}
{"id":"msg1","event":"response.output_item.added","item":{"id":"item1","type":"message","role":"assistant","content":[]}}
{"id":"msg1","event":"conversation.item.input_audio_transcription.completed","item_id":"item1","transcript":"User request"}
{"id":"msg1","event":"response.content_part.added","part":{"type":"reasoning","text":""},"content_index":0,"item_id":"item1"}
{"id":"msg1","event":"response.reasoning.delta","delta":"This is extensive reasoning text that will definitely exceed the terminal width and cause wrapping across multiple lines in the narrow terminal window"}
{"id":"msg1","event":"response.reasoning.delta","delta":" even more reasoning to ensure wrapping"}
{"id":"msg1","event":"response.reasoning.done"}
{"id":"msg1","event":"response.done","response":{"status":"completed"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let term = terminal.borrow();
        let visible_lines = term.count_visible_lines();

        assert_eq!(
            visible_lines, 1,
            "Expected 1 visible line for Codex streaming with wrapping, found {}",
            visible_lines
        );
    });
}
