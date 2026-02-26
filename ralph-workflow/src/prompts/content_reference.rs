//! Content reference types for prompt templates.
//!
//! When prompt content (PROMPT, DIFF, PLAN) exceeds size limits, we reference
//! the content by file path instead of embedding it inline. This prevents
//! CLI argument limits from being exceeded while still providing agents with
//! access to all necessary information.

use std::path::{Path, PathBuf};

/// Maximum size in bytes for inline content embedding.
/// Content larger than this should be referenced by file path.
///
/// Set to 100KB which is well below:
/// - macOS `ARG_MAX` limit (~1MB)
/// - Linux per-argument limit (~128KB)
///
/// This conservative limit ensures safety across platforms.
pub const MAX_INLINE_CONTENT_SIZE: usize = 100 * 1024; // 100KB

/// Represents content that can be either inline or referenced by path.
///
/// When content is small enough, it's embedded directly in the prompt.
/// When content exceeds [`MAX_INLINE_CONTENT_SIZE`], instructions are
/// provided to the agent to read the content from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptContentReference {
    /// Content is small enough to embed inline in the prompt.
    Inline(String),
    /// Content is too large; agent should read from this workspace-relative path.
    FilePath {
        /// Workspace-relative path to the backup file containing the content.
        path: PathBuf,
        /// Human-readable description of what the content contains.
        description: String,
    },
}

impl PromptContentReference {
    /// Create a content reference, choosing inline vs path based on size.
    ///
    /// If `content.len() <= MAX_INLINE_CONTENT_SIZE`, the content is stored inline.
    /// Otherwise, a file path reference is created.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to reference
    /// * `backup_path` - Path where the content can be read if too large
    /// * `description` - Description of the content for agent instructions
    #[must_use]
    pub fn from_content(content: String, backup_path: &Path, description: &str) -> Self {
        if content.len() <= MAX_INLINE_CONTENT_SIZE {
            Self::Inline(content)
        } else {
            Self::FilePath {
                path: backup_path.to_path_buf(),
                description: description.to_string(),
            }
        }
    }

    /// Create an inline reference (for small content).
    #[must_use]
    pub const fn inline(content: String) -> Self {
        Self::Inline(content)
    }

    /// Create a file path reference (for large content).
    #[must_use]
    pub fn file_path(path: PathBuf, description: &str) -> Self {
        Self::FilePath {
            path,
            description: description.to_string(),
        }
    }

    /// Returns true if this is an inline reference.
    #[must_use]
    pub const fn is_inline(&self) -> bool {
        matches!(self, Self::Inline(_))
    }

    /// Get the content for template rendering.
    ///
    /// For inline: returns the content directly.
    /// For file path: returns instructions to read from the file.
    #[must_use]
    pub fn render_for_template(&self) -> String {
        match self {
            Self::Inline(content) => content.clone(),
            Self::FilePath { path, description } => {
                format!(
                    "[Content too large to embed - Read from: {}]\n\
                     Description: {}\n\
                     Use your file reading tools to access this file.",
                    path.display(),
                    description
                )
            }
        }
    }
}

/// Specialized reference for DIFF content.
///
/// When DIFF is too large, the pipeline prefers writing the full diff to a file so
/// agents can read it without invoking git. Some prompts (e.g., review) may include
/// git-based fallback instructions as a last resort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffContentReference {
    /// DIFF is small enough to embed inline.
    Inline(String),
    /// DIFF is too large; agent should read from a file (with optional git fallback).
    ReadFromFile {
        /// Workspace-relative path to the diff file containing the content.
        path: PathBuf,
        /// The commit hash to diff from (fallback if file is missing).
        start_commit: String,
        /// Description of why file reading is needed.
        description: String,
    },
}

impl DiffContentReference {
    /// Create a diff reference, choosing inline vs file reference based on size.
    ///
    /// If `diff_content.len() <= MAX_INLINE_CONTENT_SIZE`, the diff is stored inline.
    /// Otherwise, instructions to read from a file are provided.
    ///
    /// # Arguments
    ///
    /// * `diff_content` - The diff content
    /// * `start_commit` - The commit hash to diff from
    #[must_use]
    pub fn from_diff(diff_content: String, start_commit: &str, diff_path: &Path) -> Self {
        if diff_content.len() <= MAX_INLINE_CONTENT_SIZE {
            Self::Inline(diff_content)
        } else {
            Self::ReadFromFile {
                path: diff_path.to_path_buf(),
                start_commit: start_commit.to_string(),
                description: format!(
                    "Diff is {} bytes (exceeds {} limit)",
                    diff_content.len(),
                    MAX_INLINE_CONTENT_SIZE
                ),
            }
        }
    }

