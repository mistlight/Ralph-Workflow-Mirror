use git2::{IndexAddOption, Oid, Repository, Signature, Status, StatusOptions};
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

/// Global mutex for tests that modify the current working directory.
/// All tests that change CWD must use this lock to ensure they don't
/// interfere with each other when run in parallel.
pub static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn init_git_repo(dir: &TempDir) -> Repository {
    let repo = Repository::init(dir.path()).expect("init git repo");

    // Configure user for libgit2's repo.signature() and Ralph's git_commit().
    let mut cfg = repo.config().expect("repo config");
    cfg.set_str("user.name", "Test User")
        .expect("set user.name");
    cfg.set_str("user.email", "test@example.com")
        .expect("set user.email");

    fs::write(
        dir.path().join(".gitignore"),
        ".agent/\n.no_agent_commit\nPROMPT.md\n",
    )
    .expect("write .gitignore");
    fs::write(
        dir.path().join("PROMPT.md"),
        "# Test Requirements\nTest task",
    )
    .expect("write PROMPT.md");
    fs::create_dir_all(dir.path().join(".agent")).expect("create .agent");

    repo
}

// Test support utilities for integration tests.
// These functions are used across multiple test files.
pub fn write_file<P: AsRef<Path>>(path: P, contents: &str) {
    if let Some(parent) = path.as_ref().parent() {
        if !parent.as_os_str().is_empty() {
            let _ = fs::create_dir_all(parent);
        }
    }
    fs::write(path, contents).expect("write file");
}

pub fn commit_all(repo: &Repository, message: &str) -> Oid {
    stage_all(repo);

    let mut index = repo.index().expect("open index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");

    let sig = Signature::now("Test User", "test@example.com").expect("signature");

    match repo.head() {
        Ok(head) => {
            let parent = head.peel_to_commit().expect("parent commit");
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
                .expect("commit")
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
            .expect("initial commit"),
        Err(e) => panic!("unexpected head error: {e}"),
    }
}

#[allow(dead_code)]
pub fn head_oid(repo: &Repository) -> String {
    repo.head()
        .ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string())
        .unwrap_or_default()
}

pub fn stage_all(repo: &Repository) {
    let mut index = repo.index().expect("open index");

    // Stage deletions explicitly.
    let mut status_opts = StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo.statuses(Some(&mut status_opts)).expect("statuses");
    for entry in statuses.iter() {
        if entry.status().contains(Status::WT_DELETED) {
            if let Some(path) = entry.path() {
                index
                    .remove_path(Path::new(path))
                    .expect("remove deleted path");
            }
        }
    }

    index
        .add_all(["."], IndexAddOption::DEFAULT, None)
        .expect("add_all");
    index.write().expect("write index");
}

/// Global mutex for tests that modify the current working directory.
///
/// Since changing CWD affects all threads, tests that do so must be
/// serialized. This mutex ensures that only one test can change CWD at
/// a time, preventing race conditions and flaky tests.
pub static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// RAII guard to restore the working directory on drop.
struct DirGuard(std::path::PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// Run a test function in a temporary directory.
///
/// This function:
/// 1. Acquires a global lock to prevent CWD race conditions
/// 2. Creates a temporary directory
/// 3. Changes to that directory
/// 4. Runs the provided test function
/// 5. Restores the original directory (even on panic)
///
/// # Panics
///
/// If the mutex is poisoned (a previous test panicked while holding it),
/// this function will clear the poison and continue. This prevents a single
/// test failure from causing cascading failures.
///
/// # Example
///
/// ```ignore
/// use test_helpers::with_temp_cwd;
///
/// #[test]
/// fn test_something() {
///     with_temp_cwd(|dir| {
///         // dir is the TempDir, and we're already in it
///         std::fs::write("test.txt", "hello").unwrap();
///         assert!(std::path::Path::new("test.txt").exists());
///     });
/// }
/// ```
pub fn with_temp_cwd<F: FnOnce(&TempDir)>(f: F) {
    let lock = CWD_LOCK.get_or_init(|| Mutex::new(()));

    // Clear poison if a previous test panicked
    let _cwd_guard = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            // Clear the poison and continue - the directory will be restored
            // by the DirGuard even if the test panics
            poisoned.into_inner()
        }
    };

    let dir = TempDir::new().expect("Failed to create temp directory");
    let old_dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    std::env::set_current_dir(dir.path()).expect("Failed to change to temp directory");
    let _guard = DirGuard(old_dir);

    f(&dir);
}
