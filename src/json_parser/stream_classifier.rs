//! Stream event classifier for algorithmic detection of partial vs complete events.
//!
//! This module provides a classifier that can distinguish between different types
//! of streaming events without prior knowledge of the specific protocol. It uses
//! heuristics based on JSON structure and field names to make conservative decisions
//! about event classification.

use serde_json::Value;

/// Classification of a streaming event
///
/// Represents the nature of a streaming event to inform how it should be
/// processed and displayed to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEventType {
    /// Partial/delta content that should be accumulated
    ///
    /// These events contain incremental updates that need to be combined
    /// with other events to form complete content.
    Partial,

    /// Complete, self-contained content
    ///
    /// These events contain full content that can be displayed independently.
    Complete,

    /// Control/metadata event
    ///
    /// These events provide session information (start/stop) or metadata
    /// but don't contain user-facing content.
    Control,
}

/// Result of event classification
///
/// Contains the classification along with extracted metadata about the event.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// The classified event type
    pub event_type: StreamEventType,
    /// Detected event type name (e.g., "message", "delta", "error")
    pub type_name: Option<String>,
    /// The primary content field if found
    pub content_field: Option<String>,
    /// Whether this appears to be a streaming delta
    pub is_delta: bool,
}

/// Stream event classifier
///
/// Analyzes JSON events to determine if they represent partial content,
/// complete messages, or control events. Uses conservative heuristics to
/// prefer showing content over hiding it.
pub struct StreamEventClassifier {
    /// Threshold for considering text content "substantial" enough to be complete
    substantial_content_threshold: usize,
}

impl Default for StreamEventClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamEventClassifier {
    /// Create a new classifier with default settings
    pub fn new() -> Self {
        Self {
            substantial_content_threshold: 50,
        }
    }

