//! Template Registry for Runtime Template Loading
//!
//! This module provides a centralized template registry that supports:
//! - User-defined template overrides (from `~/.config/ralph/templates/`)
//! - Embedded templates as fallback (compiled into binary)
//!
//! # Note
//!
//! This is the initial implementation. The full template loading refactor
//! will be completed in subsequent steps.

use std::path::PathBuf;

/// Template registry for loading templates from multiple sources.
///
/// The registry maintains a user templates directory for template overrides.
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
    /// * `"embedded"` - Template is embedded (placeholder)
    #[must_use]
    pub fn template_source(&self, name: &str) -> &'static str {
        if self.has_user_template(name) {
            "user"
        } else {
            // For now, assume all other templates are embedded
            // This will be updated when we implement full template loading
            "embedded"
        }
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
}
