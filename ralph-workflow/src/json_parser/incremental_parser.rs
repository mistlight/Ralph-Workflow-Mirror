//! Incremental NDJSON parser for real-time streaming.
//!
//! This module provides a parser that can process NDJSON (newline-delimited JSON)
//! incrementally, yielding complete JSON objects as soon as they're received,
//! without waiting for newlines.
//!
//! # Why Incremental Parsing?
//!
//! The standard approach of using `reader.lines()` blocks until a newline is received.
//! For AI agents that buffer their output (like Codex), this causes all output to appear
//! at once instead of streaming character-by-character.
//!
//! This parser detects complete JSON objects by tracking brace nesting depth,
//! allowing true real-time streaming like `ChatGPT`.

/// Incremental NDJSON parser that yields complete JSON objects as they arrive.
///
/// # How It Works
///
/// The parser maintains a buffer of received bytes and tracks brace nesting depth.
/// When the depth returns to zero after seeing a closing brace, we have a complete
/// JSON object that can be parsed.
///
/// # Depth Limit
///
/// The parser enforces a maximum nesting depth to prevent integer overflow
/// from malicious input with extremely deep nesting. If the depth exceeds
/// this limit, parsing fails with an error.
///
/// # Example
///
/// ```ignore
/// let mut parser = IncrementalNdjsonParser::new();
///
/// // Feed first half of JSON
/// let events1 = parser.feed(b"{\"type\": \"de");
/// assert_eq!(events1.len(), 0);  // Not complete yet
///
/// // Feed second half
/// let events2 = parser.feed(b"lta\"}\n");
/// assert_eq!(events2.len(), 1);  // Complete!
/// assert_eq!(events2[0], "{\"type\": \"delta\"}");
/// ```
pub struct IncrementalNdjsonParser {
    /// Buffer of received bytes that haven't been parsed yet
    buffer: Vec<u8>,
    /// Current brace nesting depth (0 means top-level)
    depth: usize,
    /// Whether we're inside a string literal
    in_string: bool,
    /// Whether the next character is escaped
    escape_next: bool,
    /// Whether we've seen at least one opening brace (started parsing JSON)
    started: bool,
}

/// Maximum allowed nesting depth for JSON objects.
/// This prevents integer overflow from malicious input with extremely deep nesting.
/// Most well-formed JSON has nesting depth < 20, so 1000 is a conservative limit.
const MAX_JSON_DEPTH: usize = 1000;

impl IncrementalNdjsonParser {
    /// Create a new incremental NDJSON parser.
    pub const fn new() -> Self {
        Self {
            buffer: Vec::new(),
            depth: 0,
            in_string: false,
            escape_next: false,
            started: false,
        }
    }

    /// Feed bytes into the parser, returning any complete JSON objects found.
    ///
    /// This method processes the input bytes and extracts complete JSON objects.
    /// Multiple JSON objects may be returned from a single call if they're all complete.
    ///
    /// # Arguments
    ///
    /// * `data` - Bytes to feed into the parser
    ///
    /// # Returns
    ///
    /// A vector of complete JSON strings, in the order they were completed.
    pub fn feed(&mut self, data: &[u8]) -> Vec<String> {
        let mut complete_jsons = Vec::new();

        for &byte in data {
            self.process_byte(byte, &mut complete_jsons);
        }

        complete_jsons
    }

    /// Process a single byte, tracking state and extracting complete JSONs.
    ///
    /// If the depth exceeds `MAX_JSON_DEPTH`, the parser will reset to a safe
    /// state and skip the current JSON to prevent integer overflow from malicious input.
    fn process_byte(&mut self, byte: u8, complete_jsons: &mut Vec<String>) {
        // Handle escape sequences
        if self.escape_next {
            self.buffer.push(byte);
            self.escape_next = false;
            return;
        }

        match byte {
            b'\\' if self.in_string => {
                self.buffer.push(byte);
                self.escape_next = true;
            }
            b'"' => {
                self.buffer.push(byte);
                if self.started {
                    self.in_string = !self.in_string;
                }
            }
            b'{' if !self.in_string => {
                // Check for depth limit to prevent overflow BEFORE pushing
                if self.depth + 1 > MAX_JSON_DEPTH {
                    // Depth exceeded - reset parser state to skip this malformed JSON
                    self.buffer.clear();
                    self.depth = 0;
                    self.started = false;
                    self.in_string = false;
                    self.escape_next = false;
                } else {
                    self.buffer.push(byte);
                    self.depth += 1;
                    self.started = true;
                }
            }
            b'}' if !self.in_string && self.started => {
                self.buffer.push(byte);
                self.depth -= 1;

                // When depth returns to 0, we have a complete JSON object
                if self.depth == 0 {
                    self.extract_complete_json(complete_jsons);
                }
            }
            _ => {
                self.buffer.push(byte);
            }
        }
    }

