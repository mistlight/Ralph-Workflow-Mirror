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

use crate::prompts::template_context::TemplateContext;
use crate::prompts::template_engine::Template;
use std::collections::HashMap;
use std::fmt::Write;
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
#[cfg(test)]
pub fn build_conflict_resolution_prompt(
    conflicts: &HashMap<String, FileConflict>,
    prompt_md_content: Option<&str>,
    plan_content: Option<&str>,
) -> String {
    let template_content = include_str!("templates/conflict_resolution.txt");
    let template = Template::new(template_content);

    let context = format_context_section(prompt_md_content, plan_content);
    let conflicts_section = format_conflicts_section(conflicts);

    let variables = HashMap::from([
        ("CONTEXT", context),
        ("CONFLICTS", conflicts_section.clone()),
    ]);

    template.render(&variables).unwrap_or_else(|e| {
        eprintln!("Warning: Failed to render conflict resolution template: {e}");
        // Use fallback template
        let fallback_template_content = include_str!("templates/conflict_resolution_fallback.txt");
        let fallback_template = Template::new(fallback_template_content);
        fallback_template.render(&variables).unwrap_or_else(|e| {
            eprintln!("Critical: Failed to render fallback template: {e}");
            // Last resort: minimal emergency prompt - conflicts_section is captured from closure
            format!(
                "# MERGE CONFLICT RESOLUTION\n\nResolve these conflicts:\n\n{}",
                &conflicts_section
            )
        })
    })
}

/// Build a conflict resolution prompt using template registry.
///
/// This version uses the template registry which supports user template overrides.
/// It's the recommended way to generate prompts going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `conflicts` - Map of file paths to their conflict information
/// * `prompt_md_content` - Optional content from PROMPT.md for task context
/// * `plan_content` - Optional content from PLAN.md for additional context
pub fn build_conflict_resolution_prompt_with_context(
    context: &TemplateContext,
    conflicts: &HashMap<String, FileConflict>,
    prompt_md_content: Option<&str>,
    plan_content: Option<&str>,
) -> String {
    let template_content = context
        .registry()
        .get_template("conflict_resolution")
        .unwrap_or_else(|_| include_str!("templates/conflict_resolution.txt").to_string());
    let template = Template::new(&template_content);

    let ctx_section = format_context_section(prompt_md_content, plan_content);
    let conflicts_section = format_conflicts_section(conflicts);

    let variables = HashMap::from([
        ("CONTEXT", ctx_section),
        ("CONFLICTS", conflicts_section.clone()),
    ]);

    template.render(&variables).unwrap_or_else(|e| {
        eprintln!("Warning: Failed to render conflict resolution template: {e}");
        // Use fallback template
        let fallback_template_content = context
            .registry()
            .get_template("conflict_resolution_fallback")
            .unwrap_or_else(|_| {
                include_str!("templates/conflict_resolution_fallback.txt").to_string()
            });
        let fallback_template = Template::new(&fallback_template_content);
        fallback_template.render(&variables).unwrap_or_else(|e| {
            eprintln!("Critical: Failed to render fallback template: {e}");
            // Last resort: minimal emergency prompt - conflicts_section is captured from closure
            format!(
                "# MERGE CONFLICT RESOLUTION\n\nResolve these conflicts:\n\n{}",
                &conflicts_section
            )
        })
    })
}

/// Format the context section with PROMPT.md and PLAN.md content.
///
/// This helper builds the context section that gets injected into the
/// {{CONTEXT}} template variable.
fn format_context_section(prompt_md_content: Option<&str>, plan_content: Option<&str>) -> String {
    let mut context = String::new();

    // Add task context from PROMPT.md if available
    if let Some(prompt_md) = prompt_md_content {
        context.push_str("## Task Context\n\n");
        context.push_str("The user was working on the following task:\n\n");
        context.push_str("```\n");
        context.push_str(prompt_md);
        context.push_str("\n```\n\n");
    }

    // Add plan context from PLAN.md if available
    if let Some(plan) = plan_content {
        context.push_str("## Implementation Plan\n\n");
        context.push_str("The following plan was being implemented:\n\n");
        context.push_str("```\n");
        context.push_str(plan);
        context.push_str("\n```\n\n");
    }

    context
}

