//! Real-time file system monitoring for PROMPT.md protection.
//!
//! This module provides proactive monitoring to detect deletion attempts
//! on PROMPT.md immediately, rather than waiting for periodic checks.
//! It uses the `notify` crate for cross-platform file system events.
//!
//! # Effect System Exception
//!
//! This module uses `std::fs` directly rather than the `Workspace` trait.
//! This is a documented exception to the effect system architecture because:
//!
//! 1. **Real-time filesystem monitoring**: The `notify` crate requires watching
//!    the actual filesystem for events (inotify, FSEvents, ReadDirectoryChangesW).
//! 2. **Background thread operation**: The monitor runs in a separate thread
//!    that cannot share `PhaseContext` or workspace references.
//! 3. **OS-level event handling**: File system events are inherently tied to
//!    the real filesystem, not an abstraction layer.
//!
//! This exception is documented in `docs/architecture/effect-system.md`.
//!
//! # Design
//!
//! The monitor runs in a background thread and watches for deletion events
//! on PROMPT.md. When a deletion is detected, it's automatically restored
//! from backup. The main thread can poll the monitor to check if any
//! restoration events occurred.
//!
//! # Platform Support
//!
//! - **Unix/Linux**: inotify via `notify` crate
//! - **macOS**: `FSEvents` via `notify` crate
//! - **Windows**: `ReadDirectoryChangesW` via `notify` crate

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// File system monitor for detecting PROMPT.md deletion events.
///
/// The monitor watches for deletion events and automatically restores
/// PROMPT.md from backup when detected. Monitoring happens in a background
/// thread, so the main thread is not blocked.
///
/// # Example
///
/// ```no_run
/// # use ralph_workflow::files::protection::monitoring::PromptMonitor;
/// let mut monitor = PromptMonitor::new().unwrap();
/// monitor.start().unwrap();
///
/// // ... run pipeline phases ...
///
/// // Check if any restoration occurred
/// if monitor.check_and_restore() {
///     println!("PROMPT.md was restored!");
/// }
///
/// monitor.stop();
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct PromptMonitor {
    /// Flag indicating if PROMPT.md was deleted and restored
    restoration_detected: Arc<AtomicBool>,
    /// Flag to signal the monitor thread to stop
    stop_signal: Arc<AtomicBool>,
    /// Handle to the monitor thread (None if not started)
    monitor_thread: Option<thread::JoinHandle<()>>,
}

