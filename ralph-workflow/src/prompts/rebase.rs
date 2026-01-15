//! Rebase conflict resolution prompts.
//!
//! This module provides prompts for AI agents to resolve merge conflicts
//! that occur during rebase operations.
//!
//! # Design Note
//!
//! Per project requirements, AI agents should NOT know that we are in the
//! middle of a rebase. The prompt frames conflicts as "merge conflicts between
//! two versions" without mentioning rebase or rebasing.

#![deny(unsafe_code)]

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Structure representing a single file conflict.
#[derive(Debug, Clone)]
pub struct FileConflict {
    /// The conflict marker content from the file
    pub conflict_content: String,
    /// The current file content with conflict markers
    pub current_content: String,
}

/// Build a conflict resolution prompt for the AI agent.
///
/// This function generates a prompt that instructs the AI agent to resolve
/// merge conflicts. The prompt does NOT mention "rebase" - it frames the
/// task as resolving merge conflicts between two versions.
///
/// # Arguments
///
/// * `conflicts` - Map of file paths to their conflict information
/// * `prompt_md_content` - Optional content from PROMPT.md for task context
/// * `plan_content` - Optional content from PLAN.md for additional context
///
/// # Returns
///
/// Returns a formatted prompt string for the AI agent.
pub fn build_conflict_resolution_prompt(
    conflicts: &HashMap<String, FileConflict>,
    prompt_md_content: Option<&str>,
    plan_content: Option<&str>,
) -> String {
    let mut prompt = String::new();

    // Header - frame as "merge conflict resolution" without mentioning rebase
    prompt.push_str("# MERGE CONFLICT RESOLUTION\n\n");
    prompt.push_str(
        "There are merge conflicts that need to be resolved. Below are the files \
         with conflicts, showing both versions of the conflicting changes.\n\n",
    );

    // Add task context from PROMPT.md if available
    if let Some(prompt_md) = prompt_md_content {
        prompt.push_str("## Task Context\n\n");
        prompt.push_str("The user was working on the following task (from PROMPT.md):\n\n");
        prompt.push_str("```\n");
        prompt.push_str(prompt_md);
        prompt.push_str("\n```\n\n");
    }

    // Add plan context from PLAN.md if available
    if let Some(plan) = plan_content {
        prompt.push_str("## Implementation Plan\n\n");
        prompt.push_str("The following plan was being implemented (from PLAN.md):\n\n");
        prompt.push_str("```\n");
        prompt.push_str(plan);
        prompt.push_str("\n```\n\n");
    }

    // Add conflict resolution instructions
    prompt.push_str("## Conflict Resolution Instructions\n\n");
    prompt.push_str(
        "For each conflicted file below:\n\
         1. Review both versions of the changes (the 'ours' and 'theirs' sections)\n\
         2. Intelligently merge the changes, considering:\n\
         - The task context from PROMPT.md above\n\
         - The implementation plan from PLAN.md if available\n\
         - The intent of both versions\n\
         - Code correctness and consistency\n\
         3. Produce the final merged file content WITHOUT conflict markers\n\n",
    );

    prompt.push_str(
        "IMPORTANT: Your output must include the complete resolved file contents. \
         Do not include conflict markers (<<<<<<<, =======, >>>>>>>) in your output.\n\n",
    );

    // List all conflicted files
    prompt.push_str("## Conflicted Files\n\n");
    for (path, conflict) in conflicts.iter() {
        prompt.push_str(&format!("### {}\n\n", path));
        prompt.push_str("Current state (with conflict markers):\n\n");
        prompt.push_str("```");
        prompt.push_str(&get_language_marker(path));
        prompt.push_str("\n");
        prompt.push_str(&conflict.current_content);
        prompt.push_str("\n```\n\n");

        if !conflict.conflict_content.is_empty() {
            prompt.push_str("Conflict sections:\n\n");
            prompt.push_str("```\n");
            prompt.push_str(&conflict.conflict_content);
            prompt.push_str("\n```\n\n");
        }
    }

    // Output format instructions
    prompt.push_str("## Output Format\n\n");
    prompt.push_str("Provide your response as a JSON object with the following structure:\n\n");
    prompt.push_str(
        "```json\n\
         {\n\
           \"resolved_files\": {\n\
             \"path/to/file1\": \"<complete resolved file content>\",\n\
             \"path/to/file2\": \"<complete resolved file content>\"\n\
           }\n\
         }\n\
         ```\n\n",
    );

    prompt.push_str(
        "Each resolved file should contain the COMPLETE file content, not just \
         the changed sections. The content must be free of conflict markers.\n\n",
    );

    prompt.push_str(
        "If you cannot resolve a particular conflict, you may mark it for manual \
         resolution by omitting it from the resolved_files object. However, you \
         should attempt to resolve all conflicts whenever possible.\n",
    );

    prompt
}

