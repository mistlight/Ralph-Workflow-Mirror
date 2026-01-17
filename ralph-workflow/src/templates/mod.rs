//! Prompt template management module.
//!
//! This module provides a collection of PROMPT.md templates for different
//! task types (feature specifications, bug fixes, refactoring, etc.).
//!
//! Templates are embedded at compile time using `include_str!` and can be
//! accessed via the `get_template_content()` function.

use std::fmt;

/// Available prompt template types.
///
/// Each variant represents a different template for a specific use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTemplate {
    /// Comprehensive product specification template
    FeatureSpec,
    /// Concise bug fix template
    BugFix,
    /// Code refactoring template
    Refactor,
    /// Test writing template
    Test,
    /// Documentation update template
    Docs,
    /// Quick/small change template
    Quick,
    /// Performance optimization template
    PerformanceOptimization,
    /// Security audit template
    SecurityAudit,
    /// API integration template
    ApiIntegration,
    /// Database migration template
    DatabaseMigration,
    /// Dependency update template
    DependencyUpdate,
}

impl PromptTemplate {
    /// Returns the name/key for this template (used for CLI arguments).
    pub const fn name(self) -> &'static str {
        match self {
            Self::FeatureSpec => "feature-spec",
            Self::BugFix => "bug-fix",
            Self::Refactor => "refactor",
            Self::Test => "test",
            Self::Docs => "docs",
            Self::Quick => "quick",
            Self::PerformanceOptimization => "performance-optimization",
            Self::SecurityAudit => "security-audit",
            Self::ApiIntegration => "api-integration",
            Self::DatabaseMigration => "database-migration",
            Self::DependencyUpdate => "dependency-update",
        }
    }

    /// Returns a short description of this template.
    pub const fn description(self) -> &'static str {
        match self {
            Self::FeatureSpec => "Comprehensive product specification with questions to consider and code quality standards",
            Self::BugFix => "Bug fix template with investigation guidance and testing requirements",
            Self::Refactor => "Code refactoring template with behavior preservation emphasis",
            Self::Test => "Test writing template with edge case considerations",
            Self::Docs => "Documentation update template with completeness checklist",
            Self::Quick => "Quick/small change template (minimal)",
            Self::PerformanceOptimization => "Performance optimization template with benchmarking and profiling guidance",
            Self::SecurityAudit => "Security audit template covering OWASP Top 10 and vulnerability remediation",
            Self::ApiIntegration => "API integration template with error handling, retry logic, and resilience patterns",
            Self::DatabaseMigration => "Database migration template with zero-downtime strategies and rollback plans",
            Self::DependencyUpdate => "Dependency update template with migration guides and breaking change handling",
        }
    }

    /// Returns the embedded template content.
    pub const fn content(self) -> &'static str {
        match self {
            Self::FeatureSpec => {
                include_str!("../../templates/prompts/feature-spec.md")
            }
            Self::BugFix => {
                include_str!("../../templates/prompts/bug-fix.md")
            }
            Self::Refactor => {
                include_str!("../../templates/prompts/refactor.md")
            }
            Self::Test => {
                include_str!("../../templates/prompts/test.md")
            }
            Self::Docs => {
                include_str!("../../templates/prompts/docs.md")
            }
            Self::Quick => {
                include_str!("../../templates/prompts/quick.md")
            }
            Self::PerformanceOptimization => {
                include_str!("../../templates/prompts/performance-optimization.md")
            }
            Self::SecurityAudit => {
                include_str!("../../templates/prompts/security-audit.md")
            }
            Self::ApiIntegration => {
                include_str!("../../templates/prompts/api-integration.md")
            }
            Self::DatabaseMigration => {
                include_str!("../../templates/prompts/database-migration.md")
            }
            Self::DependencyUpdate => {
                include_str!("../../templates/prompts/dependency-update.md")
            }
        }
    }
}

impl fmt::Display for PromptTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// All available prompt templates.
pub const ALL_TEMPLATES: [PromptTemplate; 11] = [
    PromptTemplate::FeatureSpec,
    PromptTemplate::BugFix,
    PromptTemplate::Refactor,
    PromptTemplate::Test,
    PromptTemplate::Docs,
    PromptTemplate::Quick,
    PromptTemplate::PerformanceOptimization,
    PromptTemplate::SecurityAudit,
    PromptTemplate::ApiIntegration,
    PromptTemplate::DatabaseMigration,
    PromptTemplate::DependencyUpdate,
];