    /// Extract a complete JSON object from the buffer.
    fn extract_complete_json(&mut self, complete_jsons: &mut Vec<String>) {
        // Find the end of the JSON (we know it's complete at this point)
        // Look for the closing brace that brought us to depth 0
        let json_end = self.buffer.len();

        // Convert to UTF-8 (should be valid JSON)
        let Ok(json_str) = String::from_utf8(self.buffer.drain(..json_end).collect()) else {
            // Invalid UTF-8 - skip this JSON silently
            self.started = false;
            return;
        };

        // Trim whitespace and check if it's non-empty
        let trimmed = json_str.trim();
        if !trimmed.is_empty() {
            complete_jsons.push(trimmed.to_string());
        }

        // Reset state for next JSON
        self.started = false;
    }

    /// Get any incomplete JSON currently in the buffer.
    ///
    /// This method is available in test builds for debugging purposes.
    /// It allows inspection of partial JSON data during parser development.
    #[must_use]
    #[cfg(test)]
    pub fn partial(&self) -> &[u8] {
        &self.buffer
    }

    /// Clear the internal buffer.
    ///
    /// This can be useful for error recovery when invalid data is encountered.
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.depth = 0;
        self.in_string = false;
        self.escape_next = false;
        self.started = false;
    }

    /// Check if the parser is currently inside a JSON object.
    #[must_use]
    #[cfg(test)]
    pub const fn is_parsing(&self) -> bool {
        self.started
    }
}

impl Default for IncrementalNdjsonParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incremental_parser_single_json() {
        let mut parser = IncrementalNdjsonParser::new();
        let events = parser.feed(b"{\"type\": \"delta\"}\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "{\"type\": \"delta\"}");
    }

    #[test]
    fn test_incremental_parser_split_json() {
        let mut parser = IncrementalNdjsonParser::new();

        // Feed first half
        let events1 = parser.feed(b"{\"type\": \"de");
        assert_eq!(events1.len(), 0);

        // Feed second half
        let events2 = parser.feed(b"lta\"}\n");
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0], "{\"type\": \"delta\"}");
    }

    #[test]
    fn test_incremental_parser_multiple_jsons() {
        let mut parser = IncrementalNdjsonParser::new();
        let input = b"{\"type\": \"delta\"}\n{\"type\": \"done\"}\n";
        let events = parser.feed(input);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "{\"type\": \"delta\"}");
        assert_eq!(events[1], "{\"type\": \"done\"}");
    }

    #[test]
    fn test_incremental_parser_nested_json() {
        let mut parser = IncrementalNdjsonParser::new();
        let input = b"{\"type\": \"delta\", \"data\": {\"nested\": true}}\n";
        let events = parser.feed(input);
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("\"nested\": true"));
    }

    #[test]
    fn test_incremental_parser_json_with_strings_containing_braces() {
        let mut parser = IncrementalNdjsonParser::new();
        let input = b"{\"text\": \"hello {world}\"}\n";
        let events = parser.feed(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "{\"text\": \"hello {world}\"}");
    }

    #[test]
    fn test_incremental_parser_json_with_escaped_quotes() {
        let mut parser = IncrementalNdjsonParser::new();
        let input = b"{\"text\": \"hello \\\"world\\\"\"}\n";
        let events = parser.feed(input);
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("\\\""));
    }

    #[test]
    fn test_incremental_parser_empty_input() {
        let mut parser = IncrementalNdjsonParser::new();
        let events = parser.feed(b"");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_incremental_parser_whitespace_only() {
        let mut parser = IncrementalNdjsonParser::new();
        let events = parser.feed(b"   \n  \n");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_incremental_parser_clear() {
        let mut parser = IncrementalNdjsonParser::new();

        // Feed incomplete JSON
        parser.feed(b"{\"type\":");
        assert!(parser.is_parsing());

        // Clear
        parser.clear();
        assert!(!parser.is_parsing());
        assert_eq!(parser.partial().len(), 0);

        // Should be able to parse new JSON
        let events = parser.feed(b"{\"type\": \"delta\"}\n");
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_incremental_parser_byte_by_byte() {
        let mut parser = IncrementalNdjsonParser::new();
        let input = b"{\"type\": \"delta\"}\n";

        let mut events = Vec::new();
        for byte in input {
            events.extend(parser.feed(&[*byte]));
        }

        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "{\"type\": \"delta\"}");
    }

    #[test]
    fn test_incremental_parser_multiline_json() {
        let mut parser = IncrementalNdjsonParser::new();
        let input = b"{\n  \"type\": \"delta\",\n  \"value\": 123\n}\n";
        let events = parser.feed(input);
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("\"type\": \"delta\""));
        assert!(events[0].contains("\"value\": 123"));
    }

    #[test]
    fn test_incremental_parser_depth_limit() {
        let mut parser = IncrementalNdjsonParser::new();
        // Create JSON with depth exceeding MAX_JSON_DEPTH.
        // We need exactly MAX_JSON_DEPTH + 1 opening braces to exceed the limit.
        let mut input = String::new();
        for _ in 0..=MAX_JSON_DEPTH {
            input.push('{');
        }
        // Feed the deeply nested input - should handle gracefully without panicking
        let events = parser.feed(input.as_bytes());
        // Parser should reset and return no events (skipped malformed JSON)
        assert_eq!(events.len(), 0);
        // Parser should be in a clean state after handling the error
        assert!(!parser.is_parsing());
        assert_eq!(parser.partial().len(), 0);
    }
}
