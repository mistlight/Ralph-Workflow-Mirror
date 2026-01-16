//! Template Registry for Runtime Template Loading
//!
//! This module provides a centralized template registry that supports:
//! - User-defined template overrides (from `~/.config/ralph/templates/`)
//! - Embedded templates as fallback (compiled into binary)
//! - Runtime template loading with caching
//!
//! # Template Loading Priority
//!
//! 1. User template: `{user_templates_dir}/{name}.txt`
//! 2. Embedded template: Compiled-in fallback
//! 3. Error: Template not found

use std::fs;
use std::path::PathBuf;

use crate::prompts::partials::get_shared_partials;

/// Error type for template loading operations.
#[derive(Debug, Clone, thiserror::Error)]
#[allow(dead_code)]
pub enum TemplateError {
    /// Template not found in user directory or embedded catalog
    #[error("Template '{name}' not found")]
    TemplateNotFound { name: String },

    /// Error reading user template file
    #[error("Failed to read template '{name}': {reason}")]
    ReadError { name: String, reason: String },

    /// User template has invalid syntax
    #[error("Template '{name}' has invalid syntax: {reason}")]
    SyntaxError { name: String, reason: String },
}

/// Template registry for loading templates from multiple sources.
///
/// The registry maintains a user templates directory for template overrides.
/// Templates are loaded from user directory first, falling back to embedded templates.
#[derive(Debug, Clone)]
pub struct TemplateRegistry {
    /// User templates directory (higher priority than embedded templates).
    user_templates_dir: Option<PathBuf>,
}

impl TemplateRegistry {
    /// Create a new template registry.
    ///
    /// # Arguments
    ///
    /// * `user_templates_dir` - Optional path to user templates directory.
    ///   When set, templates in this directory override embedded templates.
    #[must_use]
    pub const fn new(user_templates_dir: Option<PathBuf>) -> Self {
        Self { user_templates_dir }
    }

