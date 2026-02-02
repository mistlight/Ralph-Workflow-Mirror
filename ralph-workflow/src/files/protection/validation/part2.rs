// Tests for PROMPT.md validation.

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::with_temp_cwd;

    #[test]
    fn test_restore_prompt_if_needed_ok() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "# Test\n\nContent").unwrap();
            assert!(restore_prompt_if_needed().unwrap());
        });
    }

    #[test]
    fn test_restore_prompt_if_needed_missing() {
        with_temp_cwd(|_dir| {
            // No PROMPT.md, no backup
            let result = restore_prompt_if_needed();
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("no valid backup available"));
        });
    }

    #[test]
    fn test_restore_prompt_if_needed_restores_from_backup() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/PROMPT.md.backup", "# Restored\n\nContent").unwrap();

            // File is missing, should restore from backup
            let was_restored = restore_prompt_if_needed().unwrap();
            assert!(!was_restored);

            // Verify PROMPT.md exists with backup content
            let content = fs::read_to_string("PROMPT.md").unwrap();
            assert_eq!(content, "# Restored\n\nContent");
        });
    }

    #[test]
    fn test_restore_prompt_if_needed_empty_file() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write("PROMPT.md", "").unwrap();
            fs::write(".agent/PROMPT.md.backup", "# Restored\n\nContent").unwrap();

            // File is empty, should restore from backup
            let was_restored = restore_prompt_if_needed().unwrap();
            assert!(!was_restored);

            // Verify PROMPT.md has backup content
            let content = fs::read_to_string("PROMPT.md").unwrap();
            assert_eq!(content, "# Restored\n\nContent");
        });
    }

    #[test]
    fn test_restore_prompt_if_needed_empty_backup() {
        with_temp_cwd(|_dir| {
            fs::create_dir_all(".agent").unwrap();
            fs::write(".agent/PROMPT.md.backup", "").unwrap();

            // Backup is empty, should fail
            let result = restore_prompt_if_needed();
            assert!(result.is_err());
            // Error should mention no valid backup (since empty backup is skipped)
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("no valid backup available"));
        });
    }

    #[test]
    fn test_validate_prompt_md_not_exists() {
        with_temp_cwd(|_dir| {
            let result = validate_prompt_md(false, false);
            assert!(!result.exists());
            assert!(!result.is_valid());
            assert!(result.errors.iter().any(|e| e.contains("not found")));
            // Verify Work Guide suggestion is included
            assert!(result
                .errors
                .iter()
                .any(|e| e.contains("--list-work-guides") || e.contains("--init")));
        });
    }

    #[test]
    fn test_validate_prompt_md_empty() {
        with_temp_cwd(|_dir| {
            fs::write("PROMPT.md", "   \n\n  ").unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.exists());
            assert!(!result.has_content());
            assert!(!result.is_valid());
            assert!(result.errors.iter().any(|e| e.contains("empty")));
        });
    }

    #[test]
    fn test_validate_prompt_md_complete() {
        with_temp_cwd(|_dir| {
            fs::write(
                "PROMPT.md",
                "# PROMPT

## Goal
Build a feature

## Acceptance
- Tests pass
",
            )
            .unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.exists());
            assert!(result.has_content());
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
            let result = validate_prompt_md(false, false);
            assert!(result.exists());
            assert!(result.has_content());
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
            let result = validate_prompt_md(true, false);
            assert!(result.exists());
            assert!(result.has_content());
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
                "## Goal
Test

## Acceptance Criteria
- Pass
",
            )
            .unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.has_acceptance);

            // Test lowercase "acceptance" variant
            fs::write(
                "PROMPT.md",
                "## Goal
Test

The acceptance tests should pass.
",
            )
            .unwrap();
            let result = validate_prompt_md(false, false);
            assert!(result.has_acceptance);
        });
    }
}

#[cfg(all(test, feature = "test-utils"))]
mod workspace_tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;

    #[test]
    fn test_validate_prompt_md_with_workspace_not_exists() {
        let workspace = MemoryWorkspace::new_test();

        let result = validate_prompt_md_with_workspace(&workspace, false, false);

        assert!(!result.exists());
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.contains("not found")));
    }

    #[test]
    fn test_validate_prompt_md_with_workspace_valid() {
        let workspace = MemoryWorkspace::new_test().with_file(
            "PROMPT.md",
            "# Test\n\n## Goal\nDo something\n\n## Acceptance\n- Pass",
        );

        let result = validate_prompt_md_with_workspace(&workspace, false, false);

        assert!(result.exists());
        assert!(result.has_content());
        assert!(result.has_goal);
        assert!(result.has_acceptance);
        assert!(result.is_valid());
    }

    #[test]
    fn test_validate_prompt_md_with_workspace_restores_from_backup() {
        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/PROMPT.md.backup", "## Goal\nRestored");

        let result = validate_prompt_md_with_workspace(&workspace, false, false);

        // Should have restored from backup
        assert!(result.warnings.iter().any(|w| w.contains("restored from")));
        assert!(result.has_goal);
        // PROMPT.md should now exist in workspace
        assert!(workspace.exists(Path::new("PROMPT.md")));
    }

    #[test]
    fn test_validate_prompt_md_with_workspace_empty() {
        let workspace = MemoryWorkspace::new_test().with_file("PROMPT.md", "   \n\n  ");

        let result = validate_prompt_md_with_workspace(&workspace, false, false);

        assert!(result.exists());
        assert!(!result.has_content());
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.contains("empty")));
    }

    #[test]
    fn test_validate_prompt_md_with_workspace_missing_sections_lenient() {
        let workspace = MemoryWorkspace::new_test().with_file("PROMPT.md", "Just some content");

        let result = validate_prompt_md_with_workspace(&workspace, false, false);

        assert!(result.is_valid()); // Lenient mode: warnings, not errors
        assert!(!result.has_goal);
        assert!(!result.has_acceptance);
        assert_eq!(result.warnings.len(), 2);
    }

    #[test]
    fn test_validate_prompt_md_with_workspace_missing_sections_strict() {
        let workspace = MemoryWorkspace::new_test().with_file("PROMPT.md", "Just some content");

        let result = validate_prompt_md_with_workspace(&workspace, true, false);

        assert!(!result.is_valid()); // Strict mode: errors
        assert_eq!(result.errors.len(), 2);
    }
}