/// Format the conflicts section for all conflicted files.
///
/// This helper builds the conflicts section that gets injected into the
/// {{CONFLICTS}} template variable.
fn format_conflicts_section(conflicts: &HashMap<String, FileConflict>) -> String {
    let mut section = String::new();

    for (path, conflict) in conflicts {
        writeln!(section, "### {path}\n\n").unwrap();
        section.push_str("Current state (with conflict markers):\n\n");
        section.push_str("```");
        section.push_str(&get_language_marker(path));
        section.push('\n');
        section.push_str(&conflict.current_content);
        section.push_str("\n```\n\n");

        if !conflict.conflict_content.is_empty() {
            section.push_str("Conflict sections:\n\n");
            section.push_str("```\n");
            section.push_str(&conflict.conflict_content);
            section.push_str("\n```\n\n");
        }
    }

    section
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
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" | "cxx" => "cpp",
        "cs" => "csharp",
        "php" => "php",
        "rb" => "ruby",
        "swift" => "swift",
        "kt" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "bash",
        "fish" => "fish",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "txt" => "text",
        "html" => "html",
        "css" | "scss" | "less" => "css",
        "xml" => "xml",
        "sql" => "sql",
        _ => "",
    }
    .to_string()
}

/// Information about divergent branches for enhanced conflict resolution.
#[derive(Debug, Clone)]
pub struct BranchInfo {
    /// The current branch name
    pub current_branch: String,
    /// The upstream/target branch name
    pub upstream_branch: String,
    /// Recent commit messages from current branch
    pub current_commits: Vec<String>,
    /// Recent commit messages from upstream branch
    pub upstream_commits: Vec<String>,
    /// Number of diverging commits
    pub diverging_count: usize,
}

/// Build a conflict resolution prompt with enhanced branch context.
///
/// This version provides richer context about the branches involved in the conflict,
/// including recent commit history and divergence information.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `conflicts` - Map of file paths to their conflict information
/// * `branch_info` - Optional branch information for enhanced context
/// * `prompt_md_content` - Optional content from PROMPT.md for task context
/// * `plan_content` - Optional content from PLAN.md for additional context
pub fn build_enhanced_conflict_resolution_prompt(
    context: &TemplateContext,
    conflicts: &HashMap<String, FileConflict>,
    branch_info: Option<&BranchInfo>,
    prompt_md_content: Option<&str>,
    plan_content: Option<&str>,
) -> String {
    let template_content = context
        .registry()
        .get_template("conflict_resolution")
        .unwrap_or_else(|_| include_str!("templates/conflict_resolution.txt").to_string());
    let template = Template::new(&template_content);

    let mut ctx_section = format_context_section(prompt_md_content, plan_content);

    // Add branch information if available
    if let Some(info) = branch_info {
        ctx_section.push_str(&format_branch_info_section(info));
    }

    let conflicts_section = format_conflicts_section(conflicts);

    let variables = HashMap::from([
        ("CONTEXT", ctx_section),
        ("CONFLICTS", conflicts_section.clone()),
    ]);

    template.render(&variables).unwrap_or_else(|e| {
        eprintln!("Warning: Failed to render conflict resolution template: {e}");
        // Use fallback template
        let fallback_template_content = context
            .registry()
            .get_template("conflict_resolution_fallback")
            .unwrap_or_else(|_| {
                include_str!("templates/conflict_resolution_fallback.txt").to_string()
            });
        let fallback_template = Template::new(&fallback_template_content);
        fallback_template.render(&variables).unwrap_or_else(|e| {
            eprintln!("Critical: Failed to render fallback template: {e}");
            // Last resort: minimal emergency prompt - conflicts_section is captured from closure
            format!(
                "# MERGE CONFLICT RESOLUTION\n\nResolve these conflicts:\n\n{}",
                &conflicts_section
            )
        })
    })
}

/// Format branch information for context section.
///
/// This helper builds a branch information section that gets injected
/// into the context for AI conflict resolution.
fn format_branch_info_section(info: &BranchInfo) -> String {
    let mut section = String::new();

    section.push_str("## Branch Information\n\n");
    section.push_str(&format!(
        "- **Current branch**: `{}`\n",
        info.current_branch
    ));
    section.push_str(&format!(
        "- **Target branch**: `{}`\n",
        info.upstream_branch
    ));
    section.push_str(&format!(
        "- **Diverging commits**: {}\n\n",
        info.diverging_count
    ));

    if !info.current_commits.is_empty() {
        section.push_str("### Recent commits on current branch:\n\n");
        for (i, msg) in info.current_commits.iter().enumerate().take(5) {
            section.push_str(&format!("{}. {}\n", i + 1, msg));
        }
        section.push('\n');
    }

    if !info.upstream_commits.is_empty() {
        section.push_str("### Recent commits on target branch:\n\n");
        for (i, msg) in info.upstream_commits.iter().enumerate().take(5) {
            section.push_str(&format!("{}. {}\n", i + 1, msg));
        }
        section.push('\n');
    }

    section
}