    /// Get the content for template rendering.
    ///
    /// For inline: returns the diff content directly.
    /// For file reference: returns instructions to read from the provided path,
    /// plus optional git fallback commands.
    #[must_use]
    pub fn render_for_template(&self) -> String {
        match self {
            Self::Inline(content) => content.clone(),
            Self::ReadFromFile {
                path,
                start_commit,
                description,
            } => {
                if start_commit.is_empty() {
                    format!(
                        "[DIFF too large to embed - Read from file]\n\
                         {}\n\n\
                         Read the diff from: {}\n\
                         If this file is missing or unavailable, regenerate it with git (last resort):\n\
                         - Unstaged changes: git diff\n\
                         - Staged changes:   git diff --cached\n\
                         - Untracked files:  git ls-files --others --exclude-standard\n",
                        description,
                        path.display(),
                    )
                } else {
                    format!(
                        "[DIFF too large to embed - Read from file]\n\
                         {}\n\n\
                         Read the diff from: {}\n\
                         If this file is missing or unavailable, regenerate it with git (last resort):\n\
                         - Unstaged changes: git diff {}\n\
                         - Staged changes:   git diff --cached {}\n\
                         - Untracked files:  git ls-files --others --exclude-standard\n",
                        description,
                        path.display(),
                        start_commit,
                        start_commit,
                    )
                }
            }
        }
    }

    /// Returns true if this is an inline reference.
    #[must_use]
    pub const fn is_inline(&self) -> bool {
        matches!(self, Self::Inline(_))
    }
}

/// Specialized reference for PLAN content.
///
/// When PLAN is too large, instructs the agent to read from PLAN.md
/// with optional fallback to the XML plan file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanContentReference {
    /// PLAN is small enough to embed inline.
    Inline(String),
    /// PLAN is too large; agent should read from file.
    ReadFromFile {
        /// Primary path to the plan file (usually .agent/PLAN.md), workspace-relative.
        primary_path: PathBuf,
        /// Optional fallback path if primary is missing (usually .agent/tmp/plan.xml), workspace-relative.
        fallback_path: Option<PathBuf>,
        /// Description of why file reading is needed.
        description: String,
    },
}

impl PlanContentReference {
    /// Create a plan reference, choosing inline vs file path based on size.
    ///
    /// If `plan_content.len() <= MAX_INLINE_CONTENT_SIZE`, the plan is stored inline.
    /// Otherwise, instructions to read from file are provided.
    ///
    /// # Arguments
    ///
    /// * `plan_content` - The plan content
    /// * `plan_path` - Path to the primary plan file
    /// * `xml_fallback_path` - Optional path to XML fallback
    #[must_use]
    pub fn from_plan(
        plan_content: String,
        plan_path: &Path,
        xml_fallback_path: Option<&Path>,
    ) -> Self {
        if plan_content.len() <= MAX_INLINE_CONTENT_SIZE {
            Self::Inline(plan_content)
        } else {
            Self::ReadFromFile {
                primary_path: plan_path.to_path_buf(),
                fallback_path: xml_fallback_path.map(std::path::Path::to_path_buf),
                description: format!(
                    "Plan is {} bytes (exceeds {} limit)",
                    plan_content.len(),
                    MAX_INLINE_CONTENT_SIZE
                ),
            }
        }
    }

    /// Get the content for template rendering.
    ///
    /// For inline: returns the plan content directly.
    /// For file path: returns instructions to read from the file.
    #[must_use]
    pub fn render_for_template(&self) -> String {
        match self {
            Self::Inline(content) => content.clone(),
            Self::ReadFromFile {
                primary_path,
                fallback_path,
                description,
            } => {
                let fallback_msg = fallback_path.as_ref().map_or(String::new(), |p| {
                    format!(
                        "\nIf {} is missing or empty, try reading: {}",
                        primary_path.display(),
                        p.display()
                    )
                });
                format!(
                    "[PLAN too large to embed - Read from file]\n\
                     {}\n\n\
                     Read the implementation plan from: {}{}\n\n\
                     Use your file reading tools to access the plan.",
                    description,
                    primary_path.display(),
                    fallback_msg
                )
            }
        }
    }

