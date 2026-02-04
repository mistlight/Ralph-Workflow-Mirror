//! Regression test for Codex reasoning spam bug.
//!
//! This test verifies that Codex reasoning streaming does not spam repeated
//! "[ccs/codex] Thinking:" lines in non-TTY output modes (logs).
//!
//! Bug reproduction: When Codex emits lengthy reasoning deltas (e.g., "diff too large"
//! scenarios), each delta was printed as a fresh line instead of updating in-place,
//! causing dozens of repeated "[ccs/codex] Thinking:" lines in logs.
//!
//! Fix: Align Codex reasoning with Claude's approach using StreamingSession state
//! tracking and ThinkingDeltaRenderer with proper terminal mode awareness.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::codex::CodexParser;
use ralph_workflow::json_parser::printer::TestPrinter;
use ralph_workflow::json_parser::terminal::TerminalMode;
use ralph_workflow::logger::Colors;
use ralph_workflow::workspace::MemoryWorkspace;
use std::cell::RefCell;
use std::io::BufReader;
use std::rc::Rc;

#[test]
fn test_codex_reasoning_no_spam_in_non_tty_basic_mode() {
    with_default_timeout(|| {
        // Simulate non-TTY environment (Basic mode: colors but no cursor positioning)
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, test_printer.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::Basic);

        // Simulated stream with multiple reasoning deltas (same pattern as real bug)
        let stream = r#"
{"type":"item.started","item":{"type":"reasoning","text":"Analyzing the problem..."}}
{"type":"item.started","item":{"type":"reasoning","text":" The diff is too large"}}
{"type":"item.started","item":{"type":"reasoning","text":" to read all at once,"}}
{"type":"item.started","item":{"type":"reasoning","text":" so I need to break it down"}}
{"type":"item.started","item":{"type":"reasoning","text":" into smaller pieces."}}
{"type":"item.completed","item":{"type":"reasoning"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // In Basic mode, per-delta output is suppressed
        // Should see exactly ONE "Thinking:" line (at completion)
        let thinking_count = output.matches("[ccs/codex] Thinking:").count();
        assert_eq!(
            thinking_count, 1,
            "Expected exactly 1 'Thinking:' line in Basic mode (flushed at completion), found {}. Output:\n{}",
            thinking_count, output
        );

        // Verify the complete accumulated content is shown once
        assert!(
            output.contains("Analyzing the problem... The diff is too large to read all at once, so I need to break it down into smaller pieces."),
            "Expected full accumulated reasoning content to be flushed once. Output:\n{}", output
        );
    });
}

#[test]
fn test_codex_reasoning_no_spam_in_non_tty_none_mode() {
    with_default_timeout(|| {
        // Simulate pure non-TTY environment (None mode: no ANSI sequences at all)
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, test_printer.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::None);

        // Same stream as above
        let stream = r#"
{"type":"item.started","item":{"type":"reasoning","text":"Analyzing the problem..."}}
{"type":"item.started","item":{"type":"reasoning","text":" The diff is too large"}}
{"type":"item.started","item":{"type":"reasoning","text":" to read all at once,"}}
{"type":"item.started","item":{"type":"reasoning","text":" so I need to break it down"}}
{"type":"item.started","item":{"type":"reasoning","text":" into smaller pieces."}}
{"type":"item.completed","item":{"type":"reasoning"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // In None mode, per-delta output is also suppressed
        // Should see exactly ONE "Thinking:" line (at completion)
        let thinking_count = output.matches("[ccs/codex] Thinking:").count();
        assert_eq!(
            thinking_count, 1,
            "Expected exactly 1 'Thinking:' line in None mode (flushed at completion), found {}. Output:\n{}",
            thinking_count, output
        );
    });
}

#[test]
fn test_codex_reasoning_in_place_updates_in_full_mode() {
    with_default_timeout(|| {
        // Full TTY mode: should use in-place updates with cursor positioning
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, test_printer.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::Full);

        let stream = r#"
{"type":"item.started","item":{"type":"reasoning","text":"First chunk"}}
{"type":"item.started","item":{"type":"reasoning","text":" second"}}
{"type":"item.started","item":{"type":"reasoning","text":" third"}}
{"type":"item.completed","item":{"type":"reasoning"}}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // In Full mode, we should see:
        // 1. First delta: "[ccs/codex] Thinking: First chunk\n\x1b[1A" (newline + cursor up)
        // 2. Subsequent deltas: "\x1b[2K\r[ccs/codex] Thinking: First chunk second\n\x1b[1A"
        // 3. Completion: "\x1b[1B\n" (cursor down + newline)

        // Verify cursor sequences are present (in-place update pattern)
        assert!(
            output.contains("\x1b[1A"), // cursor up (from in-place updates)
            "Expected cursor up sequences for in-place updates in Full mode. Output:\n{}",
            output
        );
        assert!(
            output.contains("\x1b[2K"), // clear line (from subsequent deltas)
            "Expected line clear sequences for in-place updates in Full mode. Output:\n{}",
            output
        );
        assert!(
            output.contains("\x1b[1B"), // cursor down (from completion)
            "Expected cursor down sequence at completion in Full mode. Output:\n{}",
            output
        );
    });
}

#[test]
fn test_codex_reasoning_multiple_turns_no_cross_contamination() {
    with_default_timeout(|| {
        // Verify that reasoning state is properly reset between turns
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let colors = Colors::new();
        let verbosity = Verbosity::Normal;

        let parser = CodexParser::with_printer_for_test(colors, verbosity, test_printer.clone())
            .with_display_name_for_test("ccs/codex")
            .with_terminal_mode(TerminalMode::Basic);

        let stream = r#"
{"type":"turn.started"}
{"type":"item.started","item":{"type":"reasoning","text":"Turn 1 reasoning"}}
{"type":"item.completed","item":{"type":"reasoning"}}
{"type":"turn.completed"}
{"type":"turn.started"}
{"type":"item.started","item":{"type":"reasoning","text":"Turn 2 reasoning"}}
{"type":"item.completed","item":{"type":"reasoning"}}
{"type":"turn.completed"}
"#;

        let reader = BufReader::new(stream.as_bytes());
        let workspace = MemoryWorkspace::new_test();
        parser.parse_stream_for_test(reader, &workspace).unwrap();

        let output = test_printer.borrow().get_output();

        // Should see exactly 2 "Thinking:" lines (one per turn)
        let thinking_count = output.matches("[ccs/codex] Thinking:").count();
        assert_eq!(
            thinking_count, 2,
            "Expected 2 'Thinking:' lines (one per turn), found {}. Output:\n{}",
            thinking_count, output
        );

        // Verify each turn's reasoning is shown separately
        assert!(
            output.contains("Turn 1 reasoning"),
            "Expected Turn 1 reasoning in output"
        );
        assert!(
            output.contains("Turn 2 reasoning"),
            "Expected Turn 2 reasoning in output"
        );
    });
}