/// Get a language marker for syntax highlighting based on file extension.
fn get_language_marker(path: &str) -> String {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "tsx" => "typescript",
        "jsx" => "javascript",
        "go" => "go",
        "java" => "java",
        "c" => "c",
        "h" => "c",
        "cpp" => "cpp",
        "hpp" => "cpp",
        "cc" => "cpp",
        "cxx" => "cpp",
        "cs" => "csharp",
        "php" => "php",
        "rb" => "ruby",
        "swift" => "swift",
        "kt" => "kotlin",
        "scala" => "scala",
        "sh" => "bash",
        "bash" => "bash",
        "zsh" => "bash",
        "fish" => "fish",
        "yaml" => "yaml",
        "yml" => "yaml",
        "json" => "json",
        "toml" => "toml",
        "md" => "markdown",
        "markdown" => "markdown",
        "txt" => "text",
        "html" => "html",
        "css" => "css",
        "scss" => "scss",
        "less" => "css",
        "xml" => "xml",
        "sql" => "sql",
        _ => "",
    }
    .to_string()
}

/// Collect conflict information from all conflicted files.
///
/// This function reads all conflicted files and builds a map of
/// file paths to their conflict information.
///
/// # Arguments
///
/// * `conflicted_paths` - List of paths to conflicted files
///
/// # Returns
///
/// Returns `Ok(HashMap)` mapping file paths to conflict information,
/// or an error if a file cannot be read.
pub fn collect_conflict_info(
    conflicted_paths: &[String],
) -> std::io::Result<HashMap<String, FileConflict>> {
    let mut conflicts = HashMap::new();

    for path in conflicted_paths {
        // Read the current file content with conflict markers
        let current_content = fs::read_to_string(path)?;

        // Extract conflict markers
        let conflict_content = crate::git_helpers::get_conflict_markers_for_file(Path::new(path))?;

        conflicts.insert(
            path.clone(),
            FileConflict {
                conflict_content,
                current_content,
            },
        );
    }

    Ok(conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_conflict_resolution_prompt_no_mentions_rebase() {
        let conflicts = HashMap::new();
        let prompt = build_conflict_resolution_prompt(&conflicts, None, None);

        // The prompt should NOT mention "rebase" or "rebasing"
        assert!(!prompt.to_lowercase().contains("rebase"));
        assert!(!prompt.to_lowercase().contains("rebasing"));

        // But it SHOULD mention "merge conflict"
        assert!(prompt.to_lowercase().contains("merge conflict"));
    }

    #[test]
    fn test_build_conflict_resolution_prompt_with_context() {
        let mut conflicts = HashMap::new();
        conflicts.insert(
            "test.rs".to_string(),
            FileConflict {
                conflict_content: "<<<<<<< ours\nfn foo() {}\n=======\nfn bar() {}\n>>>>>>> theirs"
                    .to_string(),
                current_content: "<<<<<<< ours\nfn foo() {}\n=======\nfn bar() {}\n>>>>>>> theirs"
                    .to_string(),
            },
        );

        let prompt_md = "Add a new feature";
        let plan = "1. Create foo function\n2. Create bar function";

        let prompt = build_conflict_resolution_prompt(&conflicts, Some(prompt_md), Some(plan));

        // Should include context from PROMPT.md
        assert!(prompt.contains("Add a new feature"));

        // Should include context from PLAN.md
        assert!(prompt.contains("Create foo function"));
        assert!(prompt.contains("Create bar function"));

        // Should include the conflicted file
        assert!(prompt.contains("test.rs"));

        // Should NOT mention rebase
        assert!(!prompt.to_lowercase().contains("rebase"));
    }

    #[test]
    fn test_get_language_marker() {
        assert_eq!(get_language_marker("file.rs"), "rust");
        assert_eq!(get_language_marker("file.py"), "python");
        assert_eq!(get_language_marker("file.js"), "javascript");
        assert_eq!(get_language_marker("file.ts"), "typescript");
        assert_eq!(get_language_marker("file.go"), "go");
        assert_eq!(get_language_marker("file.java"), "java");
        assert_eq!(get_language_marker("file.cpp"), "cpp");
        assert_eq!(get_language_marker("file.md"), "markdown");
        assert_eq!(get_language_marker("file.yaml"), "yaml");
        assert_eq!(get_language_marker("file.unknown"), "");
    }
}
