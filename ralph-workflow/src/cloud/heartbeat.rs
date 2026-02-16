//! Heartbeat background task for cloud mode.
//!
//! Sends periodic heartbeat signals to the cloud API to indicate
//! the container is alive during long-running operations.

use super::CloudReporter;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Token for cancelling the heartbeat task.
pub struct CancellationToken {
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl CancellationToken {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Guard for heartbeat background task.
///
/// Automatically stops the heartbeat when dropped.
pub struct HeartbeatGuard {
    cancel_token: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl HeartbeatGuard {
    /// Start a heartbeat background task.
    ///
    /// The task will send heartbeats at the specified interval until
    /// the guard is dropped or the token is cancelled.
    pub fn start(reporter: Arc<dyn CloudReporter>, interval: Duration) -> Self {
        let cancel_token = CancellationToken::new();
        let token_clone = CancellationToken {
            cancelled: Arc::clone(&cancel_token.cancelled),
        };

        let handle = thread::spawn(move || {
            while !token_clone.is_cancelled() {
                thread::sleep(interval);
                if !token_clone.is_cancelled() {
                    // Ignore heartbeat errors - graceful degradation
                    let _ = reporter.heartbeat();
                }
            }
        });

        Self {
            cancel_token,
            handle: Some(handle),
        }
    }
}

impl Drop for HeartbeatGuard {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        if let Some(handle) = self.handle.take() {
            // Best-effort join with timeout
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::mock::MockCloudReporter;

    #[test]
    fn test_heartbeat_sends_periodic_signals() {
        let reporter = Arc::new(MockCloudReporter::new());
        let reporter_clone = Arc::clone(&reporter);

        let _guard = HeartbeatGuard::start(reporter_clone, Duration::from_millis(50));

        thread::sleep(Duration::from_millis(200));

        // Should have sent 3-4 heartbeats
        let count = reporter.heartbeat_count();
        assert!(
            count >= 3 && count <= 5,
            "Expected 3-5 heartbeats, got {}",
            count
        );
    }

    #[test]
    fn test_heartbeat_stops_on_drop() {
        let reporter = Arc::new(MockCloudReporter::new());
        let reporter_clone = Arc::clone(&reporter);

        {
            let _guard = HeartbeatGuard::start(reporter_clone, Duration::from_millis(50));
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
}
