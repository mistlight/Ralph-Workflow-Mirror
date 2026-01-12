//! Tests for Provider Types
//!
//! Unit tests for provider detection, validation, and metadata.

use super::*;

#[test]
fn test_strip_model_flag_prefix() {
    assert_eq!(
        strip_model_flag_prefix("-m opencode/glm-4.7-free"),
        "opencode/glm-4.7-free"
    );
    assert_eq!(
        strip_model_flag_prefix("--model opencode/glm-4.7-free"),
        "opencode/glm-4.7-free"
    );
    assert_eq!(
        strip_model_flag_prefix("-m=opencode/glm-4.7-free"),
        "opencode/glm-4.7-free"
    );
    assert_eq!(
        strip_model_flag_prefix("opencode/glm-4.7-free"),
        "opencode/glm-4.7-free"
    );
}

#[test]
fn test_provider_type_from_model_flag() {
    assert_eq!(
        OpenCodeProviderType::from_model_flag("opencode/glm-4.7-free"),
        OpenCodeProviderType::OpenCodeZen
    );
    assert_eq!(
        OpenCodeProviderType::from_model_flag("zai/glm-4.7"),
        OpenCodeProviderType::ZaiDirect
    );
    assert_eq!(
        OpenCodeProviderType::from_model_flag("anthropic/claude-sonnet-4"),
        OpenCodeProviderType::Anthropic
    );
    assert_eq!(
        OpenCodeProviderType::from_model_flag("unknown/model"),
        OpenCodeProviderType::Custom
    );
}

#[test]
fn test_validate_model_flag() {
    // Valid flag
    let warnings = validate_model_flag("opencode/glm-4.7-free");
    assert!(warnings.is_empty());

    // Missing prefix
    let warnings = validate_model_flag("glm-4.7-free");
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no provider prefix"));

    // Unknown provider
    let warnings = validate_model_flag("unknown/model");
    assert!(!warnings.is_empty());
}

#[test]
fn test_auth_failure_advice() {
    let advice = auth_failure_advice(Some("anthropic/claude-sonnet-4"));
    assert!(advice.contains("Anthropic"));
    assert!(advice.contains("ANTHROPIC_API_KEY"));

    let advice = auth_failure_advice(None);
    assert!(advice.contains("opencode auth login"));
}
