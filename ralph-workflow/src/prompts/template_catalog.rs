//! Embedded Template Catalog
//!
//! Central registry of all embedded templates with metadata.
//!
//! This module provides a single source of truth for all embedded templates,
//! consolidating scattered `include_str!` calls across the codebase.
//!
//! # Template Categories
//!
//! - **Commit**: Templates for commit message generation
//! - **Developer**: Templates for developer agent prompts
//! - **Reviewer**: Templates for code review prompts
//! - **`FixMode`**: Templates for fix mode prompts
//! - **Rebase**: Templates for conflict resolution prompts

use std::collections::HashMap;

/// Template category for organization and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum TemplateCategory {
    /// Commit message generation templates
    Commit,
    /// Developer agent prompts
    Developer,
    /// Code review prompts
    Reviewer,
    /// Fix mode prompts
    FixMode,
    /// Rebase/conflict resolution prompts
    Rebase,
}

/// Metadata about an embedded template.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EmbeddedTemplate {
    /// Template name (used for lookup and user override files)
    pub name: &'static str,
    /// Template content
    pub content: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Template category
    pub category: TemplateCategory,
}

/// Get an embedded template by name.
///
/// # Returns
///
/// * `Some(String)` - Template content if found
/// * `None` - Template not found
#[must_use]
#[allow(dead_code)]
pub fn get_embedded_template(name: &str) -> Option<String> {
    EMBEDDED_TEMPLATES.get(name).map(|t| t.content.to_string())
}

/// Get metadata about an embedded template.
///
/// # Returns
///
/// * `Some(&EmbeddedTemplate)` - Template metadata if found
/// * `None` - Template not found
#[must_use]
#[allow(dead_code)]
pub fn get_template_metadata(name: &str) -> Option<&'static EmbeddedTemplate> {
    EMBEDDED_TEMPLATES.get(name)
}

/// List all available embedded templates.
///
/// # Returns
///
/// A vector of all embedded templates with metadata, sorted by name.
#[must_use]
pub fn list_all_templates() -> Vec<&'static EmbeddedTemplate> {
    let mut templates: Vec<&EmbeddedTemplate> = EMBEDDED_TEMPLATES.values().collect();
    templates.sort_by_key(|t| t.name);
    templates
}

/// List templates by category.
///
/// # Arguments
///
/// * `category` - The category to filter by
///
/// # Returns
///
/// A vector of templates in the specified category, sorted by name.
#[must_use]
#[allow(dead_code)]
pub fn list_templates_by_category(category: TemplateCategory) -> Vec<&'static EmbeddedTemplate> {
    let mut templates: Vec<&EmbeddedTemplate> = EMBEDDED_TEMPLATES
        .values()
        .filter(|t| t.category == category)
        .collect();
    templates.sort_by_key(|t| t.name);
    templates
}

/// Get all templates as a map for backwards compatibility.
///
/// This matches the format used by the CLI template management code.
#[must_use]
pub fn get_templates_map() -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for template in list_all_templates() {
        map.insert(
            template.name.to_string(),
            (
                template.content.to_string(),
                template.description.to_string(),
            ),
        );
    }
    map
}

// ============================================================================
// Embedded Template Definitions
// ============================================================================

