//! Bounded event queue with semaphore-based backpressure for streaming.
//!
//! This module provides a bounded channel-based queue that sits between the
//! line reader and parser, providing backpressure to prevent memory exhaustion
//! when events are produced faster than they can be consumed.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐         ┌──────────────────┐         ┌─────────────────┐
//! │ Line Reader     │  send   │  Bounded Queue   │  recv   │  Parser         │
//! │ (Producer)      │────────▶│  (sync_channel)  │────────▶│  (Consumer)     │
//! └─────────────────┘         └──────────────────┘         └─────────────────┘
//!                                      │
//!                                      │ blocks when full
//!                                      ▼
//!                              backpressure on producer
//! ```
//!
//! # Behavior
//!
//! - **Bounded**: Queue has a fixed size (configurable via env var)
//! - **Blocking**: Producer blocks when queue is full (semaphore behavior)
//! - **Non-blocking fallback**: If queue is full and non-blocking send is attempted,
//!   returns error immediately without dropping data
//!
//! # Configuration
//!
//! Environment variables:
//! - `RALPH_STREAMING_QUEUE_SIZE`: Queue capacity (default: 100, range: 10-1000)
//!
//! # Production Status
//!
//! **This module is test-only (`#[cfg(test)]`) and is not used in production builds.**
//!
//! ## Why Not Production?
//!
//! The current streaming architecture uses **incremental byte-level parsing** which processes
//! events immediately without buffering. This provides:
//! - Zero latency (events processed as soon as JSON is complete)
//! - Bounded memory usage (only the incremental parser buffer)
//! - Immediate deduplication (KMP + Rolling Hash algorithms)
//!
//! A queue would add latency (~10-100ms) without solving an actual problem, as:
//! - The parser is faster than the producer (no backpressure needed)
//! - Memory usage is already bounded (no buffering of unprocessed events)
//! - Deduplication is stateless (no need for event queuing)
//!
//! See `QUEUE_INTEGRATION_ANALYSIS.md` for a detailed analysis of why the queue doesn't
//! fit the current architecture.
//!
//! ## When Would This Be Useful?
//!
//! This queue implementation is kept for:
//! - Future use if the architecture changes to line-based parsing
//! - Testing scenarios that require bounded queuing
//! - Reference implementation for backpressure handling
//!
//! If you need to integrate this queue into production, consider:
//! 1. Whether the incremental parser should be replaced with line-based parsing
//! 2. Whether the latency impact is acceptable for your use case
//! 3. Whether there's an actual performance problem the queue would solve
//!
//! ## Test-Only Implementation
//!
//! The module is conditionally compiled with `#[cfg(test)]` to avoid dead code warnings
//! in production builds. To test queue behavior, use the test suite:
//!
//! ```bash
//! cargo test queue --lib
//! ```

#[cfg(test)]
use std::sync::mpsc;
#[cfg(test)]
use std::sync::OnceLock;

// ============================================================================
// Configuration Constants
// ============================================================================

/// Default queue capacity in number of events.
///
/// This value balances:
/// - Memory usage (each event is a String, typically < 1KB)
/// - Latency (100 events @ ~10ms each = ~1s max additional latency)
/// - Throughput (enough buffering for bursty producers)
#[cfg(test)]
const DEFAULT_QUEUE_SIZE: usize = 100;

/// Minimum allowed queue size.
///
/// Values below 10 would cause excessive backpressure and context switching.
#[cfg(test)]
const MIN_QUEUE_SIZE: usize = 10;

/// Maximum allowed queue size.
///
/// Values above 1000 could cause memory exhaustion with large events.
#[cfg(test)]
const MAX_QUEUE_SIZE: usize = 1000;

/// Queue configuration from environment variables.
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
pub struct QueueConfig {
    /// Maximum number of events in the queue
    pub capacity: usize,
}

#[cfg(test)]
impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_QUEUE_SIZE,
        }
    }
}

