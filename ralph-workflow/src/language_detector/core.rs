//! Core types and utilities for signature detection
//!
//! Shared types and helper functions used across language-specific detection modules.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use super::scanner::{should_skip_dir_name, MAX_FILES_TO_SCAN, MAX_SIGNATURE_SEARCH_DEPTH};

/// Maximum number of signature files to collect (across all types).
pub const MAX_SIGNATURE_FILES: usize = 50;

/// Helper to push unique values to a vector.
pub fn push_unique(vec: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !vec.iter().any(|v| v == &value) {
        vec.push(value);
    }
}

/// Combine multiple items into a single string.
pub fn combine_unique(items: &[String]) -> Option<String> {
    match items.len() {
        0 => None,
        1 => Some(items[0].clone()),
        _ => Some(
            items
                .iter()
                .map(std::string::String::as_str)
                .collect::<Vec<_>>()
                .join(" + "),
        ),
    }
}

/// Container for signature files found during scanning.
#[derive(Default)]
pub struct SignatureFiles {
    pub by_name_lower: HashMap<String, Vec<PathBuf>>,
    pub by_extension_lower: HashMap<String, Vec<PathBuf>>,
}

/// Collect signature files from the repository.
pub fn collect_signature_files(root: &Path) -> SignatureFiles {
    let mut targets: HashSet<String> = HashSet::new();
    for name in [
        "cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "pipfile",
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "gemfile",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "composer.json",
        "mix.exs",
        "pubspec.yaml",
    ] {
        let _ = targets.insert(name.to_string());
    }

    let mut result = SignatureFiles::default();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    let mut scanned_entries: usize = 0;
    let mut collected: usize = 0;

    while let Some((dir, depth)) = queue.pop_front() {
        if scanned_entries >= MAX_FILES_TO_SCAN || collected >= MAX_SIGNATURE_FILES {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            if scanned_entries >= MAX_FILES_TO_SCAN || collected >= MAX_SIGNATURE_FILES {
                break;
            }
            scanned_entries += 1;

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let name_lower = name.to_lowercase();

            if path.is_dir() {
                if should_skip_dir_name(&name_lower) {
                    continue;
                }
                if depth < MAX_SIGNATURE_SEARCH_DEPTH {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }

            if !path.is_file() {
                continue;
            }

            if targets.contains(&name_lower) {
                result
                    .by_name_lower
                    .entry(name_lower.clone())
                    .or_default()
                    .push(path.clone());
                collected += 1;
            }

            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if ext_lower == "csproj" {
                    result
                        .by_extension_lower
                        .entry(ext_lower)
                        .or_default()
                        .push(path.clone());
                    collected += 1;
                }
            }
        }
    }

    result
}
