use super::super::AgentChild;
use std::io;

/// Mock agent child process for testing.
///
/// This simulates a real `std::process::Child` with configurable termination behavior.
#[derive(Debug)]
pub struct MockAgentChild {
    exit_code: i32,
    /// Simulates a process that hasn't terminated yet.
    still_running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl MockAgentChild {
    #[must_use]
    pub fn new(exit_code: i32) -> Self {
        Self {
            exit_code,
            still_running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create a mock child that simulates a running process that needs to be killed.
    /// Set the returned `AtomicBool` to `false` to simulate process termination.
    #[must_use]
    pub fn new_running(exit_code: i32) -> (Self, std::sync::Arc<std::sync::atomic::AtomicBool>) {
        let still_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let controller = std::sync::Arc::clone(&still_running);
        (
            Self {
                exit_code,
                still_running,
            },
            controller,
        )
    }
}

impl AgentChild for MockAgentChild {
    fn id(&self) -> u32 {
        12345 // Mock PID
    }

    fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        while self
            .still_running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        #[cfg(unix)]
        return Ok(std::process::ExitStatus::from_raw(self.exit_code << 8));
        #[cfg(not(unix))]
        return Ok(std::process::ExitStatus::default());
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        if self
            .still_running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Ok(None);
        }

        #[cfg(unix)]
        return Ok(Some(std::process::ExitStatus::from_raw(
            self.exit_code << 8,
        )));
        #[cfg(not(unix))]
        return Ok(Some(std::process::ExitStatus::default()));
    }
}
