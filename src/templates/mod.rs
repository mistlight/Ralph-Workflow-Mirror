//! Prompt template management module.
//!
//! This module provides a collection of PROMPT.md templates for different
//! task types (feature specifications, bug fixes, refactoring, etc.).
//!
//! Templates are embedded at compile time using `include_str!` and can be
//! accessed via the `get_template_content()` function.

use std::fmt;

/// Template category for grouping templates by purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCategory {
    /// General-purpose templates (feature-spec, bug-fix, refactor, test, docs, quick)
    General,
    /// Language-specific templates (rust-feature, rust-bug-fix, typescript-feature, ruby-on-rails, cpp-feature)
    LanguageSpecific,
}

#[allow(dead_code)]
impl TemplateCategory {
    /// Returns the display name for this category.
    pub fn name(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::LanguageSpecific => "Language-Specific",
        }
    }
}

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
    /// Rust-specific feature template
    RustFeature,
    /// Rust-specific bug fix template
    RustBugFix,
    /// TypeScript/Node.js feature template
    TypeScriptFeature,
    /// Ruby on Rails feature template
    RubyOnRails,
    /// C++ feature template
    CppFeature,
}

impl PromptTemplate {
    /// Returns the file name for this template.
    #[allow(dead_code)]
    pub fn filename(self) -> &'static str {
        match self {
            Self::FeatureSpec => "feature-spec.md",
            Self::BugFix => "bug-fix.md",
            Self::Refactor => "refactor.md",
            Self::Test => "test.md",
            Self::Docs => "docs.md",
            Self::Quick => "quick.md",
            Self::RustFeature => "rust-feature.md",
            Self::RustBugFix => "rust-bug-fix.md",
            Self::TypeScriptFeature => "typescript-feature.md",
            Self::RubyOnRails => "ruby-on-rails.md",
            Self::CppFeature => "cpp-feature.md",
        }
    }

    /// Returns the name/key for this template (used for CLI arguments).
    pub fn name(self) -> &'static str {
        match self {
            Self::FeatureSpec => "feature-spec",
            Self::BugFix => "bug-fix",
            Self::Refactor => "refactor",
            Self::Test => "test",
            Self::Docs => "docs",
            Self::Quick => "quick",
            Self::RustFeature => "rust-feature",
            Self::RustBugFix => "rust-bug-fix",
            Self::TypeScriptFeature => "typescript-feature",
            Self::RubyOnRails => "ruby-on-rails",
            Self::CppFeature => "cpp-feature",
        }
    }

    /// Returns the category for this template.
    pub fn category(self) -> TemplateCategory {
        match self {
            Self::FeatureSpec | Self::BugFix | Self::Refactor | Self::Test | Self::Docs | Self::Quick => {
                TemplateCategory::General
            }
            Self::RustFeature | Self::RustBugFix | Self::TypeScriptFeature | Self::RubyOnRails | Self::CppFeature => {
                TemplateCategory::LanguageSpecific
            }
        }
    }

    /// Returns a short description of this template.
    pub fn description(self) -> &'static str {
        match self {
            Self::FeatureSpec => "Comprehensive product specification with questions to consider and code quality standards",
            Self::BugFix => "Bug fix template with investigation guidance and testing requirements",
            Self::Refactor => "Code refactoring template with behavior preservation emphasis",
            Self::Test => "Test writing template with edge case considerations",
            Self::Docs => "Documentation update template with completeness checklist",
            Self::Quick => "Quick/small change template (minimal)",
            Self::RustFeature => "Rust feature template with ownership, async, error handling best practices",
            Self::RustBugFix => "Rust bug fix template with borrow checker, panic, unsafe guidance",
            Self::TypeScriptFeature => "TypeScript/Node.js feature template with typing, async/await, npm best practices",
            Self::RubyOnRails => "Ruby on Rails template with conventions, ActiveRecord, RESTful design",
            Self::CppFeature => "C++ feature template with modern standards, RAII, memory management",
        }
    }

    /// Returns the embedded template content.
    pub fn content(self) -> &'static str {
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
            Self::RustFeature => {
                include_str!("../../templates/prompts/rust-feature.md")
            }
            Self::RustBugFix => {
                include_str!("../../templates/prompts/rust-bug-fix.md")
            }
            Self::TypeScriptFeature => {
                include_str!("../../templates/prompts/typescript-feature.md")
            }
            Self::RubyOnRails => {
                include_str!("../../templates/prompts/ruby-on-rails.md")
            }
            Self::CppFeature => {
                include_str!("../../templates/prompts/cpp-feature.md")
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
    PromptTemplate::RustFeature,
    PromptTemplate::RustBugFix,
    PromptTemplate::TypeScriptFeature,
    PromptTemplate::RubyOnRails,
    PromptTemplate::CppFeature,
];