/// Collect branch information for conflict resolution.
///
/// Queries git to gather information about the branches involved in the conflict.
///
/// # Arguments
///
/// * `upstream_branch` - The name of the upstream/target branch
///
/// # Returns
///
/// Returns `Ok(BranchInfo)` with the gathered information, or an error if git operations fail.
pub fn collect_branch_info(upstream_branch: &str) -> std::io::Result<BranchInfo> {
    use std::process::Command;

    // Get current branch name
    let current_branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|e| std::io::Error::other(format!("git rev-parse failed: {e}")))?;

    let current_branch = String::from_utf8_lossy(&current_branch.stdout)
        .trim()
        .to_string();

    // Get recent commits from current branch
    let current_log = Command::new("git")
        .args(["log", "--oneline", "-10", "HEAD"])
        .output()
        .map_err(|e| std::io::Error::other(format!("git log failed: {e}")))?;

    let current_commits: Vec<String> = String::from_utf8_lossy(&current_log.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();

    // Get recent commits from upstream branch
    let upstream_log = Command::new("git")
        .args(["log", "--oneline", "-10", upstream_branch])
        .output()
        .map_err(|e| std::io::Error::other(format!("git log failed: {e}")))?;

    let upstream_commits: Vec<String> = String::from_utf8_lossy(&upstream_log.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();

    // Count diverging commits
    let diverging = Command::new("git")
        .args([
            "rev-list",
            "--count",
            "--left-right",
            &format!("HEAD...{upstream_branch}"),
        ])
        .output()
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("git rev-list failed: {e}"),
            )
        })?;

    let diverging_count = String::from_utf8_lossy(&diverging.stdout)
        .trim()
        .split_whitespace()
        .map(|s| s.parse::<usize>().unwrap_or(0))
        .sum::<usize>();

    Ok(BranchInfo {
        current_branch,
        upstream_branch: upstream_branch.to_string(),
        current_commits,
        upstream_commits,
        diverging_count,
    })
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

    #[test]
    fn test_format_context_section_with_both() {
        let prompt_md = "Test prompt";
        let plan = "Test plan";
        let context = format_context_section(Some(prompt_md), Some(plan));

        assert!(context.contains("## Task Context"));
        assert!(context.contains("Test prompt"));
        assert!(context.contains("## Implementation Plan"));
        assert!(context.contains("Test plan"));
    }

    #[test]
    fn test_format_context_section_with_prompt_only() {
        let prompt_md = "Test prompt";
        let context = format_context_section(Some(prompt_md), None);

        assert!(context.contains("## Task Context"));
        assert!(context.contains("Test prompt"));
        assert!(!context.contains("## Implementation Plan"));
    }

    #[test]
    fn test_format_context_section_with_plan_only() {
        let plan = "Test plan";
        let context = format_context_section(None, Some(plan));

        assert!(!context.contains("## Task Context"));
        assert!(context.contains("## Implementation Plan"));
        assert!(context.contains("Test plan"));
    }

    #[test]
    fn test_format_context_section_empty() {
        let context = format_context_section(None, None);
        assert!(context.is_empty());
    }

    #[test]
    fn test_format_conflicts_section() {
        let mut conflicts = HashMap::new();
        conflicts.insert(
            "src/test.rs".to_string(),
            FileConflict {
                conflict_content: "<<<<<<< ours\nx\n=======\ny\n>>>>>>> theirs".to_string(),
                current_content: "<<<<<<< ours\nx\n=======\ny\n>>>>>>> theirs".to_string(),
            },
        );

        let section = format_conflicts_section(&conflicts);

        assert!(section.contains("### src/test.rs"));
        assert!(section.contains("Current state (with conflict markers)"));
        assert!(section.contains("```rust"));
        assert!(section.contains("<<<<<<< ours"));
        assert!(section.contains("Conflict sections"));
    }

    #[test]
    fn test_template_is_used() {
        // Verify that the template-based approach produces valid output
        let conflicts = HashMap::new();
        let prompt = build_conflict_resolution_prompt(&conflicts, None, None);

        // Should contain key sections from the template
        assert!(prompt.contains("# MERGE CONFLICT RESOLUTION"));
        assert!(prompt.contains("## Conflict Resolution Instructions"));
        assert!(prompt.contains("## Optional JSON Output Format"));
        assert!(prompt.contains("resolved_files"));
    }

    #[test]
    fn test_build_conflict_resolution_prompt_with_registry_context() {
        let context = TemplateContext::default();
        let conflicts = HashMap::new();
        let prompt =
            build_conflict_resolution_prompt_with_context(&context, &conflicts, None, None);

        // The prompt should NOT mention "rebase" or "rebasing"
        assert!(!prompt.to_lowercase().contains("rebase"));
        assert!(!prompt.to_lowercase().contains("rebasing"));

        // But it SHOULD mention "merge conflict"
        assert!(prompt.to_lowercase().contains("merge conflict"));
    }

    #[test]
    fn test_build_conflict_resolution_prompt_with_registry_context_and_content() {
        let context = TemplateContext::default();
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

        let prompt = build_conflict_resolution_prompt_with_context(
            &context,
            &conflicts,
            Some(prompt_md),
            Some(plan),
        );

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
    fn test_registry_context_based_matches_regular() {
        let context = TemplateContext::default();
        let mut conflicts = HashMap::new();
        conflicts.insert(
            "test.rs".to_string(),
            FileConflict {
                conflict_content: "conflict".to_string(),
                current_content: "current".to_string(),
            },
        );

        let regular = build_conflict_resolution_prompt(&conflicts, Some("prompt"), Some("plan"));
        let with_context = build_conflict_resolution_prompt_with_context(
            &context,
            &conflicts,
            Some("prompt"),
            Some("plan"),
        );
        // Both should produce equivalent output
        assert_eq!(regular, with_context);
    }

    #[test]
    fn test_branch_info_struct_exists() {
        let info = ralph_workflow::prompts::rebase::BranchInfo {
            current_branch: "feature".to_string(),
            upstream_branch: "main".to_string(),
            current_commits: vec!["abc123 feat: add thing".to_string()],
            upstream_commits: vec!["def456 fix: bug".to_string()],
            diverging_count: 5,
        };
        assert_eq!(info.current_branch, "feature");
        assert_eq!(info.diverging_count, 5);
    }

    #[test]
    fn test_format_branch_info_section() {
        let info = ralph_workflow::prompts::rebase::BranchInfo {
            current_branch: "feature".to_string(),
            upstream_branch: "main".to_string(),
            current_commits: vec!["abc123 feat: add thing".to_string()],
            upstream_commits: vec!["def456 fix: bug".to_string()],
            diverging_count: 5,
        };

        let section = ralph_workflow::prompts::rebase::format_branch_info_section(&info);

        assert!(section.contains("Branch Information"));
        assert!(section.contains("feature"));
        assert!(section.contains("main"));
        assert!(section.contains("5"));
        assert!(section.contains("abc123"));
        assert!(section.contains("def456"));
    }

    #[test]
    fn test_enhanced_prompt_with_branch_info() {
        let context = TemplateContext::default();
        let mut conflicts = HashMap::new();
        conflicts.insert(
            "test.rs".to_string(),
            FileConflict {
                conflict_content: "conflict".to_string(),
                current_content: "current".to_string(),
            },
        );

        let branch_info = ralph_workflow::prompts::rebase::BranchInfo {
            current_branch: "feature".to_string(),
            upstream_branch: "main".to_string(),
            current_commits: vec!["abc123 my change".to_string()],
            upstream_commits: vec!["def456 their change".to_string()],
            diverging_count: 3,
        };

        let prompt = ralph_workflow::prompts::rebase::build_enhanced_conflict_resolution_prompt(
            &context,
            &conflicts,
            Some(&branch_info),
            None,
            None,
        );

        // Should include branch information
        assert!(prompt.contains("Branch Information"));
        assert!(prompt.contains("feature"));
        assert!(prompt.contains("main"));
        assert!(prompt.contains("3")); // diverging count

        // Should NOT mention rebase
        assert!(!prompt.to_lowercase().contains("rebase"));
    }
}
