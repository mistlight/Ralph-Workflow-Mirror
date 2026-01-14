//! Parser health monitoring and graceful degradation.
//!
//! This module provides utilities for monitoring parser health,
//! tracking parsed vs ignored events, and providing warnings when
//! parsers are not working correctly with specific agents.

use crate::colors::Colors;
use std::cell::Cell;

/// Parser health statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct ParserHealth {
    /// Total number of events processed
    pub total_events: u64,
    /// Number of events successfully parsed and displayed
    pub parsed_events: u64,
    /// Number of events ignored (malformed JSON, unknown events, etc.)
    pub ignored_events: u64,
    /// Number of unknown event types (valid JSON but unhandled)
    pub unknown_events: u64,
    /// Number of JSON parse errors (malformed JSON)
    pub parse_errors: u64,
}

impl ParserHealth {
    /// Create a new health tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a parsed event
    pub fn record_parsed(&mut self) {
        self.total_events += 1;
        self.parsed_events += 1;
    }

    /// Record an ignored event
    pub fn record_ignored(&mut self) {
        self.total_events += 1;
        self.ignored_events += 1;
    }

    /// Record an unknown event type (valid JSON but unhandled)
    ///
    /// Unknown events are valid JSON that the parser deserialized successfully
    /// but doesn't have specific handling for. These should not trigger health
    /// warnings as they represent future/new event types, not parser errors.
    pub fn record_unknown_event(&mut self) {
        self.total_events += 1;
        self.unknown_events += 1;
        self.ignored_events += 1;
    }

    /// Record a parse error (malformed JSON)
    pub fn record_parse_error(&mut self) {
        self.total_events += 1;
        self.parse_errors += 1;
        self.ignored_events += 1;
    }

    /// Get the percentage of ignored events
    pub fn ignored_percentage(&self) -> f64 {
        if self.total_events == 0 {
            return 0.0;
        }
        (self.ignored_events as f64 / self.total_events as f64) * 100.0
    }

    /// Get the percentage of parse errors (excluding unknown events)
    pub fn parse_error_percentage(&self) -> f64 {
        if self.total_events == 0 {
            return 0.0;
        }
        (self.parse_errors as f64 / self.total_events as f64) * 100.0
    }

    /// Check if the parser health is concerning
    ///
    /// Only returns true if there are actual parse errors (malformed JSON),
    /// not just unknown event types. Unknown events are valid JSON that we
    /// don't have specific handling for, which is not a health concern.
    pub fn is_concerning(&self) -> bool {
        self.total_events > 10 && self.parse_error_percentage() > 50.0
    }

    /// Get a warning message if health is concerning
    pub fn warning(&self, parser_name: &str, colors: &Colors) -> Option<String> {
        if !self.is_concerning() {
            return None;
        }

        let msg = if self.unknown_events > 0 {
            format!(
                "{}[Parser Health Warning]{} {} parser has {} parse errors ({}% of {} events). \
                 Also encountered {} unknown event types (valid JSON but unhandled). \
                 This may indicate a parser mismatch. Consider using a different json_parser in your agent config.",
                colors.yellow(),
                colors.reset(),
                parser_name,
                self.parse_errors,
                self.parse_error_percentage() as u32,
                self.total_events,
                self.unknown_events
            )
        } else {
            format!(
                "{}[Parser Health Warning]{} {} parser has {} parse errors ({}% of {} events). \
                 This may indicate malformed JSON output. Consider using a different json_parser in your agent config.",
                colors.yellow(),
                colors.reset(),
                parser_name,
                self.parse_errors,
                self.parse_error_percentage() as u32,
                self.total_events
            )
        };

        Some(msg)
    }
}

/// A wrapper that monitors parser health and provides graceful degradation
///
/// This wraps any parser function to track how many events are being ignored
/// and emit warnings when the parser seems to be misconfigured.
pub struct HealthMonitor {
    health: Cell<ParserHealth>,
    parser_name: &'static str,
    threshold_warned: Cell<bool>,
}

