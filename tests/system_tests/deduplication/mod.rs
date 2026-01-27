//! System tests for streaming output deduplication with real fixture files.
//!
//! These tests require real filesystem access to large fixture files that
//! are impractical to embed at compile time.
//!
//! # Why System Tests
//!
//! The fixture file `PROMPT-LOG.log` is 1.3MB - too large for `include_str!`.
//! Using real filesystem read is acceptable here because:
//! - This is a static fixture file, not dynamic test state
//! - The test gracefully skips if the fixture is not found
//! - Testing production log parsing is valuable for regression testing
//!
//! # Running
//!
//! ```bash
//! cargo test -p ralph-workflow-system-tests -- deduplication
//! ```

use std::cell::RefCell;
use std::io::{BufReader, Cursor};
use std::path::Path;
use std::rc::Rc;

use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::printer::{SharedPrinter, VirtualTerminal};
use ralph_workflow::json_parser::terminal::TerminalMode;
use ralph_workflow::json_parser::ClaudeParser;
use ralph_workflow::logger::Colors;
use ralph_workflow::workspace::MemoryWorkspace;

use crate::test_timeout::with_default_timeout;

// =============================================================================
// Helper Functions
// =============================================================================

/// Create a Claude parser with VirtualTerminal in Full mode (ANSI sequences enabled).
fn create_parser_with_vterm() -> (ClaudeParser, Rc<RefCell<VirtualTerminal>>) {
    let vterm = Rc::new(RefCell::new(VirtualTerminal::new()));
    let printer: SharedPrinter = vterm.clone();
    let parser = ClaudeParser::with_printer(Colors::new(), Verbosity::Normal, printer)
        .with_terminal_mode(TerminalMode::Full);
    (parser, vterm)
}

// =============================================================================
// Real Fixture File Tests
// =============================================================================

/// Test with real production log file using VirtualTerminal.
///
/// This verifies that when an actual production log file is parsed,
/// the deduplication system prevents duplicate visible content.
///
/// This test is in system_tests because the fixture file is 1.3MB,
/// too large for `include_str!` embedding.
#[test]
fn test_real_log_file_no_visible_duplicates() {
    with_default_timeout(|| {
        // Try multiple paths since working directory may vary
        let possible_paths = [
            "tests/integration_tests/deduplication/fixtures/PROMPT-LOG.log",
            "../tests/integration_tests/deduplication/fixtures/PROMPT-LOG.log",
            "../../tests/integration_tests/deduplication/fixtures/PROMPT-LOG.log",
        ];

        let log_path = possible_paths.iter().find(|p| Path::new(p).exists());

        let Some(log_path) = log_path else {
            eprintln!("Skipping real log test - fixture not found");
            return;
        };

        let log_content = std::fs::read_to_string(log_path)
            .unwrap_or_else(|e| panic!("Failed to read log file: {}", e));

        let (parser, vterm) = create_parser_with_vterm();
        let workspace = MemoryWorkspace::new_test();
        let cursor = Cursor::new(log_content);
        let reader = BufReader::new(cursor);
        parser
            .parse_stream(reader, &workspace)
            .expect("parse_stream should succeed");

        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // Should have some output
        assert!(!visible.trim().is_empty(), "Should produce visible output");

        // No duplicate visible lines
        assert!(
            !vterm_ref.has_duplicate_lines(),
            "Real log should have no duplicate visible lines"
        );

        // Verify deduplication metrics
        let metrics = parser.streaming_metrics();
        println!(
            "Real log metrics: {} deltas, {} snapshot repairs",
            metrics.total_deltas, metrics.snapshot_repairs_count
        );
    });
}
