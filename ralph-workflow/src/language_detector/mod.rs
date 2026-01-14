//! Language and Stack Detection Module
//!
//! Detects the primary technology stack of a repository by analyzing file extensions,
//! configuration files, and common patterns. This enables language-specific review
//! guidance without requiring an LLM.
//!
//! The detection is fast (< 100ms typically) and uses heuristics based on:
//! - File extension counts
//! - Signature files (Cargo.toml, package.json, etc.)
//! - Framework indicators in config files
//!
//! # Module Structure
//!
//! - [`extensions`] - Extension to language mapping
//! - [`signatures`] - Signature file detection for frameworks and package managers
//! - [`scanner`] - File system scanning utilities

#![deny(unsafe_code)]

mod extensions;
mod scanner;
mod signatures;

use std::collections::HashMap;
use std::io;
use std::path::Path;

pub use extensions::extension_to_language;
use extensions::is_non_primary_language;
use scanner::{count_extensions, detect_tests};
use signatures::detect_signature_files;

/// Maximum number of secondary languages to include in the stack summary.
///
/// Polyglot repos commonly have more than 3 relevant languages (e.g. PHP + TS + JS + SQL),
/// but we still cap this to keep prompts/banners readable.
const MAX_SECONDARY_LANGUAGES: usize = 6;

/// Minimum file count to consider a language as present
const MIN_FILES_FOR_DETECTION: usize = 1;

/// Represents the detected technology stack of a project
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStack {
    /// Primary programming language (most prevalent)
    pub(crate) primary_language: String,
    /// Secondary languages used in the project
    pub(crate) secondary_languages: Vec<String>,
    /// Detected frameworks (React, Django, Rails, etc.)
    pub(crate) frameworks: Vec<String>,
    /// Whether the project appears to have tests
    pub(crate) has_tests: bool,
    /// Detected test framework (if any)
    pub(crate) test_framework: Option<String>,
    /// Package manager detected
    pub(crate) package_manager: Option<String>,
}

impl Default for ProjectStack {
    fn default() -> Self {
        Self {
            primary_language: "Unknown".to_string(),
            secondary_languages: Vec::new(),
            frameworks: Vec::new(),
            has_tests: false,
            test_framework: None,
            package_manager: None,
        }
    }
}

impl ProjectStack {
    /// Returns true if the project uses Rust
    pub(crate) fn is_rust(&self) -> bool {
        self.primary_language == "Rust" || self.secondary_languages.iter().any(|l| l == "Rust")
    }

    /// Returns true if the project uses Python
    pub(crate) fn is_python(&self) -> bool {
        self.primary_language == "Python" || self.secondary_languages.iter().any(|l| l == "Python")
    }

    /// Returns true if the project uses JavaScript or TypeScript
    pub(crate) fn is_javascript_or_typescript(&self) -> bool {
        matches!(self.primary_language.as_str(), "JavaScript" | "TypeScript")
            || self
                .secondary_languages
                .iter()
                .any(|l| l == "JavaScript" || l == "TypeScript")
    }

    /// Returns true if the project uses Go
    pub(crate) fn is_go(&self) -> bool {
        self.primary_language == "Go" || self.secondary_languages.iter().any(|l| l == "Go")
    }

    /// Format as a summary string for display
    pub(crate) fn summary(&self) -> String {
        let mut parts = vec![self.primary_language.clone()];

        if !self.secondary_languages.is_empty() {
            parts.push(format!("(+{})", self.secondary_languages.join(", ")));
        }

        if !self.frameworks.is_empty() {
            parts.push(format!("[{}]", self.frameworks.join(", ")));
        }

        if self.has_tests {
            if let Some(ref tf) = self.test_framework {
                parts.push(format!("tests:{tf}"));
            } else {
                parts.push("tests:yes".to_string());
            }
        }

        parts.join(" ")
    }
}

/// Detect the project stack for a given repository root
pub fn detect_stack(root: &Path) -> io::Result<ProjectStack> {
    // Count file extensions
    let extension_counts = count_extensions(root)?;

    // Convert extensions to languages and aggregate
    let mut language_counts: HashMap<&str, usize> = HashMap::new();
    for (ext, count) in &extension_counts {
        if let Some(lang) = extension_to_language(ext) {
            *language_counts.entry(lang).or_insert(0) += count;
        }
    }

    // Sort languages by count (descending)
    let mut language_vec: Vec<_> = language_counts
        .into_iter()
        .filter(|(_, count)| *count >= MIN_FILES_FOR_DETECTION)
        .collect();
    language_vec.sort_by(|a, b| b.1.cmp(&a.1));

    // Determine primary and secondary languages.
    //
    // Prefer "code" languages as primary when present, even if the repo contains lots of
    // config/markup files (YAML/JSON/CSS/etc).
    let primary_language = language_vec
        .iter()
        .find(|(lang, _)| !is_non_primary_language(lang))
        .or_else(|| language_vec.first())
        .map_or_else(|| "Unknown".to_string(), |(lang, _)| (*lang).to_string());

    let secondary_languages: Vec<String> = language_vec
        .iter()
        .filter(|(lang, _)| *lang != primary_language.as_str())
        .take(MAX_SECONDARY_LANGUAGES)
        .map(|(lang, _)| (*lang).to_string())
        .collect();

    // Detect signature files for frameworks and test frameworks
    let (frameworks, test_framework, package_manager) = detect_signature_files(root);

    // Detect if tests exist
    let has_tests = test_framework.is_some() || detect_tests(root, &primary_language);

    Ok(ProjectStack {
        primary_language,
        secondary_languages,
        frameworks,
        has_tests,
        test_framework,
        package_manager,
    })
}

/// Detect stack and return a summary string (for display in banner)
pub fn detect_stack_summary(root: &Path) -> String {
    match detect_stack(root) {
        Ok(stack) => stack.summary(),
        Err(_) => "Unknown".to_string(),
    }
}

#[cfg(test)]
mod tests;
