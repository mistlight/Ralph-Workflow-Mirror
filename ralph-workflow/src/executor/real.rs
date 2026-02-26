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

#[cfg(unix)]
fn ensure_nonblocking_or_terminate(
    child: &mut std::process::Child,
    stdout_fd: std::os::unix::io::RawFd,
    stderr_fd: std::os::unix::io::RawFd,
) -> io::Result<()> {
    fn terminate_child_best_effort(child: &mut std::process::Child) {
        use std::time::{Duration, Instant};

        let pid = child.id().min(i32::MAX as u32) as i32;

        // Prefer killing the process group first (agent is in its own pgid).
        unsafe {
            let _ = libc::kill(-pid, libc::SIGTERM);
            let _ = libc::kill(pid, libc::SIGTERM);
        }

        let term_deadline = Instant::now() + Duration::from_millis(250);
        while Instant::now() < term_deadline {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            }
        }

        unsafe {
            let _ = libc::kill(-pid, libc::SIGKILL);
            let _ = libc::kill(pid, libc::SIGKILL);
        }

        let kill_deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < kill_deadline {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    if let Err(e) = set_nonblocking_fd(stdout_fd) {
        terminate_child_best_effort(child);
        return Err(e);
    }

    if let Err(e) = set_nonblocking_fd(stderr_fd) {
        terminate_child_best_effort(child);
        return Err(e);
    }

    Ok(())
}

/// Real process executor that uses `std::process::Command`.
///
/// This is the production implementation that spawns actual processes.
#[derive(Debug, Clone, Default)]
pub struct RealProcessExecutor;

impl RealProcessExecutor {
    /// Create a new `RealProcessExecutor`.
    #[must_use]
    pub const fn new() -> Self {
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
            ensure_nonblocking_or_terminate(&mut child, stdout.as_raw_fd(), stderr.as_raw_fd())?;
        }

        Ok(AgentChildHandle {
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            inner: Box::new(RealAgentChild(child)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn ensure_nonblocking_or_terminate_kills_child_on_failure() {
        use std::process::Command;
        use std::time::{Duration, Instant};

        let mut child = Command::new("sleep")
            .arg("60")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn sleep");

        let result = ensure_nonblocking_or_terminate(&mut child, -1, -1);
        assert!(result.is_err(), "expected nonblocking setup to fail");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut exited = false;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => {
                    exited = true;
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => {
                    exited = true;
                    break;
                }
            }
        }

        // Ensure we don't leave a live subprocess behind even if the assertion fails.
        if !exited {
            let _ = child.kill();
            let _ = child.wait();
        }

        assert!(
            exited,
            "expected child to be terminated when nonblocking setup fails"
        );
    }
}