    /// Get the default user templates directory path.
    ///
    /// Returns `~/.config/ralph/templates/` by default.
    /// Respects `XDG_CONFIG_HOME` environment variable.
    ///
    /// # Returns
    ///
    /// `None` if home directory cannot be determined.
    #[must_use]
    pub fn default_user_templates_dir() -> Option<PathBuf> {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let xdg = xdg.trim();
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join("ralph").join("templates"));
            }
        }

        dirs::home_dir().map(|d| d.join(".config").join("ralph").join("templates"))
    }

    /// Check if a user template exists for the given name.
    ///
    /// # Returns
    ///
    /// `true` if a user template file exists (not embedded)
    #[must_use]
    pub fn has_user_template(&self, name: &str) -> bool {
        self.user_templates_dir
            .as_ref()
            .is_some_and(|user_dir| user_dir.join(format!("{name}.txt")).exists())
    }

    /// Get the source of a template (user or embedded).
    ///
    /// # Returns
    ///
    /// * `"user"` - Template is from user directory
    /// * `"embedded"` - Template is embedded
    #[must_use]
    pub fn template_source(&self, name: &str) -> &'static str {
        if self.has_user_template(name) {
            "user"
        } else {
            "embedded"
        }
    }

    /// Get the file path for a user template.
    ///
    /// # Returns
    ///
    /// * `Some(PathBuf)` - Path to user template if it exists
    /// * `None` - User template directory not configured or file doesn't exist
    #[must_use]
    #[allow(dead_code)]
    pub fn user_template_path(&self, name: &str) -> Option<PathBuf> {
        self.user_templates_dir.as_ref().and_then(|dir| {
            let path = dir.join(format!("{name}.txt"));
            if path.exists() {
                Some(path)
            } else {
                None
            }
        })
    }

    /// Load a template by name.
    ///
    /// Template loading priority:
    /// 1. User template: `{user_templates_dir}/{name}.txt`
    /// 2. Embedded template from catalog
    /// 3. Error if neither exists
    ///
    /// # Arguments
    ///
    /// * `name` - Template name (without `.txt` extension)
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Template content
    /// * `Err(TemplateError)` - Template not found or read error
    #[allow(dead_code)]
    pub fn get_template(&self, name: &str) -> Result<String, TemplateError> {
        use crate::prompts::template_catalog;

        // Try user template first
        if let Some(user_dir) = &self.user_templates_dir {
            let user_path = user_dir.join(format!("{name}.txt"));
            if user_path.exists() {
                return fs::read_to_string(&user_path).map_err(|e| TemplateError::ReadError {
                    name: name.to_string(),
                    reason: e.to_string(),
                });
            }
        }

        // Fall back to embedded template
        if let Some(content) = template_catalog::get_embedded_template(name) {
            return Ok(content);
        }

        // Template not found
        Err(TemplateError::TemplateNotFound {
            name: name.to_string(),
        })
    }

    /// Load a partial template by name.
    ///
    /// Partials are shared template snippets that can be included in other templates.
    /// User partials override embedded partials, similar to regular templates.
    ///
    /// # Arguments
    ///
    /// * `name` - Partial name (without `.txt` extension)
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Partial content
    /// * `Err(TemplateError)` - Partial not found
    #[allow(dead_code)]
    pub fn get_partial(&self, name: &str) -> Result<String, TemplateError> {
        // Try user partial first (stored in user templates dir)
        if let Some(user_dir) = &self.user_templates_dir {
            let user_path = user_dir.join(format!("{name}.txt"));
            if user_path.exists() {
                return fs::read_to_string(&user_path).map_err(|e| TemplateError::ReadError {
                    name: name.to_string(),
                    reason: e.to_string(),
                });
            }
        }

        // Fall back to embedded partials
        let partials = get_shared_partials();
        if let Some(content) = partials.get(name) {
            return Ok(content.clone());
        }

        // Partial not found
        Err(TemplateError::TemplateNotFound {
            name: name.to_string(),
        })
    }

    /// Get all template names available in the embedded catalog.
    ///
    /// # Returns
    ///
    /// A vector of all embedded template names, sorted alphabetically.
    #[must_use]
    #[allow(dead_code)]
    #[allow(clippy::unused_self)]
    pub fn all_template_names(&self) -> Vec<String> {
        use crate::prompts::template_catalog;
        template_catalog::list_all_templates()
            .iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// Get metadata about an embedded template.
    ///
    /// # Returns
    ///
    /// * `Some(&EmbeddedTemplate)` - Template metadata if found
    /// * `None` - Template not found in embedded catalog
    #[must_use]
    #[allow(dead_code)]
    #[allow(clippy::unused_self)]
    pub fn template_metadata(
        &self,
        name: &str,
    ) -> Option<&'static crate::prompts::template_catalog::EmbeddedTemplate> {
        use crate::prompts::template_catalog;
        template_catalog::get_template_metadata(name)
    }

    /// Check if a template exists (either user or embedded).
    ///
    /// # Returns
    ///
    /// `true` if the template exists in user directory or embedded catalog
    #[must_use]
    #[allow(dead_code)]
    pub fn template_exists(&self, name: &str) -> bool {
        self.has_user_template(name) || self.template_metadata(name).is_some()
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new(Self::default_user_templates_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = TemplateRegistry::new(None);
        assert!(registry.user_templates_dir.is_none());

        let custom_dir = PathBuf::from("/custom/templates");
        let registry = TemplateRegistry::new(Some(custom_dir.clone()));
        assert_eq!(registry.user_templates_dir, Some(custom_dir));
    }

    #[test]
    fn test_default_user_templates_dir() {
        let dir = TemplateRegistry::default_user_templates_dir();
        assert!(dir.is_some());
        let path = dir.unwrap();
        assert!(path.to_string_lossy().contains("templates"));
    }

    #[test]
    fn test_has_user_template_no_dir() {
        let registry = TemplateRegistry::new(None);
        assert!(!registry.has_user_template("commit_message_xml"));
    }

    #[test]
    fn test_template_source_no_dir() {
        let registry = TemplateRegistry::new(None);
        let source = registry.template_source("commit_message_xml");
        assert_eq!(source, "embedded");
    }

    #[test]
    fn test_template_source_not_found() {
        let registry = TemplateRegistry::new(None);
        let source = registry.template_source("nonexistent_template");
        assert_eq!(source, "embedded");
    }

    #[test]
    fn test_default_registry() {
        let registry = TemplateRegistry::default();
        // Default registry should have a user templates dir if home dir exists
        if TemplateRegistry::default_user_templates_dir().is_some() {
            assert!(registry.user_templates_dir.is_some());
        }
    }

    #[test]
    fn test_get_template_embedded() {
        let registry = TemplateRegistry::new(None);
        let result = registry.get_template("developer_iteration");
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.is_empty());
        assert!(content.contains("IMPLEMENTATION MODE") || content.contains("Developer"));
    }

    #[test]
    fn test_get_template_not_found() {
        let registry = TemplateRegistry::new(None);
        let result = registry.get_template("nonexistent_template");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TemplateError::TemplateNotFound { .. }
        ));
    }

    #[test]
    fn test_get_partial_embedded() {
        let registry = TemplateRegistry::new(None);
        let partials = get_shared_partials();
        if let Some((name, _)) = partials.iter().next() {
            let result = registry.get_partial(name);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_get_partial_not_found() {
        let registry = TemplateRegistry::new(None);
        let result = registry.get_partial("nonexistent_partial");
        assert!(result.is_err());
    }

    #[test]
    fn test_all_template_names() {
        let registry = TemplateRegistry::new(None);
        let names = registry.all_template_names();
        assert!(!names.is_empty());
        assert!(names.len() >= 20); // At least 20 templates
        assert!(names.contains(&"developer_iteration".to_string()));
        assert!(names.contains(&"commit_message_xml".to_string()));
    }

    #[test]
    fn test_template_metadata() {
        let registry = TemplateRegistry::new(None);
        let metadata = registry.template_metadata("commit_message_xml");
        assert!(metadata.is_some());
        let template = metadata.unwrap();
        assert_eq!(template.name, "commit_message_xml");
        assert!(!template.description.is_empty());
    }

    #[test]
    fn test_template_metadata_not_found() {
        let registry = TemplateRegistry::new(None);
        let metadata = registry.template_metadata("nonexistent");
        assert!(metadata.is_none());
    }

    #[test]
    fn test_template_exists_embedded() {
        let registry = TemplateRegistry::new(None);
        assert!(registry.template_exists("developer_iteration"));
        assert!(registry.template_exists("commit_message_xml"));
    }

    #[test]
    fn test_template_not_exists() {
        let registry = TemplateRegistry::new(None);
        assert!(!registry.template_exists("nonexistent_template"));
    }

    #[test]
    fn test_user_template_path_none() {
        let registry = TemplateRegistry::new(None);
        assert!(registry.user_template_path("developer_iteration").is_none());
    }

    #[test]
    fn test_get_commit_template() {
        let registry = TemplateRegistry::new(None);
        let result = registry.get_template("commit_message_xml");
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_get_reviewer_template() {
        let registry = TemplateRegistry::new(None);
        let result = registry.get_template("standard_review_normal");
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_get_fix_mode_template() {
        let registry = TemplateRegistry::new(None);
        let result = registry.get_template("fix_mode");
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_all_templates_have_content() {
        let registry = TemplateRegistry::new(None);
        for name in registry.all_template_names() {
            let result = registry.get_template(&name);
            assert!(result.is_ok(), "Template '{name}' should load successfully");
            let content = result.unwrap();
            assert!(!content.is_empty(), "Template '{name}' should not be empty");
        }
    }
}