    /// Create a new classifier with custom content threshold
    ///
    /// # Arguments
    /// * `threshold` - Minimum character count for text to be considered "substantial"
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            substantial_content_threshold: threshold,
        }
    }

    /// Classify a JSON event
    ///
    /// # Arguments
    /// * `value` - The parsed JSON value to classify
    ///
    /// # Returns
    /// A `ClassificationResult` with the detected event type and metadata
    pub fn classify(&self, value: &Value) -> ClassificationResult {
        // Extract the object if present
        let obj = match value.as_object() {
            Some(o) => o,
            None => {
                return ClassificationResult {
                    event_type: StreamEventType::Complete,
                    type_name: None,
                    content_field: None,
                    is_delta: false,
                }
            }
        };

        // Extract the type field
        let type_name = obj
            .get("type")
            .or_else(|| obj.get("event_type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Check for explicit delta flag
        let is_delta = obj
            .get("delta")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Check for control event patterns
        if self.is_control_event(&type_name, obj) {
            return ClassificationResult {
                event_type: StreamEventType::Control,
                type_name,
                content_field: None,
                is_delta,
            };
        }

        // Check for partial/delta indicators
        if self.is_partial_event(&type_name, obj, is_delta) {
            return ClassificationResult {
                event_type: StreamEventType::Partial,
                type_name,
                content_field: self.find_content_field(obj),
                is_delta: true,
            };
        }

        // Default to complete for unknown events
        // (prefer showing content over hiding it)
        ClassificationResult {
            event_type: StreamEventType::Complete,
            type_name,
            content_field: self.find_content_field(obj),
            is_delta,
        }
    }

    /// Check if an event is a control/metadata event
    fn is_control_event(&self, type_name: &Option<String>, obj: &serde_json::Map<String, Value>) -> bool {
        // Check type name for control patterns
        if let Some(name) = type_name {
            let control_patterns = [
                "start", "started", "init", "initialize",
                "stop", "stopped", "end", "done", "complete",
                "error", "fail", "failed", "failure",
                "ping", "pong", "heartbeat", "keepalive",
                "metadata", "meta",
            ];

            let name_lower = name.to_lowercase();
            for pattern in control_patterns {
                if name_lower.contains(pattern) {
                    return true;
                }
            }
        }

        // Check for status/error fields without content
        let has_status = obj.contains_key("status") || obj.contains_key("error");
        let has_content = self.has_content_field(obj);
        has_status && !has_content
    }

    /// Check if an event is a partial/delta event
    fn is_partial_event(&self, type_name: &Option<String>, obj: &serde_json::Map<String, Value>, explicit_delta: bool) -> bool {
        // Explicit delta flag
        if explicit_delta {
            return true;
        }

        // Check type name for partial patterns
        if let Some(name) = type_name {
            let partial_patterns = [
                "delta", "partial", "increment", "chunk",
                "progress", "streaming", "update",
            ];

            let name_lower = name.to_lowercase();
            for pattern in partial_patterns {
                if name_lower.contains(pattern) {
                    return true;
                }
            }
        }

        // Check for delta fields in the object
        let delta_fields = ["delta", "partial", "increment"];
        for field in delta_fields {
            if obj.contains_key(field) {
                return true;
            }
        }

        // Check for small content fragments that might be partial
        if let Some(content) = self.find_content_field(obj) {
            if let Some(text) = obj.get(&content).and_then(|v| v.as_str()) {
                // Short text fragments are likely partial
                if text.len() < self.substantial_content_threshold {
                    // But only consider it partial if it doesn't look like a complete message
                    let text_lower = text.to_lowercase();
                    let complete_indicators = [
                        ".", "!", "?", "\n\n",
                        "done", "finished", "complete",
                        "error:", "warning:",
                    ];
                    let has_complete_indicator = complete_indicators
                        .iter()
                        .any(|indicator| text_lower.contains(indicator));
                    return !has_complete_indicator;
                }
            }
        }

        false
    }

    /// Find the primary content field in an object
    fn find_content_field(&self, obj: &serde_json::Map<String, Value>) -> Option<String> {
        // Common content field names in priority order
        let content_fields = [
            "content", "text", "message", "data",
            "output", "result", "response", "body",
            "thinking", "reasoning", "delta",
        ];

        for field in content_fields {
            if obj.contains_key(field) {
                // Only consider it a content field if it has a string value
                if let Some(Value::String(_)) = obj.get(field) {
                    return Some(field.to_string());
                }
            }
        }

        None
    }

    /// Check if an object has any content field
    fn has_content_field(&self, obj: &serde_json::Map<String, Value>) -> bool {
        self.find_content_field(obj).is_some()
    }

    /// Extract text content from a partial event
    ///
    /// # Arguments
    /// * `obj` - The JSON object to extract content from
    ///
    /// # Returns
    /// The extracted text content, or None if no content found
    pub fn extract_content(&self, obj: &serde_json::Map<String, Value>) -> Option<String> {
        if let Some(field) = self.find_content_field(obj) {
            obj.get(&field)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        }
    }

    /// Extract text content from a JSON value
    ///
    /// # Arguments
    /// * `value` - The JSON value to extract content from
    ///
    /// # Returns
    /// The extracted text content, or None if no content found
    pub fn extract_content_from_value(&self, value: &Value) -> Option<String> {
        value
            .as_object()
            .and_then(|obj| self.extract_content(obj))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_classify_delta_event() {
        let classifier = StreamEventClassifier::new();
        let event = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": "Hello"}
        });

        let result = classifier.classify(&event);
        assert_eq!(result.event_type, StreamEventType::Partial);
        assert_eq!(result.is_delta, true);
    }

    #[test]
    fn test_classify_control_event() {
        let classifier = StreamEventClassifier::new();
        let event = json!({
            "type": "message_start",
            "message": {"id": "msg_123"}
        });

        let result = classifier.classify(&event);
        assert_eq!(result.event_type, StreamEventType::Control);
    }

    #[test]
    fn test_classify_complete_message() {
        let classifier = StreamEventClassifier::new();
        let event = json!({
            "type": "message",
            "content": "This is a complete message with substantial content that should be displayed as is."
        });

        let result = classifier.classify(&event);
        assert_eq!(result.event_type, StreamEventType::Complete);
        assert_eq!(result.is_delta, false);
    }

    #[test]
    fn test_classify_explicit_delta_flag() {
        let classifier = StreamEventClassifier::new();
        let event = json!({
            "type": "message",
            "delta": true,
            "content": "partial"
        });

        let result = classifier.classify(&event);
        assert_eq!(result.event_type, StreamEventType::Partial);
        assert_eq!(result.is_delta, true);
    }

    #[test]
    fn test_classify_error_event() {
        let classifier = StreamEventClassifier::new();
        let event = json!({
            "type": "error",
            "message": "Something went wrong"
        });

        let result = classifier.classify(&event);
        assert_eq!(result.event_type, StreamEventType::Control);
    }

    #[test]
    fn test_extract_content() {
        let classifier = StreamEventClassifier::new();
        let event = json!({
            "type": "message",
            "content": "Hello, world!"
        });

        let obj = event.as_object().unwrap();
        let content = classifier.extract_content(obj);
        assert_eq!(content, Some("Hello, world!".to_string()));
    }

    #[test]
    fn test_small_content_is_partial() {
        let classifier = StreamEventClassifier::new();
        let event = json!({
            "type": "chunk",
            "text": "Hi"
        });

        let result = classifier.classify(&event);
        assert_eq!(result.event_type, StreamEventType::Partial);
    }

    #[test]
    fn test_substantial_content_is_complete() {
        let classifier = StreamEventClassifier::new();
        let long_text = "This is a substantial message that exceeds the threshold and should be considered complete.".repeat(2);
        let event = json!({
            "type": "message",
            "content": long_text
        });

        let result = classifier.classify(&event);
        assert_eq!(result.event_type, StreamEventType::Complete);
    }
}