impl HealthMonitor {
    /// Create a new health monitor for a parser
    pub fn new(parser_name: &'static str) -> Self {
        Self {
            health: Cell::new(ParserHealth::new()),
            parser_name,
            threshold_warned: Cell::new(false),
        }
    }

    /// Record that an event was parsed successfully
    pub fn record_parsed(&self) {
        let mut h = self.health.get();
        h.record_parsed();
        self.health.set(h);
    }

    /// Record that an event was ignored
    pub fn record_ignored(&self) {
        let mut h = self.health.get();
        h.record_ignored();
        self.health.set(h);
    }

    /// Record an unknown event type (valid JSON but unhandled)
    pub fn record_unknown_event(&self) {
        let mut h = self.health.get();
        h.record_unknown_event();
        self.health.set(h);
    }

    /// Record a parse error (malformed JSON)
    pub fn record_parse_error(&self) {
        let mut h = self.health.get();
        h.record_parse_error();
        self.health.set(h);
    }

    /// Check if we should warn about parser health (only warn once)
    pub fn check_and_warn(&self, colors: &Colors) -> Option<String> {
        if self.threshold_warned.get() {
            return None;
        }

        let health = self.health.get();
        if let Some(warning) = health.warning(self.parser_name, colors) {
            self.threshold_warned.set(true);
            Some(warning)
        } else {
            None
        }
    }

    #[cfg(test)]
    pub fn health(&self) -> ParserHealth {
        self.health.get()
    }

