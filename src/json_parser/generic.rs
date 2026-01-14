//! Generic JSON parser using algorithmic event classification.
//!
//! This parser uses the `StreamEventClassifier` to detect partial vs complete
//! events without requiring prior knowledge of the specific JSON protocol.
//! It's designed as a fallback parser for unknown JSON sources that may
//! contain streaming delta content.
//!
//! # Features
//!
//! - Algorithmic detection of delta/partial vs complete vs control events
//! - Automatic extraction of content from common field patterns
//! - Graceful handling of unknown event types
//! - Health monitoring for detecting parser/agent mismatches
//!
//! # Usage
//!
//! This parser is ideal for:
//! - New agent types that output NDJSON with streaming content
//! - Protocols not specifically supported by dedicated parsers
//! - Testing and development with custom JSON formats

use crate::colors::Colors;
use crate::config::Verbosity;
use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::health::HealthMonitor;
use super::stream_classifier::StreamEventClassifier;
use super::types::{ContentType, DeltaAccumulator};

/// Generic JSON event parser using algorithmic classification
pub(crate) struct GenericParser {
    colors: Colors,
    verbosity: Verbosity,
    #[allow(dead_code)]
    log_file: Option<String>,
    display_name: String,
    /// Delta accumulator for streaming content
    delta_accumulator: Rc<RefCell<DeltaAccumulator>>,
    /// Stream event classifier for algorithmic event detection
    classifier: StreamEventClassifier,
}

