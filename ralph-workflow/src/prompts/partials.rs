//! Shared template partials for template composition.
//!
//! This module provides common template sections that can be included
//! in other templates using the `{{> partial_name}}` syntax.
//!
//! # Usage
//!
//! ```ignore
//! use crate::prompts::{Template, partials::get_shared_partials};
//!
//! let partials = get_shared_partials();
//! let template = Template::new("{{> shared/_critical_header}}\nContent here");
//! let variables = HashMap::from([("MODE", "REVIEW MODE".to_string())]);
//! let rendered = template.render_with_partials(&variables, &partials)?;
//! ```
//!
//! # Available Partials
//!
//! - `shared/_critical_header` - "CRITICAL: You have NO access" warning
//! - `shared/_context_section` - PROMPT and PLAN context variables
//! - `shared/_diff_section` - DIFF display in code block
//! - `shared/_output_checklist` - Prioritized checklist output format

use std::collections::HashMap;

/// Get all shared partials as a `HashMap`.
///
/// Partials are loaded at compile time via `include_str!` for efficiency.
/// The `HashMap` uses partial name (without .txt extension) as the key.
#[must_use]
pub fn get_shared_partials() -> HashMap<String, String> {
    HashMap::from([
        (
            "shared/_critical_header".to_string(),
            include_str!("templates/shared/_critical_header.txt").to_string(),
        ),
        (
            "shared/_context_section".to_string(),
            include_str!("templates/shared/_context_section.txt").to_string(),
        ),
        (
            "shared/_diff_section".to_string(),
            include_str!("templates/shared/_diff_section.txt").to_string(),
        ),
        (
            "shared/_output_checklist".to_string(),
            include_str!("templates/shared/_output_checklist.txt").to_string(),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_partials_exist() {
        let partials = get_shared_partials();
        assert!(partials.contains_key("shared/_critical_header"));
        assert!(partials.contains_key("shared/_context_section"));
        assert!(partials.contains_key("shared/_diff_section"));
        assert!(partials.contains_key("shared/_output_checklist"));
    }

    #[test]
    fn test_shared_partials_not_empty() {
        let partials = get_shared_partials();
        for (name, content) in &partials {
            assert!(!content.is_empty(), "Partial '{name}' should not be empty");
        }
    }

    #[test]
    fn test_critical_header_contains_mode_variable() {
        let partials = get_shared_partials();
        let header = partials.get("shared/_critical_header").unwrap();
        assert!(header.contains("{{MODE}}"));
    }

    #[test]
    fn test_context_section_contains_variables() {
        let partials = get_shared_partials();
        let context = partials.get("shared/_context_section").unwrap();
        assert!(context.contains("{{PROMPT}}"));
        assert!(context.contains("{{PLAN}}"));
    }

    #[test]
    fn test_diff_section_contains_diff_variable() {
        let partials = get_shared_partials();
        let diff_section = partials.get("shared/_diff_section").unwrap();
        assert!(diff_section.contains("{{DIFF}}"));
    }
}
