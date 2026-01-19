//! Embedded Template Catalog
//!
//! Central registry of all embedded templates with metadata.
//!
//! This module provides a single source of truth for all embedded templates,
//! consolidating scattered `include_str!` calls across the codebase.

use std::collections::HashMap;

/// Metadata about an embedded template.
#[derive(Debug, Clone)]
pub struct EmbeddedTemplate {
    /// Template name (used for lookup and user override files)
    pub name: &'static str,
    /// Template content
    pub content: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Whether this template is deprecated
    pub deprecated: bool,
}

/// Get an embedded template by name.
///
/// # Returns
///
/// * `Some(String)` - Template content if found
/// * `None` - Template not found
#[must_use]
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
                deprecated: false,
            },
        );

        m.insert(
            "commit_emergency",
            EmbeddedTemplate {
                name: "commit_emergency",
                content: include_str!("templates/commit_emergency.txt"),
                description: "Emergency commit message with diff (fallback)",
                deprecated: false,
            },
        );

        m.insert(
            "commit_message_fallback",
            EmbeddedTemplate {
                name: "commit_message_fallback",
                content: include_str!("templates/commit_message_fallback.txt"),
                description: "Fallback commit message template",
                deprecated: false,
            },
        );

        m.insert(
            "commit_xsd_retry",
            EmbeddedTemplate {
                name: "commit_xsd_retry",
                content: include_str!("templates/commit_xsd_retry.txt"),
                description: "XSD validation retry prompt for commit messages",
                deprecated: false,
            },
        );

        // TODO: Add commit_validation_retry template when implementing retry logic

        // ============================================================================
        // Developer Templates
        // ============================================================================

        m.insert(
            "developer_iteration",
            EmbeddedTemplate {
                name: "developer_iteration",
                content: include_str!("templates/developer_iteration.txt"),
                description: "Developer agent implementation mode prompt",
                deprecated: false,
            },
        );

        m.insert(
            "planning",
            EmbeddedTemplate {
                name: "planning",
                content: include_str!("templates/planning.txt"),
                description: "Planning phase prompt for implementation plans",
                deprecated: false,
            },
        );

        m.insert(
            "developer_iteration_fallback",
            EmbeddedTemplate {
                name: "developer_iteration_fallback",
                content: include_str!("templates/developer_iteration_fallback.txt"),
                description: "Fallback developer iteration prompt",
                deprecated: false,
            },
        );

        m.insert(
            "planning_fallback",
            EmbeddedTemplate {
                name: "planning_fallback",
                content: include_str!("templates/planning_fallback.txt"),
                description: "Fallback planning prompt",
                deprecated: false,
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
                deprecated: false,
            },
        );

        m.insert(
            "fix_mode_fallback",
            EmbeddedTemplate {
                name: "fix_mode_fallback",
                content: include_str!("templates/fix_mode_fallback.txt"),
                description: "Fallback fix mode prompt",
                deprecated: false,
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
                deprecated: false,
            },
        );

        m.insert(
            "conflict_resolution_fallback",
            EmbeddedTemplate {
                name: "conflict_resolution_fallback",
                content: include_str!("templates/conflict_resolution_fallback.txt"),
                description: "Fallback conflict resolution prompt",
                deprecated: false,
            },
        );

        // ============================================================================
        // Reviewer Templates (Consolidated - 4 primary templates)
        // ============================================================================
        //
        // CONSOLIDATION: Templates have been consolidated from 12 (6 types × 2 context levels)
        // to 4 primary templates. The "minimal context" concept has been deprecated as it
        // provided no real value - reviewers should read changed files for context.
        //
        // Primary templates:
        // - standard_review: Default balanced review (most common)
        // - comprehensive_review: Priority-ordered thorough review
        // - security_review: OWASP Top 10 focused review
        // - universal_review: Simplified prompt for problematic agents (GLM, ZhipuAI)

        // Primary consolidated templates
        m.insert(
            "standard_review",
            EmbeddedTemplate {
                name: "standard_review",
                content: include_str!("reviewer/templates/standard_review.txt"),
                description: "Standard balanced review with comprehensive checklist (DEFAULT)",
                deprecated: false,
            },
        );

        m.insert(
            "comprehensive_review",
            EmbeddedTemplate {
                name: "comprehensive_review",
                content: include_str!("reviewer/templates/comprehensive_review.txt"),
                description: "Comprehensive priority-ordered review (12 categories)",
                deprecated: false,
            },
        );

        m.insert(
            "security_review",
            EmbeddedTemplate {
                name: "security_review",
                content: include_str!("reviewer/templates/security_review.txt"),
                description: "Security-focused review (OWASP Top 10)",
                deprecated: false,
            },
        );

        m.insert(
            "universal_review",
            EmbeddedTemplate {
                name: "universal_review",
                content: include_str!("reviewer/templates/universal_review.txt"),
                description: "Simplified review for maximum agent compatibility",
                deprecated: false,
            },
        );

        // Legacy aliases - deprecated templates point to consolidated versions
        // These exist for backward compatibility with user template overrides
        m.insert(
            "detailed_review_minimal",
            EmbeddedTemplate {
                name: "detailed_review_minimal",
                content: include_str!("reviewer/templates/standard_review.txt"),
                description: "[DEPRECATED] Use standard_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "detailed_review_normal",
            EmbeddedTemplate {
                name: "detailed_review_normal",
                content: include_str!("reviewer/templates/standard_review.txt"),
                description: "[DEPRECATED] Use standard_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "incremental_review_minimal",
            EmbeddedTemplate {
                name: "incremental_review_minimal",
                content: include_str!("reviewer/templates/standard_review.txt"),
                description: "[DEPRECATED] Use standard_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "incremental_review_normal",
            EmbeddedTemplate {
                name: "incremental_review_normal",
                content: include_str!("reviewer/templates/standard_review.txt"),
                description: "[DEPRECATED] Use standard_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "universal_review_minimal",
            EmbeddedTemplate {
                name: "universal_review_minimal",
                content: include_str!("reviewer/templates/universal_review.txt"),
                description: "[DEPRECATED] Use universal_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "universal_review_normal",
            EmbeddedTemplate {
                name: "universal_review_normal",
                content: include_str!("reviewer/templates/universal_review.txt"),
                description: "[DEPRECATED] Use universal_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "standard_review_minimal",
            EmbeddedTemplate {
                name: "standard_review_minimal",
                content: include_str!("reviewer/templates/standard_review.txt"),
                description: "[DEPRECATED] Use standard_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "standard_review_normal",
            EmbeddedTemplate {
                name: "standard_review_normal",
                content: include_str!("reviewer/templates/standard_review.txt"),
                description: "[DEPRECATED] Use standard_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "comprehensive_review_minimal",
            EmbeddedTemplate {
                name: "comprehensive_review_minimal",
                content: include_str!("reviewer/templates/comprehensive_review.txt"),
                description: "[DEPRECATED] Use comprehensive_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "comprehensive_review_normal",
            EmbeddedTemplate {
                name: "comprehensive_review_normal",
                content: include_str!("reviewer/templates/comprehensive_review.txt"),
                description: "[DEPRECATED] Use comprehensive_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "security_review_minimal",
            EmbeddedTemplate {
                name: "security_review_minimal",
                content: include_str!("reviewer/templates/security_review.txt"),
                description: "[DEPRECATED] Use security_review instead",
                deprecated: true,
            },
        );

        m.insert(
            "security_review_normal",
            EmbeddedTemplate {
                name: "security_review_normal",
                content: include_str!("reviewer/templates/security_review.txt"),
                description: "[DEPRECATED] Use security_review instead",
                deprecated: true,
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

    #[test]
    fn test_consolidated_reviewer_templates_exist() {
        // Verify the 4 consolidated reviewer templates exist
        assert!(get_embedded_template("standard_review").is_some());
        assert!(get_embedded_template("comprehensive_review").is_some());
        assert!(get_embedded_template("security_review").is_some());
        assert!(get_embedded_template("universal_review").is_some());
    }

    #[test]
    fn test_consolidated_reviewer_templates_have_review_checklist() {
        // Standard review should have the review coverage checklist
        let standard = get_embedded_template("standard_review").unwrap();
        assert!(
            standard.contains("REVIEW COVERAGE CHECKLIST"),
            "Standard review template should have review checklist"
        );

        // Comprehensive review should have review categories
        let comprehensive = get_embedded_template("comprehensive_review").unwrap();
        assert!(
            comprehensive.contains("REVIEW CATEGORIES"),
            "Comprehensive review template should have review categories"
        );

        // Security review should have OWASP categories
        let security = get_embedded_template("security_review").unwrap();
        assert!(
            security.contains("OWASP TOP 10"),
            "Security review template should have OWASP Top 10"
        );
    }

    #[test]
    fn test_deprecated_templates_point_to_consolidated() {
        // Legacy templates should have same content as consolidated versions
        let standard = get_embedded_template("standard_review").unwrap();
        let standard_minimal = get_embedded_template("standard_review_minimal").unwrap();
        let standard_normal = get_embedded_template("standard_review_normal").unwrap();

        assert_eq!(
            standard, standard_minimal,
            "standard_review_minimal should point to standard_review"
        );
        assert_eq!(
            standard, standard_normal,
            "standard_review_normal should point to standard_review"
        );
    }
}