impl GenericParser {
    #[allow(dead_code)]
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Generic".to_string(),
            delta_accumulator: Rc::new(RefCell::new(DeltaAccumulator::new())),
            classifier: StreamEventClassifier::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_display_name(mut self, display_name: &str) -> Self {
        self.display_name = display_name.to_string();
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_log_file(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    /// Parse and display a single JSON event using algorithmic classification
    ///
    /// This function attempts to:
    /// 1. Parse the JSON value
    /// 2. Classify the event (partial, complete, or control)
    /// 3. Extract and display content based on classification
    #[allow(dead_code)]
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let value = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => v,
            Err(_) => {
                // Non-JSON line - pass through as-is if meaningful
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('{') {
                    return Some(format!("{}\n", trimmed));
                }
                return None;
            }
        };

        let c = &self.colors;
        let name = &self.display_name;

        // Use stream classifier for algorithmic event detection
        let classification = self.classifier.classify(&value);

        let obj = match value.as_object() {
            Some(o) => o,
            None => {
                // Non-object JSON - just show the value in debug mode
                if self.verbosity.is_verbose() {
                    return Some(format!(
                        "{}[{}]{} {}\n",
                        c.dim(),
                        name,
                        c.reset(),
                        value
                    ));
                }
                return None;
            }
        };

        // Extract type field for display
        let event_type = obj
            .get("type")
            .or_else(|| obj.get("event_type"))
            .and_then(|v| v.as_str())
            .or_else(|| classification.type_name.as_deref())
            .unwrap_or("unknown");

        let output = match classification.event_type {
            super::stream_classifier::StreamEventType::Partial => {
                // Extract content from partial events for real-time display
                let content = if let Some(ref field) = classification.content_field {
                    obj.get(field)
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            // Field exists but isn't a string - try nested extraction
                            obj.get(field).and_then(|v| extract_content_from_value(v))
                        })
                } else {
                    // No content field detected - try common patterns
                    obj.get("delta")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            // Try nested delta.text
                            obj.get("delta")
                                .and_then(|d| d.as_object())
                                .and_then(|o| o.get("text"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .or_else(|| {
                            // Try other common fields
                            obj.get("text")
                                .or_else(|| obj.get("content"))
                                .or_else(|| obj.get("message"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                };

                if let Some(text) = content {
                    if !text.trim().is_empty() {
                        // Accumulate for completion events
                        {
                            let mut acc = self.delta_accumulator.borrow_mut();
                            acc.add_delta(ContentType::Text, "main", &text);
                        }
                        // Show delta in real-time (both verbose and normal mode)
                        return Some(format!("{}\n", text));
                    }
                }

                // Partial event with no extractable content
                if self.verbosity.is_verbose() {
                    format!(
                        "{}[{}]{} Partial event: {}\n",
                        c.dim(),
                        name,
                        c.reset(),
                        event_type
                    )
                } else {
                    String::new()
                }
            }
            super::stream_classifier::StreamEventType::Complete => {
                // Show complete content
                let content = if let Some(ref field) = classification.content_field {
                    obj.get(field)
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            obj.get(field).and_then(|v| extract_content_from_value(v))
                        })
                } else {
                    // Try common content fields
                    obj.get("text")
                        .or_else(|| obj.get("content"))
                        .or_else(|| obj.get("message"))
                        .or_else(|| obj.get("output"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                };

                if let Some(text) = content {
                    if !text.trim().is_empty() {
                        return Some(format!("{}\n", text));
                    }
                }

                // Complete event with no content - show in verbose mode
                if self.verbosity.is_verbose() {
                    format!(
                        "{}[{}]{} Complete event: {}\n",
                        c.dim(),
                        name,
                        c.reset(),
                        event_type
                    )
                } else {
                    String::new()
                }
            }
            super::stream_classifier::StreamEventType::Control => {
                // Control events don't produce user-facing output
                if self.verbosity.is_verbose() {
                    format!(
                        "{}[{}]{} Control event: {}\n",
                        c.dim(),
                        name,
                        c.reset(),
                        event_type
                    )
                } else {
                    String::new()
                }
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Parse a stream of generic JSON events
    #[allow(dead_code)]
    pub(crate) fn parse_stream<R: BufRead, W: Write>(
        &self,
        reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        let c = &self.colors;
        let monitor = HealthMonitor::new("Generic");
        let mut log_writer = self.log_file.as_ref().and_then(|log_path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .ok()
                .map(std::io::BufWriter::new)
        });

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with('{')
                && serde_json::from_str::<serde_json::Value>(trimmed).is_err()
            {
                monitor.record_parse_error();
                continue;
            }

            if self.verbosity.is_debug() {
                writeln!(
                    writer,
                    "{}[DEBUG]{} {}{}{}",
                    c.dim(),
                    c.reset(),
                    c.dim(),
                    &line,
                    c.reset()
                )?;
            }

            match self.parse_event(&line) {
                Some(output) => {
                    monitor.record_parsed();
                    write!(writer, "{}", output)?;
                }
                None => {
                    // Check if this was a control event using the classifier
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        let classification = self.classifier.classify(&value);
                        match classification.event_type {
                            super::stream_classifier::StreamEventType::Control => {
                                monitor.record_control_event();
                            }
                            _ => {
                                // Valid JSON but not a control event - track as unknown
                                monitor.record_unknown_event();
                            }
                        }
                    } else if trimmed.starts_with('{') {
                        monitor.record_unknown_event();
                    } else {
                        monitor.record_ignored();
                    }
                }
            }

            if let Some(ref mut file) = log_writer {
                writeln!(file, "{}", line)?;
            }
        }

        if let Some(ref mut file) = log_writer {
            file.flush()?;
        }
        if let Some(warning) = monitor.check_and_warn(c) {
            writeln!(writer, "{}", warning)?;
        }
        Ok(())
    }
}

/// Maximum recursion depth for content extraction to prevent stack overflow
const MAX_EXTRACTION_DEPTH: usize = 32;

/// Helper function to extract text content from a JSON value
///
/// This function recursively searches through JSON structures to find text content.
/// Depth limiting prevents stack overflow on malicious or malformed input.
fn extract_content_from_value(value: &serde_json::Value) -> Option<String> {
    extract_content_from_value_with_depth(value, 0)
}

/// Recursive helper with depth limiting
fn extract_content_from_value_with_depth(value: &serde_json::Value, depth: usize) -> Option<String> {
    // Prevent stack overflow by limiting recursion depth
    if depth >= MAX_EXTRACTION_DEPTH {
        return None;
    }

    // If it's a string, return it
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }

    // If it's an object, look for content fields recursively
    if let Some(obj) = value.as_object() {
        // First try common content field names at this level
        for field in ["text", "content", "message", "output", "result"] {
            if let Some(val) = obj.get(field) {
                // Try to extract from the value (which might be nested)
                if let Some(text) = extract_content_from_value_with_depth(val, depth + 1) {
                    return Some(text);
                }
            }
        }

        // If no content field found, try the first nested object that might contain text
        for (_key, val) in obj.iter() {
            if val.is_object() || val.is_array() {
                if let Some(text) = extract_content_from_value_with_depth(val, depth + 1) {
                    return Some(text);
                }
            }
        }
    }

    // If it's an array, search for content in items
    if let Some(arr) = value.as_array() {
        for item in arr {
            if let Some(text) = extract_content_from_value_with_depth(item, depth + 1) {
                return Some(text);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generic_parser_partial_delta() {
        let parser = GenericParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Hello"));
    }

    #[test]
    fn test_generic_parser_complete_message() {
        let parser = GenericParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"message","content":"This is a complete message."}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("This is a complete message."));
    }

    #[test]
    fn test_generic_parser_control_event_suppressed() {
        let parser = GenericParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"message_start","message":{"id":"msg_123"}}"#;
        let output = parser.parse_event(json);
        // Control events should return empty string in normal mode
        assert!(output.is_none());
    }

    #[test]
    fn test_generic_parser_control_event_shown_in_verbose() {
        let parser = GenericParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"message_start","message":{"id":"msg_123"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Control event"));
    }

    #[test]
    fn test_generic_parser_nested_delta_text() {
        let parser = GenericParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"delta","delta":{"text":"World"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("World"));
    }

    #[test]
    fn test_generic_parser_non_json_passthrough() {
        let parser = GenericParser::new(Colors { enabled: false }, Verbosity::Normal);
        let output = parser.parse_event("Error: something went wrong");
        assert!(output.is_some());
        assert!(output.unwrap().contains("Error: something went wrong"));
    }

    #[test]
    fn test_generic_parser_malformed_json_ignored() {
        let parser = GenericParser::new(Colors { enabled: false }, Verbosity::Normal);
        let output = parser.parse_event("{invalid json}");
        assert!(output.is_none());
    }

    #[test]
    fn test_extract_content_from_value_string() {
        let value = serde_json::json!("Hello world");
        assert_eq!(
            extract_content_from_value(&value),
            Some("Hello world".to_string())
        );
    }

    #[test]
    fn test_extract_content_from_value_object() {
        let value = serde_json::json!({"text": "content here", "other": "ignored"});
        assert_eq!(
            extract_content_from_value(&value),
            Some("content here".to_string())
        );
    }

    #[test]
    fn test_extract_content_from_value_nested() {
        let value = serde_json::json!({"delta": {"text": "nested content"}});
        assert_eq!(
            extract_content_from_value(&value),
            Some("nested content".to_string())
        );
    }

    #[test]
    fn test_extract_content_from_value_array() {
        let value = serde_json::json!([{"text": "first"}, {"content": "second"}]);
        assert_eq!(
            extract_content_from_value(&value),
            Some("first".to_string())
        );
    }
}
