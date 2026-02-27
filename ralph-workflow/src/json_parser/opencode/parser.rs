// OpenCode parser implementation.
//
// Contains the OpenCodeParser struct and its core methods.

/// `OpenCode` event parser
pub struct OpenCodeParser {
    colors: Colors,
    verbosity: Verbosity,
    /// Relative path to log file (if logging enabled)
    log_path: Option<std::path::PathBuf>,
    display_name: String,
    /// Unified streaming session for state tracking
    streaming_session: Rc<RefCell<StreamingSession>>,
    /// Terminal mode for output formatting
    terminal_mode: RefCell<TerminalMode>,
    /// Track last rendered content for append-only streaming.
    last_rendered_content: RefCell<std::collections::HashMap<String, String>>,
    /// Whether to show streaming quality metrics
    show_streaming_metrics: bool,
    /// Output printer for capturing or displaying output
    printer: SharedPrinter,
    /// Counter for step IDs when events lack stable identifiers
    fallback_step_counter: Cell<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MonitorEventClassification {
    Parsed,
    Partial,
    Control,
    Unknown,
    ParseError,
    Ignored,
}

const MAX_XML_SEARCH_BYTES: usize = 512 * 1024;
const MAX_XML_BYTES: usize = 128 * 1024;

impl OpenCodeParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self::with_printer(colors, verbosity, super::printer::shared_stdout())
    }

    /// Create a new `OpenCodeParser` with a custom printer.
    ///
    /// # Arguments
    ///
    /// * `colors` - Colors for terminal output
    /// * `verbosity` - Verbosity level for output
    /// * `printer` - Shared printer for output
    ///
    /// # Returns
    ///
    /// A new `OpenCodeParser` instance
    pub(crate) fn with_printer(
        colors: Colors,
        verbosity: Verbosity,
        printer: SharedPrinter,
    ) -> Self {
        let verbose_warnings = matches!(verbosity, Verbosity::Debug);
        let streaming_session = StreamingSession::new().with_verbose_warnings(verbose_warnings);

        // Use the printer's is_terminal method to validate it's connected correctly
        let _printer_is_terminal = printer.borrow().is_terminal();

        Self {
            colors,
            verbosity,
            log_path: None,
            display_name: "OpenCode".to_string(),
            streaming_session: Rc::new(RefCell::new(streaming_session)),
            terminal_mode: RefCell::new(TerminalMode::detect()),
            last_rendered_content: RefCell::new(std::collections::HashMap::new()),
            show_streaming_metrics: false,
            printer,
            fallback_step_counter: Cell::new(0),
        }
    }

    pub(crate) const fn with_show_streaming_metrics(mut self, show: bool) -> Self {
        self.show_streaming_metrics = show;
        self
    }

    pub(crate) fn with_display_name(mut self, display_name: &str) -> Self {
        self.display_name = display_name.to_string();
        self
    }

    pub(crate) fn with_log_file(mut self, path: &str) -> Self {
        self.log_path = Some(std::path::PathBuf::from(path));
        self
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn with_terminal_mode(self, mode: TerminalMode) -> Self {
        *self.terminal_mode.borrow_mut() = mode;
        self
    }

    /// Create a new parser with a test printer.
    ///
    /// This is the primary entry point for integration tests that need
    /// to capture parser output for verification.
    ///
    /// Defaults to `TerminalMode::Full` for testing streaming behavior.
    /// Integration tests that verify streaming output need Full mode to
    /// see per-delta rendering (non-TTY modes suppress deltas and flush at completion).
    #[cfg(feature = "test-utils")]
    pub fn with_printer_for_test(
        colors: Colors,
        verbosity: Verbosity,
        printer: SharedPrinter,
    ) -> Self {
        Self::with_printer(colors, verbosity, printer).with_terminal_mode(TerminalMode::Full)
    }

    /// Set the log file path for testing.
    ///
    /// This allows tests to verify log file content after parsing.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn with_log_file_for_test(mut self, path: &str) -> Self {
        self.log_path = Some(std::path::PathBuf::from(path));
        self
    }

    /// Parse a stream for testing purposes.
    ///
    /// This exposes the internal `parse_stream` method for integration tests.
    ///
    /// # Errors
    ///
    /// Returns an error if stream parsing or file operations fail.
    #[cfg(feature = "test-utils")]
    pub fn parse_stream_for_test<R: std::io::BufRead>(
        &self,
        reader: R,
        workspace: &dyn crate::workspace::Workspace,
    ) -> std::io::Result<()> {
        self.parse_stream(reader, workspace)
    }

    /// Get a shared reference to the printer.
    ///
    /// This allows tests, monitoring, and other code to access the printer after parsing
    /// to verify output content, check for duplicates, or capture output for analysis.
    /// Only available with the `test-utils` feature.
    ///
    /// # Returns
    ///
    /// A clone of the shared printer reference (`Rc<RefCell<dyn Printable>>`)
    #[cfg(feature = "test-utils")]
    pub fn printer(&self) -> SharedPrinter {
        Rc::clone(&self.printer)
    }

    /// Get streaming quality metrics from the current session.
    ///
    /// This provides insight into the deduplication and streaming quality of the
    /// parsing session. Only available with the `test-utils` feature.
    ///
    /// # Returns
    ///
    /// A copy of the streaming quality metrics from the internal `StreamingSession`.
    #[cfg(feature = "test-utils")]
    pub fn streaming_metrics(&self) -> StreamingQualityMetrics {
        self.streaming_session
            .borrow()
            .get_streaming_quality_metrics()
    }

    /// Parse and display a single `OpenCode` JSON event
    ///
    /// From `OpenCode` source (`run.ts` lines 146-201), the NDJSON format uses events with:
    /// - `step_start`: Step initialization with snapshot info
    /// - `step_finish`: Step completion with reason, cost, tokens
    /// - `tool_use`: Tool invocation with tool name, callID, and state (status, input, output)
    /// - `text`: Streaming text content
    /// - `error`: Session/API error events
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: OpenCodeEvent = if let Ok(e) = serde_json::from_str(line) {
            e
        } else {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('{') {
                return Some(format!("{trimmed}\n"));
            }
            return None;
        };
        let c = &self.colors;
        let prefix = &self.display_name;

        let output = match event.event_type.as_str() {
            "step_start" => self.format_step_start_event(&event),
            "step_finish" => self.format_step_finish_event(&event),
            "tool_use" => self.format_tool_use_event(&event),
            "text" => self.format_text_event(&event),
            "error" => self.format_error_event(&event, line),
            _ => {
                // Unknown event type - use the generic formatter in verbose mode
                format_unknown_json_event(line, prefix, *c, self.verbosity.is_verbose())
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    fn next_fallback_step_id(&self, session: &str, timestamp: Option<u64>) -> String {
        let counter = self.fallback_step_counter.get().saturating_add(1);
        self.fallback_step_counter.set(counter);
        timestamp.map_or_else(
            || format!("{session}:fallback:{counter}"),
            |ts| format!("{session}:{ts}:{counter}")
        )
    }

    /// Check if an `OpenCode` event is a control event (state management with no user output)
    ///
    /// Control events are valid JSON that represent state transitions rather than
    /// user-facing content. They should be tracked separately from "ignored" events
    /// to avoid false health warnings.
    fn is_control_event(event: &OpenCodeEvent) -> bool {
        match event.event_type.as_str() {
            // Step lifecycle events are control events
            "step_start" | "step_finish" => true,
            _ => false,
        }
    }

    /// Check if an `OpenCode` event is a partial/delta event (streaming content displayed incrementally)
    ///
    /// Partial events represent streaming text deltas that are shown to the user
    /// in real-time. These should be tracked separately to avoid inflating "ignored" percentages.
    fn is_partial_event(event: &OpenCodeEvent) -> bool {
        match event.event_type.as_str() {
            // Text events produce streaming content
            "text" => true,
            _ => false,
        }
    }

    fn process_stream_json_line(
        &self,
        line: &str,
        monitor: &HealthMonitor,
        logging_enabled: bool,
        log_buffer: &mut Vec<u8>,
    ) -> io::Result<()> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        self.maybe_write_debug_event(line)?;

        match self.parse_event(line) {
            Some(output) => {
                Self::record_monitor_event(monitor, Self::classify_successful_parse_for_monitor(line, trimmed));
                let mut printer = self.printer.borrow_mut();
                write!(printer, "{output}")?;
                printer.flush()?;
            }
            None => {
                Self::record_monitor_event(monitor, Self::classify_empty_output_for_monitor(line, trimmed));
            }
        }

        if logging_enabled {
            writeln!(log_buffer, "{line}")?;
        }
        Ok(())
    }

    fn maybe_write_debug_event(&self, line: &str) -> io::Result<()> {
        if !self.verbosity.is_debug() {
            return Ok(());
        }

        let c = &self.colors;
        let mut printer = self.printer.borrow_mut();
        writeln!(
            printer,
            "{}[DEBUG]{} {}{}{}",
            c.dim(),
            c.reset(),
            c.dim(),
            line,
            c.reset()
        )?;
        printer.flush()?;
        Ok(())
    }

    fn classify_successful_parse_for_monitor(
        line: &str,
        trimmed: &str,
    ) -> MonitorEventClassification {
        if trimmed.starts_with('{') {
            if let Ok(event) = serde_json::from_str::<OpenCodeEvent>(line) {
                if Self::is_partial_event(&event) {
                    return MonitorEventClassification::Partial;
                }
            }
        }
        MonitorEventClassification::Parsed
    }

    fn classify_empty_output_for_monitor(
        line: &str,
        trimmed: &str,
    ) -> MonitorEventClassification {
        if !trimmed.starts_with('{') {
            return MonitorEventClassification::Ignored;
        }

        serde_json::from_str::<OpenCodeEvent>(line).map_or(
            MonitorEventClassification::ParseError,
            |event| {
                if Self::is_control_event(&event) {
                    MonitorEventClassification::Control
                } else {
                    MonitorEventClassification::Unknown
                }
            },
        )
    }

    fn record_monitor_event(monitor: &HealthMonitor, classification: MonitorEventClassification) {
        match classification {
            MonitorEventClassification::Parsed => monitor.record_parsed(),
            MonitorEventClassification::Partial => monitor.record_partial_event(),
            MonitorEventClassification::Control => monitor.record_control_event(),
            MonitorEventClassification::Unknown => monitor.record_unknown_event(),
            MonitorEventClassification::ParseError => monitor.record_parse_error(),
            MonitorEventClassification::Ignored => monitor.record_ignored(),
        }
    }

    fn process_incremental_stream<R: BufRead>(
        &self,
        reader: &mut R,
        parser: &mut super::incremental_parser::IncrementalNdjsonParser,
        monitor: &HealthMonitor,
        logging_enabled: bool,
        log_buffer: &mut Vec<u8>,
    ) -> io::Result<()> {
        let mut byte_buffer = Vec::new();
        loop {
            byte_buffer.clear();
            let chunk = reader.fill_buf()?;
            if chunk.is_empty() {
                break;
            }

            byte_buffer.extend_from_slice(chunk);
            let consumed = chunk.len();
            reader.consume(consumed);

            for line in parser.feed(&byte_buffer) {
                self.process_stream_json_line(&line, monitor, logging_enabled, log_buffer)?;
            }
        }
        Ok(())
    }

    fn process_remaining_buffered_event(
        &self,
        remaining: &str,
        monitor: &HealthMonitor,
        logging_enabled: bool,
        log_buffer: &mut Vec<u8>,
    ) -> io::Result<()> {
        let trimmed = remaining.trim();
        if trimmed.is_empty()
            || !trimmed.starts_with('{')
            || serde_json::from_str::<OpenCodeEvent>(remaining).is_err()
        {
            return Ok(());
        }

        match self.parse_event(remaining) {
            Some(output) => {
                monitor.record_parsed();
                let mut printer = self.printer.borrow_mut();
                write!(printer, "{output}")?;
                printer.flush()?;
            }
            None => {
                Self::record_monitor_event(
                    monitor,
                    Self::classify_empty_output_for_monitor(remaining, trimmed),
                );
            }
        }

        if logging_enabled {
            writeln!(log_buffer, "{remaining}")?;
        }
        Ok(())
    }

    fn write_log_buffer_if_enabled(
        &self,
        workspace: &dyn crate::workspace::Workspace,
        log_buffer: &[u8],
    ) -> io::Result<()> {
        if let Some(log_path) = &self.log_path {
            workspace.append_bytes(log_path, log_buffer)?;
        }
        Ok(())
    }

    fn with_xml_tail_bound(accumulated: &str, max_bytes: usize) -> &str {
        if accumulated.len() <= max_bytes {
            return accumulated;
        }

        let mut start = accumulated.len() - max_bytes;
        while start < accumulated.len() && !accumulated.is_char_boundary(start) {
            start += 1;
        }
        &accumulated[start..]
    }

    fn persist_extracted_xml(
        workspace: &dyn crate::workspace::Workspace,
        output_path: &str,
        xml: &str,
    ) -> io::Result<()> {
        if xml.len() > MAX_XML_BYTES {
            return Ok(());
        }

        workspace.create_dir_all(Path::new(".agent/tmp"))?;
        workspace.write(Path::new(output_path), xml)?;
        Ok(())
    }

    fn persist_extracted_xml_artifacts(
        &self,
        workspace: &dyn crate::workspace::Workspace,
    ) -> io::Result<()> {
        let maybe_accumulated = self
            .streaming_session
            .borrow()
            .get_accumulated(ContentType::Text, "main")
            .map(str::to_owned);

        let Some(accumulated) = maybe_accumulated else {
            return Ok(());
        };

        let accumulated_tail = Self::with_xml_tail_bound(&accumulated, MAX_XML_SEARCH_BYTES);

        if let Some(xml) = crate::files::llm_output_extraction::xml_extraction::extract_xml_commit(
            accumulated_tail,
        ) {
            Self::persist_extracted_xml(
                workspace,
                crate::files::llm_output_extraction::file_based_extraction::paths::COMMIT_MESSAGE_XML,
                &xml,
            )?;
        }

        if let Some(xml) = crate::files::llm_output_extraction::extract_issues_xml(accumulated_tail) {
            Self::persist_extracted_xml(
                workspace,
                crate::files::llm_output_extraction::file_based_extraction::paths::ISSUES_XML,
                &xml,
            )?;
        }

        Ok(())
    }

    fn write_monitor_warning_if_needed(&self, monitor: &HealthMonitor) -> io::Result<()> {
        if let Some(warning) = monitor.check_and_warn(self.colors) {
            let mut printer = self.printer.borrow_mut();
            writeln!(printer, "{warning}")?;
        }
        Ok(())
    }

    /// Parse a stream of `OpenCode` NDJSON events
    pub(crate) fn parse_stream<R: BufRead>(
        &self,
        mut reader: R,
        workspace: &dyn crate::workspace::Workspace,
    ) -> io::Result<()> {
        use super::incremental_parser::IncrementalNdjsonParser;

        let monitor = HealthMonitor::new("OpenCode");
        let logging_enabled = self.log_path.is_some();
        let mut log_buffer: Vec<u8> = Vec::new();
        let mut incremental_parser = IncrementalNdjsonParser::new();

        self.process_incremental_stream(
            &mut reader,
            &mut incremental_parser,
            &monitor,
            logging_enabled,
            &mut log_buffer,
        )?;

        if let Some(remaining) = incremental_parser.finish() {
            self.process_remaining_buffered_event(
                &remaining,
                &monitor,
                logging_enabled,
                &mut log_buffer,
            )?;
        }

        self.write_log_buffer_if_enabled(workspace, &log_buffer)?;
        self.persist_extracted_xml_artifacts(workspace)?;
        self.write_monitor_warning_if_needed(&monitor)?;
        Ok(())
    }
}
