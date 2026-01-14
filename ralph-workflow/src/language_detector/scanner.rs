//! File system scanning utilities.
//!
//! Functions for scanning directories and counting file extensions.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

/// Maximum number of files to scan (for performance on large repos)
pub(super) const MAX_FILES_TO_SCAN: usize = 2000;

/// Maximum directory depth to search for signature files
pub(super) const MAX_SIGNATURE_SEARCH_DEPTH: usize = 6;

/// Check if a directory name should be skipped during scanning.
pub(super) fn should_skip_dir_name(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    matches!(
        name,
        "node_modules"
            | "target"
            | "dist"
            | "build"
            | "vendor"
            | "__pycache__"
            | "venv"
            | ".venv"
            | "env"
    )
}

/// Scan directory recursively and count file extensions
pub(super) fn count_extensions(root: &Path) -> io::Result<HashMap<String, usize>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut files_scanned = 0;

    // Use a simple recursive approach with early termination
    fn scan_dir(
        dir: &Path,
        counts: &mut HashMap<String, usize>,
        files_scanned: &mut usize,
    ) -> io::Result<()> {
        if *files_scanned >= MAX_FILES_TO_SCAN {
            return Ok(());
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()), // Skip unreadable directories
        };

        for entry in entries.flatten() {
            if *files_scanned >= MAX_FILES_TO_SCAN {
                return Ok(());
            }

            let path = entry.path();
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Skip hidden directories and common non-source directories
            let name_lower = file_name_str.to_ascii_lowercase();
            if should_skip_dir_name(&name_lower) {
                continue;
            }

            if path.is_dir() {
                scan_dir(&path, counts, files_scanned)?;
            } else if path.is_file() {
                *files_scanned += 1;
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    *counts.entry(ext_str).or_insert(0) += 1;
                }
            }
        }

        Ok(())
    }

    scan_dir(root, &mut counts, &mut files_scanned)?;
    Ok(counts)
}

/// Detect if tests exist in common test directories
pub(super) fn detect_tests(root: &Path, primary_lang: &str) -> bool {
    use std::collections::VecDeque;
    use std::path::PathBuf;

    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    let mut scanned_files = 0usize;

    while let Some((dir, depth)) = queue.pop_front() {
        if scanned_files >= MAX_FILES_TO_SCAN {
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            if scanned_files >= MAX_FILES_TO_SCAN {
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let name_lower = name.to_lowercase();

            if path.is_dir() {
                if should_skip_dir_name(&name_lower) {
                    continue;
                }
                // Common test directories (any language)
                if matches!(name_lower.as_str(), "tests" | "test" | "spec" | "__tests__") {
                    return true;
                }
                if depth < MAX_SIGNATURE_SEARCH_DEPTH {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }

            if !path.is_file() {
                continue;
            }
            scanned_files += 1;

            let path_components: Vec<String> = path
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect();

            let file_name = name_lower.as_str();
            match primary_lang {
                "Rust" => {
                    if file_name == "tests.rs" || file_name.ends_with("_test.rs") {
                        return true;
                    }
                    if file_name.ends_with(".rs")
                        && path_components.windows(1).any(|w| w[0] == "tests")
                    {
                        return true;
                    }
                }
                "Python" => {
                    if (file_name.starts_with("test_") && file_name.ends_with(".py"))
                        || file_name.ends_with("_test.py")
                    {
                        return true;
                    }
                }
                "JavaScript" | "TypeScript" => {
                    if file_name.ends_with(".test.js")
                        || file_name.ends_with(".spec.js")
                        || file_name.ends_with(".test.ts")
                        || file_name.ends_with(".spec.ts")
                        || file_name.ends_with(".test.tsx")
                        || file_name.ends_with(".spec.tsx")
                    {
                        return true;
                    }
                }
                "Go" => {
                    if file_name.ends_with("_test.go") {
                        return true;
                    }
                }
                "Java" => {
                    if path_components
                        .windows(2)
                        .any(|w| w[0] == "src" && w[1] == "test")
                        || file_name.ends_with("test.java")
                    {
                        return true;
                    }
                }
                "Ruby" => {
                    if file_name.ends_with("_spec.rb") || file_name.ends_with("_test.rb") {
                        return true;
                    }
                }
                _ => {
                    if file_name.contains("test") || file_name.contains("spec") {
                        return true;
                    }
                }
            }
        }
    }

    false
}