/// Get a template by name.
///
/// # Arguments
///
/// * `name` - The template name (e.g., "feature-spec", "bug-fix")
///
/// # Returns
///
/// * `Some(PromptTemplate)` - The template if found
/// * `None` - If no template matches the name
pub fn get_template(name: &str) -> Option<PromptTemplate> {
    ALL_TEMPLATES.iter().find(|t| t.name() == name).copied()
}

/// List all available templates with their descriptions.
///
/// Returns a vector of (name, description) tuples.
pub fn list_templates() -> Vec<(&'static str, &'static str)> {
    ALL_TEMPLATES
        .iter()
        .map(|t| (t.name(), t.description()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_names() {
        assert_eq!(PromptTemplate::FeatureSpec.name(), "feature-spec");
        assert_eq!(PromptTemplate::BugFix.name(), "bug-fix");
        assert_eq!(PromptTemplate::Refactor.name(), "refactor");
        assert_eq!(PromptTemplate::Test.name(), "test");
        assert_eq!(PromptTemplate::Docs.name(), "docs");
        assert_eq!(PromptTemplate::Quick.name(), "quick");
        assert_eq!(
            PromptTemplate::PerformanceOptimization.name(),
            "performance-optimization"
        );
        assert_eq!(PromptTemplate::SecurityAudit.name(), "security-audit");
        assert_eq!(PromptTemplate::ApiIntegration.name(), "api-integration");
        assert_eq!(
            PromptTemplate::DatabaseMigration.name(),
            "database-migration"
        );
        assert_eq!(PromptTemplate::DependencyUpdate.name(), "dependency-update");
    }

    #[test]
    fn test_template_descriptions() {
        assert!(!PromptTemplate::FeatureSpec.description().is_empty());
        assert!(!PromptTemplate::BugFix.description().is_empty());
        assert!(!PromptTemplate::Refactor.description().is_empty());
        assert!(!PromptTemplate::Test.description().is_empty());
        assert!(!PromptTemplate::Docs.description().is_empty());
        assert!(!PromptTemplate::Quick.description().is_empty());
        assert!(!PromptTemplate::PerformanceOptimization
            .description()
            .is_empty());
        assert!(!PromptTemplate::SecurityAudit.description().is_empty());
        assert!(!PromptTemplate::ApiIntegration.description().is_empty());
        assert!(!PromptTemplate::DatabaseMigration.description().is_empty());
        assert!(!PromptTemplate::DependencyUpdate.description().is_empty());
    }

    #[test]
    fn test_get_template() {
        assert_eq!(
            get_template("feature-spec"),
            Some(PromptTemplate::FeatureSpec)
        );
        assert_eq!(get_template("bug-fix"), Some(PromptTemplate::BugFix));
        assert_eq!(
            get_template("performance-optimization"),
            Some(PromptTemplate::PerformanceOptimization)
        );
        assert_eq!(
            get_template("security-audit"),
            Some(PromptTemplate::SecurityAudit)
        );
        assert_eq!(
            get_template("api-integration"),
            Some(PromptTemplate::ApiIntegration)
        );
        assert_eq!(
            get_template("database-migration"),
            Some(PromptTemplate::DatabaseMigration)
        );
        assert_eq!(
            get_template("dependency-update"),
            Some(PromptTemplate::DependencyUpdate)
        );
        assert_eq!(get_template("nonexistent"), None);
    }

    #[test]
    fn test_list_templates() {
        let templates = list_templates();
        assert_eq!(templates.len(), 11);
        assert!(templates.iter().any(|(name, _)| name == &"feature-spec"));
        assert!(templates.iter().any(|(name, _)| name == &"bug-fix"));
        assert!(templates
            .iter()
            .any(|(name, _)| name == &"performance-optimization"));
        assert!(templates.iter().any(|(name, _)| name == &"security-audit"));
        assert!(templates.iter().any(|(name, _)| name == &"api-integration"));
        assert!(templates
            .iter()
            .any(|(name, _)| name == &"database-migration"));
        assert!(templates
            .iter()
            .any(|(name, _)| name == &"dependency-update"));
    }

    #[test]
    fn test_template_content_has_goal() {
        for template in ALL_TEMPLATES {
            let content = template.content();
            assert!(
                content.contains("## Goal"),
                "Template {} missing Goal section",
                template.name()
            );
        }
    }

    #[test]
    fn test_template_content_has_acceptance() {
        for template in ALL_TEMPLATES {
            let content = template.content();
            assert!(
                content.contains("## Acceptance") || content.contains("## Acceptance Checks"),
                "Template {} missing Acceptance section",
                template.name()
            );
        }
    }
}