    /// Returns true if this is an inline reference.
    #[must_use]
    pub const fn is_inline(&self) -> bool {
        matches!(self, Self::Inline(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // PromptContentReference tests
    // =========================================================================

    #[test]
    fn test_small_content_is_inline() {
        let content = "Small content".to_string();
        let reference = PromptContentReference::from_content(
            content.clone(),
            Path::new("/backup/path"),
            "test",
        );
        assert!(reference.is_inline());
        assert_eq!(reference.render_for_template(), content);
    }

    #[test]
    fn test_large_content_becomes_file_path() {
        let content = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let reference = PromptContentReference::from_content(
            content,
            Path::new("/backup/prompt.md"),
            "User requirements",
        );
        assert!(!reference.is_inline());
        let rendered = reference.render_for_template();
        assert!(rendered.contains("/backup/prompt.md"));
        assert!(rendered.contains("User requirements"));
    }

    #[test]
    fn test_exactly_max_size_is_inline() {
        let content = "x".repeat(MAX_INLINE_CONTENT_SIZE);
        let reference = PromptContentReference::from_content(
            content.clone(),
            Path::new("/backup/path"),
            "test",
        );
        assert!(reference.is_inline());
    }

    #[test]
    fn test_empty_content_is_inline() {
        let reference =
            PromptContentReference::from_content(String::new(), Path::new("/backup"), "test");
        assert!(reference.is_inline());
        assert_eq!(reference.render_for_template(), "");
    }

    #[test]
    fn test_unicode_content_size_in_bytes() {
        // Unicode characters take multiple bytes
        // 🎉 is 4 bytes in UTF-8
        let emoji = "🎉".repeat(MAX_INLINE_CONTENT_SIZE / 4 + 1);
        let reference = PromptContentReference::from_content(emoji, Path::new("/backup"), "test");
        // Should exceed limit due to multi-byte characters
        assert!(!reference.is_inline());
    }

    #[test]
    fn test_prompt_inline_constructor() {
        let content = "Direct content".to_string();
        let reference = PromptContentReference::inline(content.clone());
        assert!(reference.is_inline());
        assert_eq!(reference.render_for_template(), content);
    }

    #[test]
    fn test_prompt_file_path_constructor() {
        let path = PathBuf::from("/path/to/file.md");
        let reference = PromptContentReference::file_path(path.clone(), "Description");
        assert!(!reference.is_inline());
        let rendered = reference.render_for_template();
        assert!(rendered.contains("/path/to/file.md"));
        assert!(rendered.contains("Description"));
    }

    // =========================================================================
    // DiffContentReference tests
    // =========================================================================

    #[test]
    fn test_small_diff_is_inline() {
        let diff = "+added line\n-removed line".to_string();
        let reference =
            DiffContentReference::from_diff(diff.clone(), "abc123", Path::new("/backup/diff.txt"));
        assert!(reference.is_inline());
        assert_eq!(reference.render_for_template(), diff);
    }

    #[test]
    fn test_large_diff_reads_from_file() {
        let diff = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let reference =
            DiffContentReference::from_diff(diff, "abc123", Path::new("/backup/diff.txt"));
        assert!(!reference.is_inline());
        let rendered = reference.render_for_template();
        assert!(rendered.contains("/backup/diff.txt"));
        assert!(rendered.contains("git diff"));
    }

    #[test]
    fn test_diff_with_empty_start_commit_includes_git_fallback() {
        let reference = DiffContentReference::from_diff(
            "x".repeat(MAX_INLINE_CONTENT_SIZE + 1),
            "",
            Path::new("/backup/diff.txt"),
        );
        let rendered = reference.render_for_template();
        assert!(rendered.contains("/backup/diff.txt"));
        assert!(rendered.contains("Unstaged changes: git diff"));
        assert!(rendered.contains("Staged changes:   git diff --cached"));
    }

    #[test]
    fn test_diff_exactly_max_size_is_inline() {
        let diff = "d".repeat(MAX_INLINE_CONTENT_SIZE);
        let reference =
            DiffContentReference::from_diff(diff.clone(), "abc", Path::new("/backup/diff.txt"));
        assert!(reference.is_inline());
        assert_eq!(reference.render_for_template(), diff);
    }

    // =========================================================================
    // PlanContentReference tests
    // =========================================================================

    #[test]
    fn test_small_plan_is_inline() {
        let plan = "# Plan\n\n1. Do thing".to_string();
        let reference =
            PlanContentReference::from_plan(plan.clone(), Path::new(".agent/PLAN.md"), None);
        assert!(reference.is_inline());
        assert_eq!(reference.render_for_template(), plan);
    }

    #[test]
    fn test_large_plan_reads_from_file() {
        let plan = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let reference = PlanContentReference::from_plan(
            plan,
            Path::new(".agent/PLAN.md"),
            Some(Path::new(".agent/tmp/plan.xml")),
        );
        assert!(!reference.is_inline());
        let rendered = reference.render_for_template();
        assert!(rendered.contains(".agent/PLAN.md"));
        assert!(rendered.contains("plan.xml"));
    }

    #[test]
    fn test_plan_without_xml_fallback() {
        let reference = PlanContentReference::from_plan(
            "x".repeat(MAX_INLINE_CONTENT_SIZE + 1),
            Path::new(".agent/PLAN.md"),
            None,
        );
        let rendered = reference.render_for_template();
        assert!(rendered.contains(".agent/PLAN.md"));
        assert!(!rendered.contains("plan.xml"));
    }

    #[test]
    fn test_plan_exactly_max_size_is_inline() {
        let plan = "p".repeat(MAX_INLINE_CONTENT_SIZE);
        let reference =
            PlanContentReference::from_plan(plan.clone(), Path::new(".agent/PLAN.md"), None);
        assert!(reference.is_inline());
        assert_eq!(reference.render_for_template(), plan);
    }
}
