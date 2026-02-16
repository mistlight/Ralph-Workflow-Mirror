//! Heartbeat background task for cloud mode.
//!
//! Sends periodic heartbeat signals to the cloud API to indicate
//! the container is alive during long-running operations.

use super::CloudReporter;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Guard for heartbeat background task.
///
/// Automatically stops the heartbeat when dropped.
pub struct HeartbeatGuard {
    stop_tx: Option<mpsc::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl HeartbeatGuard {
    /// Start a heartbeat background task.
    ///
    /// The task will send heartbeats at the specified interval until
    /// the guard is dropped or the token is cancelled.
    pub fn start(reporter: Arc<dyn CloudReporter>, interval: Duration) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let handle = thread::spawn(move || {
            loop {
                match stop_rx.recv_timeout(interval) {
                    Ok(()) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        // Ignore heartbeat errors - graceful degradation
                        let _ = reporter.heartbeat();
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Self {
            stop_tx: Some(stop_tx),
            handle: Some(handle),
        }
    }
}

impl Drop for HeartbeatGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::mock::MockCloudReporter;
    use std::time::Instant;

    #[test]
    fn test_heartbeat_sends_periodic_signals() {
        let reporter = Arc::new(MockCloudReporter::new());
        let reporter_clone = Arc::clone(&reporter);

        let _guard = HeartbeatGuard::start(reporter_clone, Duration::from_millis(25));

        let deadline = Instant::now() + Duration::from_millis(750);
        while reporter.heartbeat_count() < 3 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }

        let count = reporter.heartbeat_count();
        assert!(count >= 3, "Expected at least 3 heartbeats, got {count}");
    }

    #[test]
    fn test_heartbeat_stops_on_drop() {
        let reporter = Arc::new(MockCloudReporter::new());
        let reporter_clone = Arc::clone(&reporter);

        {
            let _guard = HeartbeatGuard::start(reporter_clone, Duration::from_millis(25));
            thread::sleep(Duration::from_millis(100));
        } // guard dropped here

        let count_at_drop = reporter.heartbeat_count();
        thread::sleep(Duration::from_millis(100));
        let count_after_drop = reporter.heartbeat_count();

        assert_eq!(
            count_at_drop, count_after_drop,
            "Heartbeats should stop after guard is dropped"
        );
    }

    #[test]
    fn test_drop_does_not_block_for_full_interval() {
        let reporter = Arc::new(MockCloudReporter::new());
        let reporter_clone = Arc::clone(&reporter);

        let start = Instant::now();
        {
            let _guard = HeartbeatGuard::start(reporter_clone, Duration::from_secs(5));
            // Give the worker a chance to enter its sleep.
            thread::sleep(Duration::from_millis(50));
        }
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(500),
            "drop should return promptly; elapsed={:?}",
            elapsed
        );
    }
}
