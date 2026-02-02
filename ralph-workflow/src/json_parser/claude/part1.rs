use crate::common::truncate_text;
use crate::config::Verbosity;
use crate::logger::{Colors, CHECK, CROSS};
use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::delta_display::{DeltaDisplayFormatter, DeltaRenderer, TextDeltaRenderer};
use super::health::HealthMonitor;
#[cfg(feature = "test-utils")]
use super::health::StreamingQualityMetrics;
use super::printer::SharedPrinter;
use super::streaming_state::StreamingSession;
use super::terminal::TerminalMode;
use super::types::{
    format_tool_input, format_unknown_json_event, ClaudeEvent, ContentBlock, ContentBlockDelta,
    ContentType, StreamInnerEvent,
};

// Claude event parser
//
// Note: This parser is designed for single-threaded use only.
// The internal state uses `Rc<RefCell<>>` for convenience, not for thread safety.
// Do not share this parser across threads.
pub struct ClaudeParser {
    colors: Colors,
    pub(crate) verbosity: Verbosity,
    // Relative path to log file (if logging enabled)
    log_path: Option<std::path::PathBuf>,
    display_name: String,
    // Unified streaming session tracker
    // Provides single source of truth for streaming state across all content types
    streaming_session: Rc<RefCell<StreamingSession>>,
    // Terminal mode for output formatting
    // Detected at parse time and cached for performance
    terminal_mode: RefCell<TerminalMode>,
    // Whether to show streaming quality metrics
    show_streaming_metrics: bool,
    // Output printer for capturing or displaying output
    printer: SharedPrinter,
}