impl PromptMonitor {
    /// Create a new file system monitor for PROMPT.md.
    ///
    /// Returns an error if the current directory cannot be accessed or
    /// if PROMPT.md doesn't exist (we need to know what to watch for).
    pub fn new() -> std::io::Result<Self> {
        // Verify we're in a valid directory with PROMPT.md
        let prompt_path = Path::new("PROMPT.md");
        if !prompt_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "PROMPT.md does not exist - cannot monitor",
            ));
        }

        Ok(Self {
            restoration_detected: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new(AtomicBool::new(false)),
            monitor_thread: None,
        })
    }

    /// Start monitoring PROMPT.md for deletion events.
    ///
    /// This spawns a background thread that watches for file system events.
    /// Returns immediately; monitoring happens asynchronously.
    ///
    /// The monitor will automatically restore PROMPT.md from backup if
    /// deletion is detected.
    pub fn start(&mut self) -> std::io::Result<()> {
        if self.monitor_thread.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Monitor is already running",
            ));
        }

        let restoration_flag = Arc::clone(&self.restoration_detected);
        let stop_signal = Arc::clone(&self.stop_signal);

        let handle = thread::spawn(move || {
            Self::monitor_thread_main(&restoration_flag, &stop_signal);
        });

        self.monitor_thread = Some(handle);
        Ok(())
    }

    /// Background thread entry point for file system monitoring.
    ///
    /// This thread watches the current directory for deletion events on
    /// PROMPT.md and restores from backup when detected.
    fn monitor_thread_main(restoration_detected: &Arc<AtomicBool>, stop_signal: &Arc<AtomicBool>) {
        use notify::Watcher;

        // UNBOUNDED CHANNEL JUSTIFICATION (monitoring.rs:129):
        //
        // This channel receives filesystem events from the notify watcher.
        // Unbounded is safe here because:
        //
        // 1. Event rate bound: Filesystem events are rare (only PROMPT.md changes)
        // 2. Expected rate: <10 events per hour in typical usage
        // 3. Worst case: Even under attack (rapid create/delete), event rate limited by OS
        // 4. Fallback protection: If watcher fails, falls back to polling
        // 5. Thread lifecycle: Monitor thread is joined on shutdown (no leak)
        //
        // Alternative considered: sync_channel(100)
        // Rejected because: Would add unnecessary complexity for extremely low event rate
        // and could theoretically drop events under rapid filesystem activity.
        //
        // The notify crate's recommended_watcher() returns a channel-based interface;
        // using unbounded matches the notify crate's design and is safe given the
        // low event rate.
        //
        // See: tests/integration_tests/memory_safety/channel_bounds.rs for verification
        let (tx, rx) = std::sync::mpsc::channel();

        // Create a watcher for the current directory
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Warning: Failed to create file system watcher: {e}");
                eprintln!("Falling back to periodic polling for PROMPT.md protection");
                // Fallback to polling if watcher creation fails
                Self::polling_monitor(restoration_detected, stop_signal);
                return;
            }
        };

        // Watch the current directory for events
        if let Err(e) = watcher.watch(Path::new("."), notify::RecursiveMode::NonRecursive) {
            eprintln!("Warning: Failed to watch current directory: {e}");
            eprintln!("Falling back to periodic polling for PROMPT.md protection");
            Self::polling_monitor(restoration_detected, stop_signal);
            return;
        }

        // Process events until stop signal is received
        let mut prompt_existed_last_check = true;

        while !stop_signal.load(Ordering::Relaxed) {
            // Check for events with a short timeout
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => {
                    Self::handle_fs_event(
                        &event,
                        restoration_detected,
                        &mut prompt_existed_last_check,
                    );
                }
                Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Error in watcher or timeout - continue anyway
                }
                Err(_) => {
                    // Channel disconnected - stop monitoring
                    break;
                }
            }
        }
    }

    /// Handle a file system event from the watcher.
    fn handle_fs_event(
        event: &notify::Event,
        restoration_detected: &Arc<AtomicBool>,
        _prompt_existed_last_check: &mut bool,
    ) {
        for path in &event.paths {
            if is_prompt_md_path(path) {
                // Check for remove event
                if matches!(event.kind, notify::EventKind::Remove(_)) {
                    // PROMPT.md was removed - restore it
                    if Self::restore_from_backup() {
                        restoration_detected.store(true, Ordering::Release);
                    }
                }
            }
        }
    }

    /// Fallback polling-based monitor when file system watcher fails.
    ///
    /// Some filesystems (NFS, network drives) don't support file system
    /// events. This fallback polls every 100ms to check if PROMPT.md exists.
    fn polling_monitor(restoration_detected: &Arc<AtomicBool>, stop_signal: &Arc<AtomicBool>) {
        let mut prompt_existed = Path::new("PROMPT.md").exists();

        while !stop_signal.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100));

            let prompt_exists_now = Path::new("PROMPT.md").exists();

            // Detect deletion (transition from exists to not exists)
            if prompt_existed && !prompt_exists_now && Self::restore_from_backup() {
                restoration_detected.store(true, Ordering::Release);
            }

            prompt_existed = prompt_exists_now;
        }
    }

    /// Restore PROMPT.md from backup.
    ///
    /// Tries backups in order:
    /// - .agent/PROMPT.md.backup
    /// - .agent/PROMPT.md.backup.1
    /// - .agent/PROMPT.md.backup.2
    ///
    /// Returns true if restoration succeeded, false otherwise.
    ///
    /// Uses atomic open to avoid TOCTOU race conditions - opens and reads
    /// the file in one operation rather than checking existence separately.
    fn restore_from_backup() -> bool {
        let backup_paths = [
            Path::new(".agent/PROMPT.md.backup"),
            Path::new(".agent/PROMPT.md.backup.1"),
            Path::new(".agent/PROMPT.md.backup.2"),
        ];

        for backup_path in &backup_paths {
            // Use std::fs::File::open to atomically open the file, avoiding TOCTOU
            // race conditions where the file could be replaced between exists() check
            // and read operation
            let backup_content = match std::fs::File::open(backup_path) {
                Ok(mut file) => {
                    // Verify it's a regular file, not a symlink or special file
                    match file.metadata() {
                        Ok(metadata) if metadata.is_file() => {
                            // Read the content
                            let mut buffer = String::new();
                            match std::io::Read::read_to_string(&mut file, &mut buffer) {
                                Ok(_) => buffer,
                                Err(_) => continue,
                            }
                        }
                        _ => continue, // Not a regular file, skip
                    }
                }
                Err(_) => continue, // File doesn't exist or can't be opened
            };

            if backup_content.trim().is_empty() {
                continue;
            }

            // Restore from backup - ensure parent directory exists
            let prompt_path = Path::new("PROMPT.md");
            if let Some(parent) = prompt_path.parent().filter(|p| !p.as_os_str().is_empty()) {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("Failed to create parent directory for PROMPT.md: {e}");
                    continue;
                }
            }

            if fs::write(prompt_path, backup_content).is_err() {
                eprintln!("Failed to write PROMPT.md from backup");
                continue;
            }

            // Set read-only permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(prompt_path) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o444);
                    let _ = fs::set_permissions(prompt_path, perms);
                }
            }

            #[cfg(windows)]
            {
                if let Ok(metadata) = fs::metadata(prompt_path) {
                    let mut perms = metadata.permissions();
                    perms.set_readonly(true);
                    let _ = fs::set_permissions(prompt_path, perms);
                }
            }

            return true;
        }

        false
    }

    /// Check if any restoration events were detected and reset the flag.
    ///
    /// Returns true if PROMPT.md was deleted and restored since the last
    /// check. This is a one-time check - the flag is reset after reading.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ralph_workflow::files::protection::monitoring::PromptMonitor;
    /// # let mut monitor = PromptMonitor::new().unwrap();
    /// # monitor.start().unwrap();
    /// // After running some agent code
    /// if monitor.check_and_restore() {
    ///     println!("PROMPT.md was restored during this phase!");
    /// }
    /// ```
    pub fn check_and_restore(&self) -> bool {
        self.restoration_detected.swap(false, Ordering::AcqRel)
    }

    /// Stop monitoring and cleanup resources.
    ///
    /// Signals the monitor thread to stop and waits for it to complete.
    pub fn stop(mut self) {
        // Signal the thread to stop
        self.stop_signal.store(true, Ordering::Release);

        // Wait for the thread to finish and check for panics
        if let Some(handle) = self.monitor_thread.take() {
            if let Err(panic_payload) = handle.join() {
                // Thread panicked - extract and log panic message for diagnostics
                // Try common panic payload types
                let panic_msg = panic_payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| {
                        panic_payload
                            .downcast_ref::<&str>()
                            .map(ToString::to_string)
                    })
                    .or_else(|| {
                        panic_payload
                            .downcast_ref::<&String>()
                            .map(|s| (*s).clone())
                    })
                    .unwrap_or_else(|| {
                        // Fallback: Try to get any available information
                        format!(
                            "<unknown panic type: {}>",
                            std::any::type_name_of_val(&panic_payload)
                        )
                    });
                eprintln!("Warning: File monitoring thread panicked: {panic_msg}");
            }
        }
    }
}