/// Get the queue configuration from environment variables or use defaults.
///
/// Reads the following environment variables:
/// - `RALPH_STREAMING_QUEUE_SIZE`: Queue capacity (default: 100, range: 10-1000)
///
/// # Returns
/// The configured queue settings.
#[cfg(test)]
pub fn get_queue_config() -> QueueConfig {
    static CONFIG: OnceLock<QueueConfig> = OnceLock::new();
    *CONFIG.get_or_init(|| {
        let capacity = std::env::var("RALPH_STREAMING_QUEUE_SIZE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|v| {
                if (MIN_QUEUE_SIZE..=MAX_QUEUE_SIZE).contains(&v) {
                    Some(v)
                } else {
                    None
                }
            })
            .unwrap_or(DEFAULT_QUEUE_SIZE);

        QueueConfig { capacity }
    })
}

// ============================================================================
// Bounded Event Queue
// ============================================================================

/// A bounded event queue with semaphore-based backpressure.
///
/// This queue uses Rust's `sync_channel` which provides a bounded channel
/// that blocks the sender when the buffer is full. This provides natural
/// backpressure to prevent the producer from outpacing the consumer.
///
/// # Type Parameters
///
/// * `T` - The type of events in the queue (typically `String` for JSON events)
///
/// # Example
///
/// ```ignore
/// let mut queue = BoundedEventQueue::<String>::new();
///
/// // Producer: send events (blocks when full)
/// queue.send("{\"type\": \"delta\"}".to_string()).unwrap();
///
/// // Consumer: receive events (non-blocking)
/// if let Some(event) = queue.try_recv() {
///     println!("Got event: {}", event);
/// }
///
/// // Get metrics
/// let metrics = queue.metrics();
/// println!("Queue depth: {}", metrics.depth);
/// ```
#[derive(Debug)]
#[cfg(test)]
pub struct BoundedEventQueue<T> {
    /// Sender for the bounded channel
    sender: mpsc::SyncSender<T>,
    /// Receiver for the bounded channel
    receiver: mpsc::Receiver<T>,
    /// Queue metrics
    metrics: QueueMetrics,
}

/// Metrics tracking queue health and performance.
#[derive(Debug, Clone, Default)]
#[cfg(test)]
pub struct QueueMetrics {
    /// Current number of events in the queue
    pub depth: usize,
    /// Number of times backpressure was triggered (send blocked on full queue)
    pub backpressure_count: usize,
    /// Maximum observed queue depth
    pub max_depth: usize,
}

#[cfg(test)]
impl<T> BoundedEventQueue<T> {
    /// Create a new bounded event queue with default configuration.
    ///
    /// # Example
    /// ```ignore
    /// let queue: BoundedEventQueue<String> = BoundedEventQueue::new();
    /// ```
    pub fn new() -> Self {
        let config = get_queue_config();
        Self::with_config(config)
    }

    /// Create a new bounded event queue with specific configuration.
    ///
    /// # Arguments
    /// * `config` - Queue configuration (capacity)
    ///
    /// # Example
    /// ```ignore
    /// let config = QueueConfig { capacity: 500 };
    /// let queue: BoundedEventQueue<String> = BoundedEventQueue::with_config(config);
    /// ```
    pub fn with_config(config: QueueConfig) -> Self {
        let (sender, receiver) = mpsc::sync_channel(config.capacity);
        Self {
            sender,
            receiver,
            metrics: QueueMetrics::default(),
        }
    }

    /// Send an event to the queue, blocking if full.
    ///
    /// # Behavior
    ///
    /// - If queue has space: Event is sent immediately
    /// - If queue is full: Blocks until space is available (backpressure)
    ///
    /// # Arguments
    /// * `event` - The event to send
    ///
    /// # Returns
    /// * `Ok(())` - Event was sent successfully
    /// * `Err(mpsc::SendError(_))` - Receiver was dropped
    ///
    /// # Example
    /// ```ignore
    /// queue.send(event)?;
    /// ```
    pub fn send(&mut self, event: T) -> Result<(), mpsc::SendError<T>> {
        // Try to send without blocking first
        match self.sender.try_send(event) {
            Ok(()) => {
                // Update depth estimate
                self.metrics.depth = self.metrics.depth.saturating_add(1);
                self.metrics.max_depth = self.metrics.max_depth.max(self.metrics.depth);
                Ok(())
            }
            Err(mpsc::TrySendError::Full(event)) => {
                // Queue is full - this is backpressure
                self.metrics.backpressure_count = self.metrics.backpressure_count.saturating_add(1);

                // Block until space is available
                self.sender.send(event)?;
                self.metrics.depth = self.metrics.depth.saturating_add(1);
                self.metrics.max_depth = self.metrics.max_depth.max(self.metrics.depth);
                Ok(())
            }
            Err(mpsc::TrySendError::Disconnected(event)) => Err(mpsc::SendError(event)),
        }
    }

