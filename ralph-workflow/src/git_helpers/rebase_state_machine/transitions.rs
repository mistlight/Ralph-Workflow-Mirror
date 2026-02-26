// Lock management and state transition helpers for rebase operations.
//
// This file contains the RebaseLock RAII guard and lock acquisition/release functions.

/// Rebase lock file name.
const REBASE_LOCK_FILE: &str = "rebase.lock";

/// Default lock timeout in seconds (30 minutes).
const DEFAULT_LOCK_TIMEOUT_SECONDS: u64 = 1800;

/// Get the rebase lock file path.
///
/// The lock is stored in `.agent/rebase.lock`
/// relative to the current working directory.
fn rebase_lock_path() -> String {
    format!(".agent/{REBASE_LOCK_FILE}")
}

/// RAII-style guard for rebase lock.
///
/// Automatically releases the lock when dropped.
pub struct RebaseLock {
    /// Whether we own the lock
    owns_lock: bool,
}

impl Drop for RebaseLock {
    fn drop(&mut self) {
        if self.owns_lock {
            let _ = release_rebase_lock();
        }
    }
}

impl RebaseLock {
    /// Create a new lock guard that owns the lock.
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails.
    pub fn new() -> io::Result<Self> {
        acquire_rebase_lock()?;
        Ok(Self { owns_lock: true })
    }

    /// Relinquish ownership of the lock without releasing it.
    ///
    /// This is useful when transferring ownership.
    #[must_use]
    #[cfg(any(test, feature = "test-utils"))]
    pub fn leak(mut self) -> bool {
        let owned = self.owns_lock;
        self.owns_lock = false;
        owned
    }
}

/// Acquire the rebase lock.
///
/// Creates a lock file with the current process ID and timestamp.
/// Returns an error if the lock is held by another process.
///
/// # Errors
///
/// Returns an error if:
/// - The lock file exists and is not stale
/// - The lock file cannot be created
pub fn acquire_rebase_lock() -> io::Result<()> {
    let lock_path = rebase_lock_path();
    let path = Path::new(&lock_path);

    // Ensure .agent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check if lock already exists
    if path.exists() {
        if !is_lock_stale()? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Rebase is already in progress. If you believe this is incorrect, \
                 wait 30 minutes for the lock to expire or manually remove `.agent/rebase.lock`.",
            ));
        }
        // Lock is stale, remove it
        fs::remove_file(path)?;
    }

    // Create lock file with PID and timestamp
    let pid = std::process::id();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let lock_content = format!("pid={pid}\ntimestamp={timestamp}\n");

    let mut file = fs::File::create(path)?;
    file.write_all(lock_content.as_bytes())?;
    file.sync_all()?;

    Ok(())
}

/// Release the rebase lock.
///
/// Removes the lock file. Does nothing if no lock exists.
///
/// # Errors
///
/// Returns an error if the lock file exists but cannot be removed.
pub fn release_rebase_lock() -> io::Result<()> {
    let lock_path = rebase_lock_path();
    let path = Path::new(&lock_path);

    if path.exists() {
        fs::remove_file(path)?;
    }

    Ok(())
}

/// Check if the lock file is stale.
///
/// A lock is considered stale if it's older than the timeout period.
///
/// # Returns
///
/// Returns `true` if the lock is stale, `false` otherwise.
fn is_lock_stale() -> io::Result<bool> {
    let lock_path = rebase_lock_path();
    let path = Path::new(&lock_path);

    if !path.exists() {
        return Ok(false);
    }

    // Read lock file to get timestamp
    let content = fs::read_to_string(path)?;

    // Parse timestamp from lock file
    let timestamp_line = content
        .lines()
        .find(|line| line.starts_with("timestamp="))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Lock file missing timestamp"))?;

    let timestamp_str = timestamp_line.strip_prefix("timestamp=").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid timestamp format in lock file",
        )
    })?;

    let lock_time = chrono::DateTime::parse_from_rfc3339(timestamp_str).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid timestamp format in lock file",
        )
    })?;

    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(lock_time);

    Ok(elapsed.num_seconds() > DEFAULT_LOCK_TIMEOUT_SECONDS as i64)
}
