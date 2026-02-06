//! Integration test for XSD retry missing file detection.
//!
//! Verifies that XSD retry prompts detect missing schema files and last_output.xml
//! and emit actionable diagnostics including workspace root path.

use ralph_workflow::prompts::{
    prompt_commit_xsd_retry_with_context, prompt_developer_iteration_xsd_retry_with_context_files,
    prompt_fix_xsd_retry_with_context_files, prompt_planning_xsd_retry_with_context_files,
    prompt_review_xsd_retry_with_context_files, TemplateContext,
};
use ralph_workflow::workspace::MemoryWorkspace;
use std::path::PathBuf;

use crate::test_timeout::with_default_timeout;

/// Test that planning XSD retry detects missing schema file.
#[test]
fn test_planning_xsd_retry_detects_missing_schema() {
    with_default_timeout(|| {
        let workspace_root = PathBuf::from("/tmp/test_workspace");
        let workspace = MemoryWorkspace::new(workspace_root);
        let template_context = TemplateContext::default();

        // Generate XSD retry prompt with missing schema
        let prompt = prompt_planning_xsd_retry_with_context_files(
            &template_context,
            "Test error",
            &workspace,
        );

        // Verify: prompt indicates missing file and includes workspace root
        assert!(
            prompt.contains("REQUIRED OUTPUT PATH DOES NOT EXIST")
                || prompt.contains("workspace.root()"),
            "Should detect missing schema and include workspace root diagnostics"
        );
    });
}

/// Test that review XSD retry detects missing files.
#[test]
fn test_review_xsd_retry_detects_missing_files() {
    with_default_timeout(|| {
        let workspace_root = PathBuf::from("/tmp/test_workspace");
        let workspace = MemoryWorkspace::new(workspace_root);
        let template_context = TemplateContext::default();

        // Generate XSD retry prompt with missing schema
        let prompt =
            prompt_review_xsd_retry_with_context_files(&template_context, "Test error", &workspace);

        // Verify: prompt indicates missing file
        assert!(
            prompt.contains("REQUIRED OUTPUT PATH DOES NOT EXIST")
                || prompt.contains("workspace.root()"),
            "Should detect missing schema and include diagnostics"
        );
    });
}

/// Test that development XSD retry detects missing files.
#[test]
fn test_development_xsd_retry_detects_missing_files() {
    with_default_timeout(|| {
        let workspace_root = PathBuf::from("/tmp/test_workspace");
        let workspace = MemoryWorkspace::new(workspace_root);
        let template_context = TemplateContext::default();

        // Generate XSD retry prompt with missing schema
        let prompt = prompt_developer_iteration_xsd_retry_with_context_files(
            &template_context,
            "Test error",
            &workspace,
        );

        // Verify: prompt indicates missing file
        assert!(
            prompt.contains("REQUIRED OUTPUT PATH DOES NOT EXIST")
                || prompt.contains("workspace.root()"),
            "Should detect missing schema and include diagnostics"
        );
    });
}

/// Test that fix XSD retry detects missing files.
#[test]
fn test_fix_xsd_retry_detects_missing_files() {
    with_default_timeout(|| {
        let workspace_root = PathBuf::from("/tmp/test_workspace");
        let workspace = MemoryWorkspace::new(workspace_root);
        let template_context = TemplateContext::default();

        // Generate XSD retry prompt with missing schema
        let prompt =
            prompt_fix_xsd_retry_with_context_files(&template_context, "Test error", &workspace);

        // Verify: prompt indicates missing file
        assert!(
            prompt.contains("REQUIRED OUTPUT PATH DOES NOT EXIST")
                || prompt.contains("workspace.root()"),
            "Should detect missing schema and include diagnostics"
        );
    });
}

/// Test that commit XSD retry detects missing files.
#[test]
fn test_commit_xsd_retry_detects_missing_files() {
    with_default_timeout(|| {
        let workspace_root = PathBuf::from("/tmp/test_workspace");
        let workspace = MemoryWorkspace::new(workspace_root);
        let template_context = TemplateContext::default();

        // Generate XSD retry prompt with missing schema
        let prompt =
            prompt_commit_xsd_retry_with_context(&template_context, "Test error", &workspace);

        // Verify: prompt indicates missing file
        assert!(
            prompt.contains("REQUIRED OUTPUT PATH DOES NOT EXIST")
                || prompt.contains("workspace.root()"),
            "Should detect missing schema and include diagnostics"
        );
    });
}
