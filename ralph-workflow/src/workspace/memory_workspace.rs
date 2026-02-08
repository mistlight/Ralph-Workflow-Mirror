// MemoryWorkspace - In-memory test implementation of the Workspace trait.
//
// This module provides an in-memory implementation that stores files in memory rather
// than on disk. This enables tests to:
// - Verify file operations without touching the real filesystem
// - Control what file reads return (including error conditions)
// - Run in parallel without filesystem conflicts
// - Execute quickly and deterministically
//
// ## Thread Safety and RwLock Poisoning
//
// The workspace uses `RwLock` for interior mutability to allow concurrent reads while
// serializing writes. Lock operations use `.expect()` instead of `.unwrap()` with
// descriptive panic messages for clarity when failures occur.
//
// **RwLock Poisoning:** An `RwLock` becomes "poisoned" when a thread panics while holding
// the lock. This prevents data corruption by ensuring no thread can access potentially
// inconsistent state left by the panicked thread.
//
// In test infrastructure like `MemoryWorkspace`, poisoning indicates a serious test bug
// (a panic while holding the workspace lock). Using `.expect()` with a clear message
// helps diagnose these issues quickly:
// - The panic message identifies which lock was poisoned
// - The message explains what poisoning means (panic in another thread)
// - The original panic that caused poisoning is preserved in the stack trace
//
// For production code paths that must not panic, prefer returning `Result` and handling
// lock poisoning errors explicitly. For test infrastructure, `.expect()` with descriptive
// messages is acceptable as poisoning indicates a test bug that should be fixed.

/// In-memory file entry with content and metadata.
#[derive(Debug, Clone)]
struct MemoryFile {
    content: Vec<u8>,
    modified: std::time::SystemTime,
}

impl MemoryFile {
    fn new(content: Vec<u8>) -> Self {
        Self {
            content,
            modified: std::time::SystemTime::now(),
        }
    }

    fn with_modified(content: Vec<u8>, modified: std::time::SystemTime) -> Self {
        Self { content, modified }
    }
}

/// In-memory workspace implementation for testing.
///
/// All file operations are performed against an in-memory HashMap, allowing tests to:
/// - Verify what was written without touching real files
/// - Control what reads return
/// - Run in parallel without filesystem conflicts
/// - Be deterministic and fast
#[derive(Debug)]
pub struct MemoryWorkspace {
    root: PathBuf,
    files: std::sync::RwLock<std::collections::HashMap<PathBuf, MemoryFile>>,
    directories: std::sync::RwLock<std::collections::HashSet<PathBuf>>,
}

impl MemoryWorkspace {
    /// Create a new in-memory workspace with the given virtual root path.
    ///
    /// The root path is used for path resolution but no real filesystem access occurs.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: std::sync::RwLock::new(std::collections::HashMap::new()),
            directories: std::sync::RwLock::new(std::collections::HashSet::new()),
        }
    }

    /// Create a new in-memory workspace with a default test root path.
    pub fn new_test() -> Self {
        Self::new(PathBuf::from("/test/repo"))
    }

    /// Ensure all parent directories exist for the given path.
    ///
    /// This is a helper to reduce duplication in file/directory creation methods.
    fn ensure_parent_dirs(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if parent.as_os_str().is_empty() {
                return;
            }
            let mut dirs = self.directories.write()
                .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock");
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
        }
    }

    /// Ensure all components of the path exist as directories.
    ///
    /// Used for creating directories themselves (not just parents).
    fn ensure_dir_path(&self, path: &Path) {
        let mut dirs = self.directories.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock");
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component);
            dirs.insert(current.clone());
        }
    }

    /// Pre-populate a file with content for testing.
    ///
    /// Also creates parent directories automatically.
    pub fn with_file(self, path: &str, content: &str) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files
            .write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .insert(path_buf, MemoryFile::new(content.as_bytes().to_vec()));
        self
    }

    /// Pre-populate a file with content and explicit modification time for testing.
    ///
    /// Also creates parent directories automatically.
    pub fn with_file_at_time(
        self,
        path: &str,
        content: &str,
        modified: std::time::SystemTime,
    ) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .insert(
                path_buf,
                MemoryFile::with_modified(content.as_bytes().to_vec(), modified),
            );
        self
    }

    /// Pre-populate a file with bytes for testing.
    ///
    /// Also creates parent directories automatically.
    pub fn with_file_bytes(self, path: &str, content: &[u8]) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files
            .write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .insert(path_buf, MemoryFile::new(content.to_vec()));
        self
    }

    /// Pre-populate a directory for testing.
    pub fn with_dir(self, path: &str) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_dir_path(&path_buf);
        self
    }

    /// List all files in a directory (for test assertions).
    ///
    /// Returns file paths relative to the workspace root.
    pub fn list_files_in_dir(&self, dir: &str) -> Vec<PathBuf> {
        let dir_path = PathBuf::from(dir);
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .keys()
            .filter(|path| {
                path.parent()
                    .map(|p| p == dir_path || p.starts_with(&dir_path))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Get the modification time of a file (for test assertions).
    pub fn get_modified(&self, path: &str) -> Option<std::time::SystemTime> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .get(&PathBuf::from(path))
            .map(|f| f.modified)
    }

    /// List all directories (for test assertions).
    pub fn list_directories(&self) -> Vec<PathBuf> {
        self.directories.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock")
            .iter().cloned().collect()
    }

    /// Get all files that were written (for test assertions).
    pub fn written_files(&self) -> std::collections::HashMap<PathBuf, Vec<u8>> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .iter()
            .map(|(k, v)| (k.clone(), v.content.clone()))
            .collect()
    }

    /// Get a specific file's content (for test assertions).
    pub fn get_file(&self, path: &str) -> Option<String> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .get(&PathBuf::from(path))
            .map(|f| String::from_utf8_lossy(&f.content).to_string())
    }

    /// Get a specific file's bytes (for test assertions).
    pub fn get_file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .get(&PathBuf::from(path))
            .map(|f| f.content.clone())
    }

    /// Check if a file was written (for test assertions).
    pub fn was_written(&self, path: &str) -> bool {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .contains_key(&PathBuf::from(path))
    }

    /// Clear all files (for test setup).
    pub fn clear(&self) {
        self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .clear();
        self.directories.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock")
            .clear();
    }
}

