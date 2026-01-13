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
    /// Number of JSON parse errors
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

    /// Record a parse error
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

    /// Check if the parser health is concerning (>50% ignored)
    pub fn is_concerning(&self) -> bool {
        self.total_events > 10 && self.ignored_percentage() > 50.0
    }

    /// Get a warning message if health is concerning
    pub fn warning(&self, parser_name: &str, colors: &Colors) -> Option<String> {
        if !self.is_concerning() {
            return None;
        }

        Some(format!(
            "{}[Parser Health Warning]{} {} parser ignored {:.1}% of events ({} of {}). \
             This may indicate a parser mismatch. Consider using a different json_parser in your agent config.",
            colors.yellow(),
            colors.reset(),
            parser_name,
            self.ignored_percentage(),
            self.ignored_events,
            self.total_events
        ))
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

    /// Record a parse error
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

        // Concerning with many ignored events
        for _ in 0..8 {
            health.record_ignored();
        }
        assert!(health.is_concerning()); // 11 total, >50% ignored

        // Not concerning when most are parsed
        let mut health2 = ParserHealth::new();
        for _ in 0..15 {
            health2.record_parsed();
        }
        for _ in 0..5 {
            health2.record_ignored();
        }
        assert!(!health2.is_concerning()); // 20 total, 25% ignored
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

        // Add enough ignored events to trigger warning
        for _ in 0..15 {
            monitor.record_ignored();
        }

        let warning1 = monitor.check_and_warn(&colors);
        assert!(warning1.is_some());

        let warning2 = monitor.check_and_warn(&colors);
        assert!(warning2.is_none()); // Already warned
    }
}