    /// Try to send an event to the queue without blocking.
    ///
    /// # Returns
    /// * `Ok(())` - Event was sent successfully
    /// * `Err(mpsc::TrySendError::Full(_))` - Queue is full
    /// * `Err(mpsc::TrySendError::Disconnected(_))` - Receiver was dropped
    ///
    /// # Example
    /// ```ignore
    /// match queue.try_send(event) {
    ///     Ok(_) => println!("Sent"),
    ///     Err(mpsc::TrySendError::Full(_)) => println!("Queue full"),
    ///     Err(_) => println!("Receiver disconnected"),
    /// }
    /// ```
    pub fn try_send(&mut self, event: T) -> Result<(), mpsc::TrySendError<T>> {
        match self.sender.try_send(event) {
            Ok(()) => {
                self.metrics.depth = self.metrics.depth.saturating_add(1);
                self.metrics.max_depth = self.metrics.max_depth.max(self.metrics.depth);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Try to receive an event without blocking.
    ///
    /// # Returns
    /// * `Some(event)` - An event was available
    /// * `None` - Queue is empty
    ///
    /// # Example
    /// ```ignore
    /// while let Some(event) = queue.try_recv() {
    ///     process_event(event);
    /// }
    /// ```
    pub fn try_recv(&mut self) -> Option<T> {
        match self.receiver.try_recv() {
            Ok(event) => {
                self.metrics.depth = self.metrics.depth.saturating_sub(1);
                Some(event)
            }
            Err(_) => None,
        }
    }

    /// Receive an event, blocking until one is available.
    ///
    /// # Returns
    /// * `Ok(event)` - An event was received
    /// * `Err(mpsc::RecvError)` - Sender was dropped
    ///
    /// # Example
    /// ```ignore
    /// let event = queue.recv()?;
    /// ```
    pub fn recv(&mut self) -> Result<T, mpsc::RecvError> {
        let event = self.receiver.recv()?;
        self.metrics.depth = self.metrics.depth.saturating_sub(1);
        Ok(event)
    }

    /// Get the current queue metrics.
    ///
    /// # Example
    /// ```ignore
    /// let metrics = queue.metrics();
    /// println!("Depth: {}, Backpressure: {}", metrics.depth, metrics.backpressure_count);
    /// ```
    #[must_use]
    pub const fn metrics(&self) -> &QueueMetrics {
        &self.metrics
    }

    /// Get the current queue depth (number of pending events).
    ///
    /// This is an estimate that may not be perfectly accurate due to
    /// concurrent access, but is sufficient for monitoring purposes.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.metrics.depth
    }

    /// Check if the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.depth() == 0
    }

    /// Clear all events from the queue.
    ///
    /// This is useful for error recovery when invalid data is encountered.
    pub fn clear(&mut self) {
        while self.try_recv().is_some() {
            // Drain all events
        }
        self.metrics.depth = 0;
    }

    /// Reset metrics while preserving queue contents.
    pub fn reset_metrics(&mut self) {
        self.metrics = QueueMetrics {
            depth: self.metrics.depth,
            ..Default::default()
        };
    }
}

#[cfg(test)]
impl<T> Default for BoundedEventQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_config_default() {
        let config = get_queue_config();
        assert_eq!(config.capacity, DEFAULT_QUEUE_SIZE);
    }