impl Workspace for MemoryWorkspace {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read(&self, relative: &Path) -> io::Result<String> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .get(relative)
            .map(|f| String::from_utf8_lossy(&f.content).to_string())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("File not found: {}", relative.display()),
                )
            })
    }

    fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .get(relative)
            .map(|f| f.content.clone())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("File not found: {}", relative.display()),
                )
            })
    }

    fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
        self.ensure_parent_dirs(relative);
        self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .insert(
                relative.to_path_buf(),
                MemoryFile::new(content.as_bytes().to_vec()),
            );
        Ok(())
    }

    fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.ensure_parent_dirs(relative);
        self.files
            .write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .insert(relative.to_path_buf(), MemoryFile::new(content.to_vec()));
        Ok(())
    }

    fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.ensure_parent_dirs(relative);
        let mut files = self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock");
        let entry = files
            .entry(relative.to_path_buf())
            .or_insert_with(|| MemoryFile::new(Vec::new()));
        entry.content.extend_from_slice(content);
        entry.modified = std::time::SystemTime::now();
        Ok(())
    }

    fn exists(&self, relative: &Path) -> bool {
        self.files.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .contains_key(relative)
            || self.directories.read()
                .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock")
                .contains(relative)
    }

    fn is_file(&self, relative: &Path) -> bool {
        self.files.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .contains_key(relative)
    }

    fn is_dir(&self, relative: &Path) -> bool {
        self.directories.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock")
            .contains(relative)
    }

    fn remove(&self, relative: &Path) -> io::Result<()> {
        self.files
            .write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .remove(relative)
            .map(|_| ())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("File not found: {}", relative.display()),
                )
            })
    }

    fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
        self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .remove(relative);
        Ok(())
    }

    fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
        // Check if directory exists first
        if !self.directories.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock")
            .contains(relative) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Directory not found: {}", relative.display()),
            ));
        }
        self.remove_dir_all_if_exists(relative)
    }

    fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
        // Remove all files under this directory
        {
            let mut files = self.files.write()
                .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock");
            let to_remove: Vec<PathBuf> = files
                .keys()
                .filter(|path| path.starts_with(relative))
                .cloned()
                .collect();
            for path in to_remove {
                files.remove(&path);
            }
        }
        // Remove all directories under this directory (including itself)
        {
            let mut dirs = self.directories.write()
                .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock");
            let to_remove: Vec<PathBuf> = dirs
                .iter()
                .filter(|path| path.starts_with(relative) || *path == relative)
                .cloned()
                .collect();
            for path in to_remove {
                dirs.remove(&path);
            }
        }
        Ok(())
    }

    fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.ensure_dir_path(relative);
        Ok(())
    }

    fn read_dir(&self, relative: &Path) -> io::Result<Vec<DirEntry>> {
        let files = self.files.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock");
        let dirs = self.directories.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock");

        // Check if the directory exists
        if !relative.as_os_str().is_empty() && !dirs.contains(relative) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Directory not found: {}", relative.display()),
            ));
        }

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Find all files that are direct children of this directory
        for (path, mem_file) in files.iter() {
            if let Some(parent) = path.parent() {
                if parent == relative {
                    if let Some(name) = path.file_name() {
                        if seen.insert(name.to_os_string()) {
                            entries.push(DirEntry::with_modified(
                                path.clone(),
                                true,
                                false,
                                mem_file.modified,
                            ));
                        }
                    }
                }
            }
        }

        // Find all directories that are direct children of this directory
        for dir_path in dirs.iter() {
            if let Some(parent) = dir_path.parent() {
                if parent == relative {
                    if let Some(name) = dir_path.file_name() {
                        if seen.insert(name.to_os_string()) {
                            entries.push(DirEntry::new(dir_path.clone(), false, true));
                        }
                    }
                }
            }
        }

        Ok(entries)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        // Create parent directories for destination first (before taking files lock)
        self.ensure_parent_dirs(to);
        let mut files = self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock");
        if let Some(file) = files.remove(from) {
            files.insert(to.to_path_buf(), file);
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("File not found: {}", from.display()),
            ))
        }
    }

    fn set_readonly(&self, _relative: &Path) -> io::Result<()> {
        // No-op for in-memory workspace - permissions aren't relevant for testing
        Ok(())
    }

    fn set_writable(&self, _relative: &Path) -> io::Result<()> {
        // No-op for in-memory workspace - permissions aren't relevant for testing
        Ok(())
    }

    fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
        // In-memory operations are inherently atomic - no partial state possible.
        // Just delegate to regular write().
        self.write(relative, content)
    }
}

impl Clone for MemoryWorkspace {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            files: std::sync::RwLock::new(self.files.read()
                .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
                .clone()),
            directories: std::sync::RwLock::new(self.directories.read()
                .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock")
                .clone()),
        }
    }
}
