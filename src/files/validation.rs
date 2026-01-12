//! PROMPT.md validation utilities.
//!
//! Validates the structure and content of PROMPT.md files to ensure
//! they have the required sections for the pipeline to work effectively.

use std::fs;
use std::path::Path;

/// Result of PROMPT.md validation.
///
/// Contains flags indicating what was found and any errors or warnings.
#[derive(Debug, Clone)]
pub struct PromptValidationResult {
    /// Whether PROMPT.md exists
    pub exists: bool,
    /// Whether PROMPT.md has non-empty content
    pub has_content: bool,
    /// Whether a Goal section was found
    pub has_goal: bool,
    /// Whether an Acceptance section was found
    pub has_acceptance: bool,
    /// List of warnings (non-blocking issues)
    pub warnings: Vec<String>,
    /// List of errors (blocking issues)
    pub errors: Vec<String>,
}

impl PromptValidationResult {
    /// Returns true if validation passed (no errors).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns true if validation passed with no warnings.
    pub fn is_perfect(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }
}

/// Validate PROMPT.md structure and content.
///
/// Checks for:
/// - File existence and non-empty content
/// - Goal section (## Goal or # Goal)
/// - Acceptance section (## Acceptance, Acceptance Criteria, or acceptance)
///
/// # Arguments
///
/// * `strict` - In strict mode, missing sections are errors; otherwise they're warnings.
///
/// # Returns
///
/// A `PromptValidationResult` containing validation findings.
pub fn validate_prompt_md(strict: bool) -> PromptValidationResult {
    let prompt_path = Path::new("PROMPT.md");
    let mut result = PromptValidationResult {
        exists: prompt_path.exists(),
        has_content: false,
        has_goal: false,
        has_acceptance: false,
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    if !result.exists {
        result.errors.push("PROMPT.md not found".to_string());
        return result;
    }

    let content = match fs::read_to_string(prompt_path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("Failed to read PROMPT.md: {}", e));
            return result;
        }
    };

    result.has_content = !content.trim().is_empty();
    if !result.has_content {
        result.errors.push("PROMPT.md is empty".to_string());
        return result;
    }

    // Check for Goal section
    result.has_goal = content.contains("## Goal") || content.contains("# Goal");
    if !result.has_goal {
        let msg = "PROMPT.md missing '## Goal' section".to_string();
        if strict {
            result.errors.push(msg);
        } else {
            result.warnings.push(msg);
        }
    }

    // Check for Acceptance section
    result.has_acceptance = content.contains("## Acceptance")
        || content.contains("# Acceptance")
        || content.contains("Acceptance Criteria")
        || content.to_lowercase().contains("acceptance");
    if !result.has_acceptance {
        let msg = "PROMPT.md missing acceptance checks section".to_string();
        if strict {
            result.errors.push(msg);
        } else {
            result.warnings.push(msg);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::testing::with_temp_cwd;

    #[test]
    fn test_validate_prompt_md_not_exists() {
        with_temp_cwd(|_dir| {
            let result = validate_prompt_md(false);
            assert!(!result.exists);
            assert!(!result.is_valid());
            assert!(result.errors.iter().any(|e| e.contains("not found")));
        });
    }

    #[test]
    fn test_validate_prompt_md_empty() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "   \n\n  ").unwrap();
            let result = validate_prompt_md(false);
            assert!(result.exists);
            assert!(!result.has_content);
            assert!(!result.is_valid());
            assert!(result.errors.iter().any(|e| e.contains("empty")));
        });
    }

    #[test]
    fn test_validate_prompt_md_complete() {
        with_temp_cwd(|_dir| {
            fs::write(
                "PROMPT.md",
                r#"# PROMPT

## Goal
Build a feature

## Acceptance
- Tests pass
"#,
            )
            .unwrap();
            let result = validate_prompt_md(false);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(result.has_goal);
            assert!(result.has_acceptance);
            assert!(result.is_valid());
            assert!(result.is_perfect());
        });
    }

    #[test]
    fn test_validate_prompt_md_missing_sections_lenient() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "Just some random content").unwrap();
            let result = validate_prompt_md(false);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(!result.has_goal);
            assert!(!result.has_acceptance);
            // In lenient mode, missing sections are warnings, not errors
            assert!(result.is_valid());
            assert!(!result.is_perfect());
            assert_eq!(result.warnings.len(), 2);
        });
    }

    #[test]
    fn test_validate_prompt_md_missing_sections_strict() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "Just some random content").unwrap();
            let result = validate_prompt_md(true);
            assert!(result.exists);
            assert!(result.has_content);
            assert!(!result.has_goal);
            assert!(!result.has_acceptance);
            // In strict mode, missing sections are errors
            assert!(!result.is_valid());
            assert_eq!(result.errors.len(), 2);
        });
    }

    #[test]
    fn test_validate_prompt_md_acceptance_variations() {
        with_temp_cwd(|_dir| {
            // Test "Acceptance Criteria" variant
            fs::write(
                "PROMPT.md",
                r#"## Goal
Test

## Acceptance Criteria
- Pass
"#,
            )
            .unwrap();
            let result = validate_prompt_md(false);
            assert!(result.has_acceptance);

            // Test lowercase "acceptance" variant
            fs::write(
                "PROMPT.md",
                r#"## Goal
Test

The acceptance tests should pass.
"#,
            )
            .unwrap();
            let result = validate_prompt_md(false);
            assert!(result.has_acceptance);
        });
    }
}