/// Central registry of all embedded templates.
///
/// All templates are embedded at compile time using `include_str!`.
/// User templates in `~/.config/ralph/templates/*.txt` override these.
static EMBEDDED_TEMPLATES: std::sync::LazyLock<HashMap<&str, EmbeddedTemplate>> =
    std::sync::LazyLock::new(|| {
        let mut m = HashMap::new();

        // ============================================================================
        // Commit Templates
        // ============================================================================

        m.insert(
            "commit_message_xml",
            EmbeddedTemplate {
                name: "commit_message_xml",
                content: include_str!("templates/commit_message_xml.txt"),
                description: "Generate Conventional Commits messages from git diffs (XML format)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_strict_json",
            EmbeddedTemplate {
                name: "commit_strict_json",
                content: include_str!("templates/commit_strict_json.txt"),
                description: "Strict JSON commit message format (retry attempt 1)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_strict_json_v2",
            EmbeddedTemplate {
                name: "commit_strict_json_v2",
                content: include_str!("templates/commit_strict_json_v2.txt"),
                description: "Strict JSON commit message format with examples (retry attempt 2)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_ultra_minimal",
            EmbeddedTemplate {
                name: "commit_ultra_minimal",
                content: include_str!("templates/commit_ultra_minimal.txt"),
                description: "Ultra-minimal commit message prompt (retry attempt 3)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_ultra_minimal_v2",
            EmbeddedTemplate {
                name: "commit_ultra_minimal_v2",
                content: include_str!("templates/commit_ultra_minimal_v2.txt"),
                description: "Ultra-minimal commit message prompt v2 (retry attempt 4)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_file_list_only",
            EmbeddedTemplate {
                name: "commit_file_list_only",
                content: include_str!("templates/commit_file_list_only.txt"),
                description: "Commit message from file list only (fallback 1)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_file_list_summary",
            EmbeddedTemplate {
                name: "commit_file_list_summary",
                content: include_str!("templates/commit_file_list_summary.txt"),
                description: "Commit message from file summary (fallback 2)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_emergency",
            EmbeddedTemplate {
                name: "commit_emergency",
                content: include_str!("templates/commit_emergency.txt"),
                description: "Emergency commit message with diff (fallback 3)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_emergency_no_diff",
            EmbeddedTemplate {
                name: "commit_emergency_no_diff",
                content: include_str!("templates/commit_emergency_no_diff.txt"),
                description: "Emergency commit message without diff (last resort)",
                category: TemplateCategory::Commit,
            },
        );

        m.insert(
            "commit_message_fallback",
            EmbeddedTemplate {
                name: "commit_message_fallback",
                content: include_str!("templates/commit_message_fallback.txt"),
                description: "Fallback commit message template",
                category: TemplateCategory::Commit,
            },
        );

        // ============================================================================
        // Developer Templates
        // ============================================================================

        m.insert(
            "developer_iteration",
            EmbeddedTemplate {
                name: "developer_iteration",
                content: include_str!("templates/developer_iteration.txt"),
                description: "Developer agent implementation mode prompt",
                category: TemplateCategory::Developer,
            },
        );

        m.insert(
            "planning",
            EmbeddedTemplate {
                name: "planning",
                content: include_str!("templates/planning.txt"),
                description: "Planning phase prompt for implementation plans",
                category: TemplateCategory::Developer,
            },
        );

        m.insert(
            "developer_iteration_fallback",
            EmbeddedTemplate {
                name: "developer_iteration_fallback",
                content: include_str!("templates/developer_iteration_fallback.txt"),
                description: "Fallback developer iteration prompt",
                category: TemplateCategory::Developer,
            },
        );

        m.insert(
            "planning_fallback",
            EmbeddedTemplate {
                name: "planning_fallback",
                content: include_str!("templates/planning_fallback.txt"),
                description: "Fallback planning prompt",
                category: TemplateCategory::Developer,
            },
        );

        // ============================================================================
        // Fix Mode Templates
        // ============================================================================

        m.insert(
            "fix_mode",
            EmbeddedTemplate {
                name: "fix_mode",
                content: include_str!("templates/fix_mode.txt"),
                description: "Fix mode prompt for addressing review issues",
                category: TemplateCategory::FixMode,
            },
        );

        m.insert(
            "fix_mode_fallback",
            EmbeddedTemplate {
                name: "fix_mode_fallback",
                content: include_str!("templates/fix_mode_fallback.txt"),
                description: "Fallback fix mode prompt",
                category: TemplateCategory::FixMode,
            },
        );

        // ============================================================================
        // Rebase Templates
        // ============================================================================

        m.insert(
            "conflict_resolution",
            EmbeddedTemplate {
                name: "conflict_resolution",
                content: include_str!("templates/conflict_resolution.txt"),
                description: "Merge conflict resolution prompt",
                category: TemplateCategory::Rebase,
            },
        );

        m.insert(
            "conflict_resolution_fallback",
            EmbeddedTemplate {
                name: "conflict_resolution_fallback",
                content: include_str!("templates/conflict_resolution_fallback.txt"),
                description: "Fallback conflict resolution prompt",
                category: TemplateCategory::Rebase,
            },
        );

        // ============================================================================
        // Reviewer Templates
        // ============================================================================

        m.insert(
            "detailed_review_minimal",
            EmbeddedTemplate {
                name: "detailed_review_minimal",
                content: include_str!("reviewer/templates/detailed_review_minimal.txt"),
                description: "Detailed review mode (minimal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "detailed_review_normal",
            EmbeddedTemplate {
                name: "detailed_review_normal",
                content: include_str!("reviewer/templates/detailed_review_normal.txt"),
                description: "Detailed review mode (normal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "incremental_review_minimal",
            EmbeddedTemplate {
                name: "incremental_review_minimal",
                content: include_str!("reviewer/templates/incremental_review_minimal.txt"),
                description: "Incremental review (changed files only, minimal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "incremental_review_normal",
            EmbeddedTemplate {
                name: "incremental_review_normal",
                content: include_str!("reviewer/templates/incremental_review_normal.txt"),
                description: "Incremental review (changed files only, normal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "universal_review_minimal",
            EmbeddedTemplate {
                name: "universal_review_minimal",
                content: include_str!("reviewer/templates/universal_review_minimal.txt"),
                description: "Universal review (all file types, minimal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "universal_review_normal",
            EmbeddedTemplate {
                name: "universal_review_normal",
                content: include_str!("reviewer/templates/universal_review_normal.txt"),
                description: "Universal review (all file types, normal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "standard_review_minimal",
            EmbeddedTemplate {
                name: "standard_review_minimal",
                content: include_str!("reviewer/templates/standard_review_minimal.txt"),
                description: "Standard review (balanced, minimal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "standard_review_normal",
            EmbeddedTemplate {
                name: "standard_review_normal",
                content: include_str!("reviewer/templates/standard_review_normal.txt"),
                description: "Standard review (balanced, normal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "comprehensive_review_minimal",
            EmbeddedTemplate {
                name: "comprehensive_review_minimal",
                content: include_str!("reviewer/templates/comprehensive_review_minimal.txt"),
                description: "Comprehensive review (thorough, minimal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "comprehensive_review_normal",
            EmbeddedTemplate {
                name: "comprehensive_review_normal",
                content: include_str!("reviewer/templates/comprehensive_review_normal.txt"),
                description: "Comprehensive review (thorough, normal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "security_review_minimal",
            EmbeddedTemplate {
                name: "security_review_minimal",
                content: include_str!("reviewer/templates/security_review_minimal.txt"),
                description: "Security-focused review (OWASP, minimal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m.insert(
            "security_review_normal",
            EmbeddedTemplate {
                name: "security_review_normal",
                content: include_str!("reviewer/templates/security_review_normal.txt"),
                description: "Security-focused review (OWASP, normal context)",
                category: TemplateCategory::Reviewer,
            },
        );

        m
    });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_embedded_template_existing() {
        let result = get_embedded_template("developer_iteration");
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(!content.is_empty());
        assert!(content.contains("IMPLEMENTATION MODE") || content.contains("Developer"));
    }

    #[test]
    fn test_get_embedded_template_not_found() {
        let result = get_embedded_template("nonexistent_template");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_template_metadata() {
        let metadata = get_template_metadata("commit_message_xml");
        assert!(metadata.is_some());
        let template = metadata.unwrap();
        assert_eq!(template.name, "commit_message_xml");
        assert_eq!(template.category, TemplateCategory::Commit);
        assert!(!template.description.is_empty());
    }

    #[test]
    fn test_list_all_templates() {
        let templates = list_all_templates();
        assert!(!templates.is_empty());
        assert!(templates.len() >= 20); // At least 20 templates

        // Verify sorted by name
        for window in templates.windows(2) {
            assert!(window[0].name <= window[1].name);
        }
    }

    #[test]
    fn test_list_templates_by_category() {
        let commit_templates = list_templates_by_category(TemplateCategory::Commit);
        assert!(!commit_templates.is_empty());
        assert!(commit_templates
            .iter()
            .all(|t| t.category == TemplateCategory::Commit));

        let developer_templates = list_templates_by_category(TemplateCategory::Developer);
        assert!(!developer_templates.is_empty());
        assert!(developer_templates
            .iter()
            .all(|t| t.category == TemplateCategory::Developer));

        let reviewer_templates = list_templates_by_category(TemplateCategory::Reviewer);
        assert!(!reviewer_templates.is_empty());
        assert!(reviewer_templates
            .iter()
            .all(|t| t.category == TemplateCategory::Reviewer));
    }

    #[test]
    fn test_get_templates_map() {
        let map = get_templates_map();
        assert!(!map.is_empty());
        assert!(map.contains_key("developer_iteration"));
        assert!(map.contains_key("commit_message_xml"));

        let (content, description) = map.get("developer_iteration").unwrap();
        assert!(!content.is_empty());
        assert!(!description.is_empty());
    }

    #[test]
    fn test_all_templates_have_content() {
        let templates = list_all_templates();
        for template in templates {
            assert!(
                !template.content.is_empty(),
                "Template '{}' has empty content",
                template.name
            );
        }
    }

    #[test]
    fn test_all_templates_have_descriptions() {
        let templates = list_all_templates();
        for template in templates {
            assert!(
                !template.description.is_empty(),
                "Template '{}' has empty description",
                template.name
            );
        }
    }

    #[test]
    fn test_fallback_templates_exist() {
        // Verify all fallback templates exist
        assert!(get_embedded_template("developer_iteration_fallback").is_some());
        assert!(get_embedded_template("planning_fallback").is_some());
        assert!(get_embedded_template("fix_mode_fallback").is_some());
        assert!(get_embedded_template("conflict_resolution_fallback").is_some());
        assert!(get_embedded_template("commit_message_fallback").is_some());
    }
}