/// Get template content by name.
///
/// # Arguments
///
/// * `name` - The template name (e.g., "feature-spec", "bug-fix")
///
/// # Returns
///
/// * `Some(&str)` - The template content if found
/// * `None` - If no template matches the name
#[allow(dead_code)]
pub fn get_template_content(name: &str) -> Option<&'static str> {
    ALL_TEMPLATES
        .iter()
        .find(|t| t.name() == name)
        .map(|t| t.content())
}

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

/// Suggest a template based on task keywords.
///
/// # Arguments
///
/// * `keywords` - Keywords from the task description (e.g., commit message)
///
/// # Returns
///
/// A suggested template, or `FeatureSpec` as the default if no match is found.
#[allow(dead_code)]
pub fn suggest_template(keywords: &str) -> PromptTemplate {
    let keywords_lower = keywords.to_lowercase();

    // Check for quick/small change keywords (check before bug fix, since "quick fix" should be quick)
    if keywords_lower.contains("quick")
        || keywords_lower.contains("small")
        || keywords_lower.contains("minor")
        || keywords_lower.contains("typo")
    {
        return PromptTemplate::Quick;
    }

    // Check for bug fix keywords
    if keywords_lower.contains("bug")
        || keywords_lower.contains("fix")
        || keywords_lower.contains("issue")
        || keywords_lower.contains("error")
    {
        return PromptTemplate::BugFix;
    }

    // Check for refactor keywords
    if keywords_lower.contains("refactor")
        || keywords_lower.contains("cleanup")
        || keywords_lower.contains("reorganize")
        || keywords_lower.contains("restructure")
    {
        return PromptTemplate::Refactor;
    }

    // Check for test keywords
    if keywords_lower.contains("test")
        || keywords_lower.contains("testing")
        || keywords_lower.contains("coverage")
    {
        return PromptTemplate::Test;
    }

    // Check for docs keywords
    if keywords_lower.contains("doc")
        || keywords_lower.contains("readme")
        || keywords_lower.contains("documentation")
    {
        return PromptTemplate::Docs;
    }

    // Default to feature spec
    PromptTemplate::FeatureSpec
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
    }

    #[test]
    fn test_template_descriptions() {
        assert!(!PromptTemplate::FeatureSpec.description().is_empty());
        assert!(!PromptTemplate::BugFix.description().is_empty());
        assert!(!PromptTemplate::Refactor.description().is_empty());
        assert!(!PromptTemplate::Test.description().is_empty());
        assert!(!PromptTemplate::Docs.description().is_empty());
        assert!(!PromptTemplate::Quick.description().is_empty());
    }

    #[test]
    fn test_get_template_content() {
        assert!(get_template_content("feature-spec").is_some());
        assert!(get_template_content("bug-fix").is_some());
        assert!(get_template_content("refactor").is_some());
        assert!(get_template_content("test").is_some());
        assert!(get_template_content("docs").is_some());
        assert!(get_template_content("quick").is_some());
        assert!(get_template_content("nonexistent").is_none());
    }

    #[test]
    fn test_get_template() {
        assert_eq!(
            get_template("feature-spec"),
            Some(PromptTemplate::FeatureSpec)
        );
        assert_eq!(get_template("bug-fix"), Some(PromptTemplate::BugFix));
        assert_eq!(get_template("nonexistent"), None);
    }

    #[test]
    fn test_list_templates() {
        let templates = list_templates();
        assert_eq!(templates.len(), 11);
        assert!(templates.iter().any(|(name, _)| name == &"feature-spec"));
        assert!(templates.iter().any(|(name, _)| name == &"bug-fix"));
    }

    #[test]
    fn test_language_specific_templates_exist() {
        assert_eq!(PromptTemplate::RustFeature.name(), "rust-feature");
        assert_eq!(PromptTemplate::RustBugFix.name(), "rust-bug-fix");
        assert_eq!(PromptTemplate::TypeScriptFeature.name(), "typescript-feature");
        assert_eq!(PromptTemplate::RubyOnRails.name(), "ruby-on-rails");
        assert_eq!(PromptTemplate::CppFeature.name(), "cpp-feature");
    }

    #[test]
    fn test_language_specific_templates_have_content() {
        assert!(!PromptTemplate::RustFeature.content().is_empty());
        assert!(!PromptTemplate::RustBugFix.content().is_empty());
        assert!(!PromptTemplate::TypeScriptFeature.content().is_empty());
        assert!(!PromptTemplate::RubyOnRails.content().is_empty());
        assert!(!PromptTemplate::CppFeature.content().is_empty());
    }

    #[test]
    fn test_template_categories() {
        // General templates
        assert_eq!(PromptTemplate::FeatureSpec.category(), TemplateCategory::General);
        assert_eq!(PromptTemplate::BugFix.category(), TemplateCategory::General);
        assert_eq!(PromptTemplate::Refactor.category(), TemplateCategory::General);
        assert_eq!(PromptTemplate::Test.category(), TemplateCategory::General);
        assert_eq!(PromptTemplate::Docs.category(), TemplateCategory::General);
        assert_eq!(PromptTemplate::Quick.category(), TemplateCategory::General);

        // Language-specific templates
        assert_eq!(PromptTemplate::RustFeature.category(), TemplateCategory::LanguageSpecific);
        assert_eq!(PromptTemplate::RustBugFix.category(), TemplateCategory::LanguageSpecific);
        assert_eq!(PromptTemplate::TypeScriptFeature.category(), TemplateCategory::LanguageSpecific);
        assert_eq!(PromptTemplate::RubyOnRails.category(), TemplateCategory::LanguageSpecific);
        assert_eq!(PromptTemplate::CppFeature.category(), TemplateCategory::LanguageSpecific);
    }

    #[test]
    fn test_suggest_template_bug_fix() {
        assert_eq!(
            suggest_template("fix: broken login"),
            PromptTemplate::BugFix
        );
        assert_eq!(
            suggest_template("bug: crash on startup"),
            PromptTemplate::BugFix
        );
        assert_eq!(suggest_template("issue #123"), PromptTemplate::BugFix);
    }

    #[test]
    fn test_suggest_template_refactor() {
        assert_eq!(
            suggest_template("refactor: clean up module"),
            PromptTemplate::Refactor
        );
        assert_eq!(
            suggest_template("cleanup: remove unused code"),
            PromptTemplate::Refactor
        );
    }

    #[test]
    fn test_suggest_template_test() {
        assert_eq!(
            suggest_template("test: add unit tests"),
            PromptTemplate::Test
        );
        assert_eq!(
            suggest_template("testing: improve coverage"),
            PromptTemplate::Test
        );
    }

    #[test]
    fn test_suggest_template_docs() {
        assert_eq!(
            suggest_template("docs: update README"),
            PromptTemplate::Docs
        );
        assert_eq!(
            suggest_template("documentation: add API docs"),
            PromptTemplate::Docs
        );
    }

    #[test]
    fn test_suggest_template_quick() {
        assert_eq!(suggest_template("quick: fix typo"), PromptTemplate::Quick);
        assert_eq!(
            suggest_template("minor: adjust spacing"),
            PromptTemplate::Quick
        );
    }

    #[test]
    fn test_suggest_template_default() {
        assert_eq!(
            suggest_template("feat: add new feature"),
            PromptTemplate::FeatureSpec
        );
        assert_eq!(
            suggest_template("implement something"),
            PromptTemplate::FeatureSpec
        );
        assert_eq!(suggest_template("add widget"), PromptTemplate::FeatureSpec);
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