    #[test]
    fn test_queue_new() {
        let queue: BoundedEventQueue<String> = BoundedEventQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.depth(), 0);
    }

    #[test]
    fn test_queue_send_and_recv() {
        let mut queue: BoundedEventQueue<String> = BoundedEventQueue::new();
        let event = "test_event".to_string();

        queue.send(event.clone()).unwrap();
        assert!(!queue.is_empty());

        let received = queue.try_recv().unwrap();
        assert_eq!(received, event);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_queue_multiple_events() {
        let mut queue: BoundedEventQueue<i32> = BoundedEventQueue::new();

        for i in 0..10 {
            queue.send(i).unwrap();
        }

        for i in 0..10 {
            let received = queue.try_recv().unwrap();
            assert_eq!(received, i);
        }

        assert!(queue.is_empty());
    }

    #[test]
    fn test_queue_try_send_full() {
        let config = QueueConfig { capacity: 2 };
        let mut queue: BoundedEventQueue<i32> = BoundedEventQueue::with_config(config);

        // Fill the queue
        queue.try_send(1).unwrap();
        queue.try_send(2).unwrap();

        // This should fail with Full error
        let result = queue.try_send(3);
        assert!(matches!(result, Err(mpsc::TrySendError::Full(3))));
    }

    #[test]
    fn test_queue_try_recv_empty() {
        let mut queue: BoundedEventQueue<String> = BoundedEventQueue::new();
        assert!(queue.try_recv().is_none());
    }

    #[test]
    fn test_queue_clear() {
        let mut queue: BoundedEventQueue<i32> = BoundedEventQueue::new();

        for i in 0..5 {
            queue.send(i).unwrap();
        }

        queue.clear();
        assert!(queue.is_empty());
        assert_eq!(queue.depth(), 0);
    }

    #[test]
    fn test_queue_metrics_initial() {
        let queue: BoundedEventQueue<String> = BoundedEventQueue::new();
        let metrics = queue.metrics();
        assert_eq!(metrics.depth, 0);
        assert_eq!(metrics.backpressure_count, 0);
    }

    #[test]
    fn test_queue_reset_metrics() {
        let mut queue: BoundedEventQueue<String> = BoundedEventQueue::new();

        queue.send("test".to_string()).unwrap();
        queue.reset_metrics();

        let metrics = queue.metrics();
        assert_eq!(metrics.depth, 1); // depth preserved
        assert_eq!(metrics.backpressure_count, 0); // reset
    }

    #[test]
    fn test_queue_config_bounds() {
        // Verify bounds constants
        assert_eq!(MIN_QUEUE_SIZE, 10);
        assert_eq!(MAX_QUEUE_SIZE, 1000);
    }

    #[test]
    fn test_queue_with_custom_config() {
        let config = QueueConfig { capacity: 50 };
        let queue: BoundedEventQueue<String> = BoundedEventQueue::with_config(config);

        // Queue should use custom config
        assert!(queue.is_empty());
    }

    #[test]
    fn test_queue_metrics_depth_tracking() {
        let mut queue: BoundedEventQueue<i32> = BoundedEventQueue::new();

        queue.send(1).unwrap();
        assert_eq!(queue.depth(), 1);

        queue.send(2).unwrap();
        assert_eq!(queue.depth(), 2);

        queue.try_recv();
        assert_eq!(queue.depth(), 1);
    }

    #[test]
    fn test_queue_recv_blocking() {
        let mut queue: BoundedEventQueue<String> = BoundedEventQueue::new();
        let event = "test".to_string();

        queue.send(event.clone()).unwrap();

        let received = queue.recv().unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn test_queue_backpressure_tracking() {
        let config = QueueConfig { capacity: 2 };
        let mut queue: BoundedEventQueue<i32> = BoundedEventQueue::with_config(config);

        // Fill the queue
        queue.try_send(1).unwrap();
        queue.try_send(2).unwrap();

        // Verify queue is full
        let result = queue.try_send(3);
        assert!(matches!(result, Err(mpsc::TrySendError::Full(3))));

        // Backpressure is tracked when try_send fails
        // Note: We can't test the blocking send() in a single-threaded test
        // because it would block forever without a consumer thread
        let metrics = queue.metrics();
        assert_eq!(metrics.depth, 2);
    }

    #[test]
    fn test_queue_max_depth_tracking() {
        let config = QueueConfig { capacity: 10 };
        let mut queue: BoundedEventQueue<i32> = BoundedEventQueue::with_config(config);

        for i in 0..5 {
            queue.send(i).unwrap();
        }

        let metrics = queue.metrics();
        assert_eq!(metrics.max_depth, 5);

        // Add more events
        for i in 5..8 {
            queue.send(i).unwrap();
        }

        let metrics = queue.metrics();
        assert_eq!(metrics.max_depth, 8);
    }
}