fn is_prompt_md_path(path: &Path) -> bool {
    matches!(path.file_name(), Some(name) if name == "PROMPT.md")
}

impl Drop for PromptMonitor {
    fn drop(&mut self) {
        // Signal the thread to stop when dropped
        self.stop_signal.store(true, Ordering::Release);

        // Take the handle and let it finish on its own
        // (we can't wait in Drop because we might be panicking)
        let _ = self.monitor_thread.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_prompt_md_path_matches_by_file_name() {
        assert!(is_prompt_md_path(Path::new("PROMPT.md")));
        assert!(is_prompt_md_path(Path::new("./PROMPT.md")));
        assert!(is_prompt_md_path(Path::new("dir/PROMPT.md")));
        assert!(is_prompt_md_path(Path::new("/tmp/PROMPT.md")));

        assert!(!is_prompt_md_path(Path::new("PROMPT.md.backup")));
        assert!(!is_prompt_md_path(Path::new("PROMPT.mdx")));
    }

    #[test]
    fn test_check_and_restore_returns_and_clears_flag() {
        let monitor = PromptMonitor {
            restoration_detected: Arc::new(AtomicBool::new(true)),
            stop_signal: Arc::new(AtomicBool::new(false)),
            monitor_thread: None,
        };

        assert!(monitor.check_and_restore());
        assert!(!monitor.check_and_restore());
    }
}