    #[cfg(test)]
    pub fn reset(&self) {
        self.health.set(ParserHealth::new());
        self.threshold_warned.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_health_new() {
        let health = ParserHealth::new();
        assert_eq!(health.total_events, 0);
        assert_eq!(health.parsed_events, 0);
        assert_eq!(health.ignored_events, 0);
    }

    #[test]
    fn test_parser_health_record_parsed() {
        let mut health = ParserHealth::new();
        health.record_parsed();
        assert_eq!(health.total_events, 1);
        assert_eq!(health.parsed_events, 1);
        assert_eq!(health.ignored_events, 0);
    }

    #[test]
    fn test_parser_health_record_ignored() {
        let mut health = ParserHealth::new();
        health.record_ignored();
        assert_eq!(health.total_events, 1);
        assert_eq!(health.parsed_events, 0);
        assert_eq!(health.ignored_events, 1);
    }

    #[test]
    fn test_parser_health_ignored_percentage() {
        let mut health = ParserHealth::new();
        assert_eq!(health.ignored_percentage(), 0.0);

        for _ in 0..5 {
            health.record_parsed();
        }
        for _ in 0..5 {
            health.record_ignored();
        }
        assert_eq!(health.ignored_percentage(), 50.0);
    }

    #[test]
    fn test_parser_health_is_concerning() {
        let mut health = ParserHealth::new();
        // Not concerning with few events
        for _ in 0..3 {
            health.record_ignored();
        }
        assert!(!health.is_concerning());

        // Unknown events should NOT trigger concerning state (they're valid JSON)
        for _ in 0..20 {
            health.record_unknown_event();
        }
        assert!(!health.is_concerning()); // Even with many unknown events, not concerning

        // Only parse errors trigger concerning state
        let mut health2 = ParserHealth::new();
        for _ in 0..10 {
            health2.record_parsed();
        }
        for _ in 0..15 {
            health2.record_parse_error();
        }
        assert!(health2.is_concerning()); // 25 total, 60% parse errors

        // Not concerning when most are parsed or unknown (but few parse errors)
        let mut health3 = ParserHealth::new();
        for _ in 0..15 {
            health3.record_parsed();
        }
        for _ in 0..10 {
            health3.record_unknown_event();
        }
        for _ in 0..2 {
            health3.record_parse_error();
        }
        assert!(!health3.is_concerning()); // 27 total, only 7% parse errors
    }

    #[test]
    fn test_parser_health_unknown_events() {
        let mut health = ParserHealth::new();
        assert_eq!(health.unknown_events, 0);

        health.record_unknown_event();
        health.record_unknown_event();
        assert_eq!(health.unknown_events, 2);
        assert_eq!(health.ignored_events, 2); // unknown counts as ignored
        assert_eq!(health.parse_errors, 0); // but not as parse error

        // Unknown events don't make it concerning
        assert!(!health.is_concerning());
    }

    #[test]
    fn test_health_monitor() {
        let monitor = HealthMonitor::new("claude");

        monitor.record_parsed();
        monitor.record_parsed();
        monitor.record_ignored();

        let health = monitor.health();
        assert_eq!(health.total_events, 3);
        assert_eq!(health.parsed_events, 2);
        assert_eq!(health.ignored_events, 1);

        let colors = Colors { enabled: false };
        assert!(monitor.check_and_warn(&colors).is_none()); // Not concerning yet

        monitor.reset();
        assert_eq!(monitor.health().total_events, 0);
    }

    #[test]
    fn test_health_monitor_warns_once() {
        let monitor = HealthMonitor::new("test");
        let colors = Colors { enabled: false };

        // Add enough parse errors to trigger warning (unknown events shouldn't trigger)
        for _ in 0..15 {
            monitor.record_parse_error();
        }

        let warning1 = monitor.check_and_warn(&colors);
        assert!(warning1.is_some());

        let warning2 = monitor.check_and_warn(&colors);
        assert!(warning2.is_none()); // Already warned
    }

    #[test]
    fn test_health_monitor_many_unknown_no_warning() {
        let monitor = HealthMonitor::new("test");
        let colors = Colors { enabled: false };

        // Add many unknown events (simulating 97.5% unknown like the bug report)
        for _ in 0..2049 {
            monitor.record_unknown_event();
        }
        for _ in 0..53 {
            monitor.record_parsed();
        }

        let warning = monitor.check_and_warn(&colors);
        assert!(warning.is_none()); // Should NOT warn even with 97.5% unknown events
    }

    #[test]
    fn test_health_monitor_mixed_unknown_and_parse_errors() {
        let monitor = HealthMonitor::new("test");
        let colors = Colors { enabled: false };

        // Mix of unknown and parse errors - only parse errors count for warning
        for _ in 0..100 {
            monitor.record_unknown_event();
        }
        for _ in 0..20 {
            monitor.record_parse_error();
        }
        for _ in 0..20 {
            monitor.record_parsed();
        }

        // 140 total events, 20 parse errors = ~14% (not concerning)
        let warning = monitor.check_and_warn(&colors);
        assert!(warning.is_none());

        // Add more parse errors to trigger warning
        for _ in 0..30 {
            monitor.record_parse_error();
        }

        // 170 total events, 50 parse errors = ~29% (still not concerning)
        let warning = monitor.check_and_warn(&colors);
        assert!(warning.is_none());

        // Add even more parse errors
        for _ in 0..60 {
            monitor.record_parse_error();
        }

        // 230 total events, 110 parse errors = ~48% (close to threshold)
        let warning = monitor.check_and_warn(&colors);
        assert!(warning.is_none());

        // Push it over 50%
        for _ in 0..30 {
            monitor.record_parse_error();
        }

        // 260 total events, 140 parse errors = ~54% (concerning!)
        let warning = monitor.check_and_warn(&colors);
        assert!(warning.is_some());
    }

    #[test]
    fn test_parser_health_parse_error_percentage() {
        let mut health = ParserHealth::new();
        assert_eq!(health.parse_error_percentage(), 0.0);

        // Parse errors only
        for _ in 0..5 {
            health.record_parse_error();
        }
        assert_eq!(health.parse_error_percentage(), 100.0);

        // Add parsed events
        let mut health2 = ParserHealth::new();
        for _ in 0..5 {
            health2.record_parse_error();
        }
        for _ in 0..5 {
            health2.record_parsed();
        }
        assert_eq!(health2.parse_error_percentage(), 50.0);

        // Unknown events don't affect parse error percentage
        let mut health3 = ParserHealth::new();
        for _ in 0..5 {
            health3.record_parse_error();
        }
        for _ in 0..10 {
            health3.record_unknown_event();
        }
        for _ in 0..5 {
            health3.record_parsed();
        }
        // 20 total, 5 parse errors = 25%
        assert_eq!(health3.parse_error_percentage(), 25.0);
    }
}
