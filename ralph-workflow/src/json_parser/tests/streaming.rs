// Streaming behavior tests for JSON parsers.
//
// This module contains tests for streaming functionality, event classification,
// health monitoring, session management, and deduplication logic.

// Tests for format_unknown_json_event and event classification
include!("streaming/event_tests.rs");

// Tests for streaming session management and snapshot-as-delta detection
include!("streaming/session_tests.rs");

// End-to-end streaming integration tests
include!("streaming/integration_tests.rs");

// Tests for render deduplication, session-level deduplication, and delta-level deduplication
include!("streaming/dedup_render.rs");
include!("streaming/dedup_session.rs");
include!("streaming/delta_hash_dedup.rs");
include!("streaming/ccs_glm_scenarios.rs");
include!("streaming/consecutive_duplicate_detection.rs");
include!("streaming/result_error_suppression.rs");
