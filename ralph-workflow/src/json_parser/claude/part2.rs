// Constructor methods and parse_event.

impl ClaudeParser {
    // Create a new `ClaudeParser` with the given colors and verbosity.
    pub fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self::with_printer(colors, verbosity, super::printer::shared_stdout())
    }

    // Create a new `ClaudeParser` with a custom printer.
    pub fn with_printer(colors: Colors, verbosity: Verbosity, printer: SharedPrinter) -> Self {
        let verbose_warnings = matches!(verbosity, Verbosity::Debug);
        let streaming_session = StreamingSession::new().with_verbose_warnings(verbose_warnings);

        // Use the printer's is_terminal method to validate it's connected correctly
        // This is a sanity check that also satisfies the compiler that the method is used
        let _printer_is_terminal = printer.borrow().is_terminal();

        Self {
            colors,
            verbosity,
            log_path: None,
            display_name: "Claude".to_string(),
            streaming_session: Rc::new(RefCell::new(streaming_session)),
            terminal_mode: RefCell::new(TerminalMode::detect()),
            show_streaming_metrics: false,
            printer,
        }
    }

    pub(crate) const fn with_show_streaming_metrics(mut self, show: bool) -> Self {
        self.show_streaming_metrics = show;
        self
    }

    // Set the display name for this parser.
    pub fn with_display_name(mut self, display_name: &str) -> Self {
        self.display_name = display_name.to_string();
        self
    }

    pub(crate) fn with_log_file(mut self, path: &str) -> Self {
        self.log_path = Some(std::path::PathBuf::from(path));
        self
    }

    // Set the terminal mode for this parser.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_terminal_mode(self, mode: TerminalMode) -> Self {
        *self.terminal_mode.borrow_mut() = mode;
        self
    }

    // Get a clone of the printer used by this parser.
    // This is primarily useful for testing and monitoring.
    // Only available with the `test-utils` feature.
    #[cfg(feature = "test-utils")]
    pub fn printer(&self) -> SharedPrinter {
        Rc::clone(&self.printer)
    }

    // Get streaming quality metrics from the current session.
    // Only available with the `test-utils` feature.
    #[cfg(feature = "test-utils")]
    pub fn streaming_metrics(&self) -> StreamingQualityMetrics {
        self.streaming_session
            .borrow()
            .get_streaming_quality_metrics()
    }

    // Parse and display a single Claude JSON event
    //
    // Returns `Some(formatted_output)` for valid events, or None for:
    // - Malformed JSON (logged at debug level)
    // - Unknown event types
    // - Empty or whitespace-only output
    pub fn parse_event(&self, line: &str) -> Option<String> {
        let event: ClaudeEvent = if let Ok(e) = serde_json::from_str(line) {
            e
        } else {
            // Non-JSON line - could be raw text output from agent
            // Pass through as-is if it looks like real output (not empty)
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('{') {
                return Some(format!("{trimmed}\n"));
            }
            return None;
        };
        let c = &self.colors;
        let prefix = &self.display_name;

        let output = match event {
            ClaudeEvent::System {
                subtype,
                session_id,
                cwd,
            } => self.format_system_event(subtype.as_ref(), session_id, cwd),
            ClaudeEvent::Assistant { message } => self.format_assistant_event(message),
            ClaudeEvent::User { message } => self.format_user_event(message),
            ClaudeEvent::Result {
                subtype,
                duration_ms,
                total_cost_usd,
                num_turns,
                result,
                error,
            } => self.format_result_event(
                subtype,
                duration_ms,
                total_cost_usd,
                num_turns,
                result,
                error,
            ),
            ClaudeEvent::StreamEvent { event } => {
                // Handle streaming events for delta/partial updates
                self.parse_stream_event(event)
            }
            ClaudeEvent::Unknown => {
                // Use the generic unknown event formatter for consistent handling
                // In verbose mode, this will show the event type and key fields
                // In normal mode, this returns empty string
                format_unknown_json_event(line, prefix, *c, self.verbosity.is_verbose())
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    // Parse a streaming event for delta/partial updates
    fn parse_stream_event(&self, event: StreamInnerEvent) -> String {
        let mut session = self.streaming_session.borrow_mut();

        match event {
            StreamInnerEvent::MessageStart {
                message,
                message_id,
            } => {
                // Extract message_id from either the top-level field or nested message.id
                // The Claude API typically puts the ID in message.id, not at the top level
                let effective_message_id =
                    message_id.or_else(|| message.as_ref().and_then(|m| m.id.clone()));
                // Set message ID for tracking and clear session state on new message
                session.set_current_message_id(effective_message_id);
                session.on_message_start();
                String::new()
            }
            StreamInnerEvent::ContentBlockStart {
                index: Some(index),
                content_block: Some(block),
            } => {
                // Initialize a new content block at this index
                session.on_content_block_start(index);
                match &block {
                    ContentBlock::Text { text: Some(t) } if !t.is_empty() => {
                        // Initial text in ContentBlockStart - treat as first delta
                        session.on_text_delta(index, t);
                    }
                    ContentBlock::ToolUse { name, input } => {
                        // Track tool name for GLM/CCS deduplication.
                        // IMPORTANT: Track the tool name when provided, even when input is None.
                        // GLM may send ContentBlockStart with name but no input, then send input via delta.
                        // We only store when we have a name to avoid overwriting a previous tool name with None.
                        if let Some(n) = name {
                            session.set_tool_name(index, Some(n.clone()));
                        }

                        // Initialize tool input accumulator only if input is present
                        if let Some(i) = input {
                            let input_str = if let serde_json::Value::String(s) = &i {
                                s.clone()
                            } else {
                                format_tool_input(i)
                            };
                            session.on_tool_input_delta(index, &input_str);
                        }
                    }
                    _ => {}
                }
                String::new()
            }
            StreamInnerEvent::ContentBlockStart {
                index: Some(index),
                content_block: None,
            } => {
                // Content block started but no initial content provided
                session.on_content_block_start(index);
                String::new()
            }
            StreamInnerEvent::ContentBlockStart { .. } => {
                // Content block without index - ignore
                String::new()
            }
            StreamInnerEvent::ContentBlockDelta {
                index: Some(index),
                delta: Some(delta),
            } => self.handle_content_block_delta(&mut session, index, delta),
            StreamInnerEvent::TextDelta { text: Some(text) } => {
                self.handle_text_delta(&mut session, &text)
            }
            StreamInnerEvent::ContentBlockStop { .. } => {
                // Content block completion event - no output needed
                // This event marks the end of a content block but doesn't produce
                // any displayable content. It's a control event for state management.
                String::new()
            }
            StreamInnerEvent::MessageDelta { .. } => {
                // Message delta event with usage/metadata - no output needed
                // This event contains final message metadata (stop_reason, usage stats)
                // but is used for tracking/monitoring purposes only, not display.
                String::new()
            }
            StreamInnerEvent::ContentBlockDelta { .. }
            | StreamInnerEvent::Ping
            | StreamInnerEvent::TextDelta { text: None }
            | StreamInnerEvent::Error { error: None } => String::new(),
            StreamInnerEvent::MessageStop => self.handle_message_stop(&mut session),
            StreamInnerEvent::Error {
                error: Some(err), ..
            } => self.handle_error_event(err),
            StreamInnerEvent::Unknown => self.handle_unknown_event(),
        }
    }
}
