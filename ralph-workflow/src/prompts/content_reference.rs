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
/// - macOS ARG_MAX limit (~1MB)
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
    /// Content is too large; agent should read from this absolute path.
    FilePath {
        /// Absolute path to the backup file containing the content.
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
    pub fn inline(content: String) -> Self {
        Self::Inline(content)
    }

    /// Create a file path reference (for large content).
    pub fn file_path(path: PathBuf, description: &str) -> Self {
        Self::FilePath {
            path,
            description: description.to_string(),
        }
    }

    /// Returns true if this is an inline reference.
    pub fn is_inline(&self) -> bool {
        matches!(self, Self::Inline(_))
    }

    /// Get the content for template rendering.
    ///
    /// For inline: returns the content directly.
    /// For file path: returns instructions to read from the file.
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
/// When DIFF is too large, instructs the agent to use `git diff` command
/// instead of embedding the diff content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffContentReference {
    /// DIFF is small enough to embed inline.
    Inline(String),
    /// DIFF is too large; agent should run git diff from start_commit.
    UseGitDiff {
        /// The commit hash to diff from (usually the start of the session).
        start_commit: String,
        /// Description of why git diff is needed.
        description: String,
    },
}

impl DiffContentReference {
    /// Create a diff reference, choosing inline vs git command based on size.
    ///
    /// If `diff_content.len() <= MAX_INLINE_CONTENT_SIZE`, the diff is stored inline.
    /// Otherwise, instructions to use `git diff` are provided.
    ///
    /// # Arguments
    ///
    /// * `diff_content` - The diff content
    /// * `start_commit` - The commit hash to diff from
    pub fn from_diff(diff_content: String, start_commit: &str) -> Self {
        if diff_content.len() <= MAX_INLINE_CONTENT_SIZE {
            Self::Inline(diff_content)
        } else {
            Self::UseGitDiff {
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
    /// For git diff: returns instructions to run the git command.
    pub fn render_for_template(&self) -> String {
        match self {
            Self::Inline(content) => content.clone(),
            Self::UseGitDiff {
                start_commit,
                description,
            } => {
                format!(
                    "[DIFF too large to embed - Use git diff instead]\n\
                     {}\n\n\
                     To see the changes, run:\n\
                     git diff {}..HEAD\n\n\
                     This shows all changes since the start of this session.",
                    description, start_commit
                )
            }
        }
    }

    /// Returns true if this is an inline reference.
    pub fn is_inline(&self) -> bool {
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
        /// Primary path to the plan file (usually .agent/PLAN.md).
        primary_path: PathBuf,
        /// Optional fallback path if primary is missing (usually .agent/tmp/plan.xml).
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
                fallback_path: xml_fallback_path.map(|p| p.to_path_buf()),
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
    pub fn is_inline(&self) -> bool {
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
        let reference = DiffContentReference::from_diff(diff.clone(), "abc123");
        assert!(reference.is_inline());
        assert_eq!(reference.render_for_template(), diff);
    }

    #[test]
    fn test_large_diff_uses_git_command() {
        let diff = "x".repeat(MAX_INLINE_CONTENT_SIZE + 1);
        let reference = DiffContentReference::from_diff(diff, "abc123");
        assert!(!reference.is_inline());
        let rendered = reference.render_for_template();
        assert!(rendered.contains("git diff abc123..HEAD"));
    }

    #[test]
    fn test_diff_with_empty_start_commit() {
        let reference =
            DiffContentReference::from_diff("x".repeat(MAX_INLINE_CONTENT_SIZE + 1), "");
        let rendered = reference.render_for_template();
        assert!(rendered.contains("git diff ..HEAD"));
    }

    #[test]
    fn test_diff_exactly_max_size_is_inline() {
        let diff = "d".repeat(MAX_INLINE_CONTENT_SIZE);
        let reference = DiffContentReference::from_diff(diff.clone(), "abc");
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
