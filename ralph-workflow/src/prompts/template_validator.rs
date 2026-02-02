//! Template validation and inspection module.
//!
//! Provides functionality for validating template syntax, extracting variables,
//! and checking template integrity.
//!
//! This module is organized into sub-modules:
//! - `template_types`: Type definitions for validation results and errors
//! - `rendered_validation`: Validation of rendered prompts for unresolved placeholders
//! - `template_extraction`: Extraction of variables, partials, and metadata
//! - `syntax_validation`: Syntax checking for template structure

use std::collections::HashSet;

// Sub-modules
#[path = "rendered_validation.rs"]
mod rendered_validation;
#[path = "syntax_validation.rs"]
mod syntax_validation;
#[path = "template_extraction.rs"]
mod template_extraction;
#[path = "template_types.rs"]
mod template_types;

// Re-export public types and functions that are currently used
// Note: TemplateMetadata and VariableInfo are defined in template_types.rs
// but not re-exported here because they're not currently used by any consumers.
// If needed in the future, they can be added to this re-export list.
pub use rendered_validation::{
    validate_no_unresolved_placeholders, validate_no_unresolved_placeholders_with_ignored_content,
};
pub use syntax_validation::validate_syntax;
pub use template_extraction::{extract_metadata, extract_partials, extract_variables};
pub use template_types::{
    RenderedPromptError, TemplateVariablesInvalidError, ValidationError, ValidationResult,
    ValidationWarning,
};

/// Validate a complete template.
///
/// Performs comprehensive validation including syntax checking,
/// variable extraction, and partial reference validation.
pub fn validate_template(content: &str, available_partials: &HashSet<String>) -> ValidationResult {
    let mut is_valid = true;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Validate syntax
    let syntax_errors = validate_syntax(content);
    if !syntax_errors.is_empty() {
        is_valid = false;
        errors.extend(syntax_errors);
    }

    // Extract variables
    let variables = extract_variables(content);

    // Extract partials
    let partials = extract_partials(content);

    // Check for missing partials
    for partial in &partials {
        if !available_partials.contains(partial) {
            is_valid = false;
            errors.push(ValidationError::PartialNotFound {
                name: partial.clone(),
            });
        }
    }

    // Check for variables without defaults that might error
    for var in &variables {
        if !var.has_default {
            warnings.push(ValidationWarning::VariableMayError {
                name: var.name.clone(),
            });
        }
    }

    ValidationResult {
        is_valid,
        variables,
        partials,
        errors,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_template_complete() {
        let content = "Hello {{NAME|default=\"Guest\"}}";
        let partials = HashSet::new();
        let result = validate_template(content, &partials);

        assert!(result.is_valid);
        assert_eq!(result.variables.len(), 1);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_template_with_missing_partial() {
        let content = "{{> missing_partial}}";
        let partials = HashSet::new();
        let result = validate_template(content, &partials);

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }
}
