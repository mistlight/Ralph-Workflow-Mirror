//! Real process executor implementation.
//!
//! This module provides the production implementation that spawns actual processes
//! using `std::process::Command`.

use super::{AgentChildHandle, AgentSpawnConfig, ProcessExecutor, ProcessOutput, RealAgentChild};
use std::io;
use std::path::Path;

#[cfg(unix)]
fn set_nonblocking_fd(fd: std::os::unix::io::RawFd) -> io::Result<()> {
    // Make the file descriptor non-blocking so readers can poll/cancel without
    // getting stuck in a blocking read().
    //
    // Safety: fcntl is called with a valid fd owned by this process.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Real process executor that uses `std::process::Command`.
///
/// This is the production implementation that spawns actual processes.
#[derive(Debug, Clone, Default)]
pub struct RealProcessExecutor;

impl RealProcessExecutor {
    /// Create a new RealProcessExecutor.
    pub fn new() -> Self {
        Self
    }
}

impl ProcessExecutor for RealProcessExecutor {
    fn execute(
        &self,
        command: &str,
        args: &[&str],
        env: &[(String, String)],
        workdir: Option<&Path>,
    ) -> io::Result<ProcessOutput> {
        let mut cmd = std::process::Command::new(command);
        cmd.args(args);

        for (key, value) in env {
            cmd.env(key, value);
        }

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        let output = cmd.output()?;

        Ok(ProcessOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    fn spawn(
        &self,
        command: &str,
        args: &[&str],
        env: &[(String, String)],
        workdir: Option<&Path>,
    ) -> io::Result<std::process::Child> {
        let mut cmd = std::process::Command::new(command);
        cmd.args(args);

        for (key, value) in env {
            cmd.env(key, value);
        }

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    }

    fn spawn_agent(&self, config: &AgentSpawnConfig) -> io::Result<AgentChildHandle> {
        let mut cmd = std::process::Command::new(&config.command);
        cmd.args(&config.args);

        // Set environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        // Add the prompt as the final argument
        cmd.arg(&config.prompt);

        // Set buffering variables for real-time streaming
        cmd.env("PYTHONUNBUFFERED", "1");
        cmd.env("NODE_ENV", "production");

        // Put the agent in its own process group so idle-timeout enforcement can
        // terminate the whole subtree (and not just the direct child PID).
        #[cfg(unix)]
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }

        // Spawn the process with piped stdout/stderr
        let mut child = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("Failed to capture stderr"))?;

        // The stderr collector and stdout pump rely on non-blocking reads so they can
        // be cancelled promptly (idle timeout, early failures).
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            set_nonblocking_fd(stdout.as_raw_fd())?;
            set_nonblocking_fd(stderr.as_raw_fd())?;
        }

        Ok(AgentChildHandle {
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            inner: Box::new(RealAgentChild(child)),
        })
    }
}
