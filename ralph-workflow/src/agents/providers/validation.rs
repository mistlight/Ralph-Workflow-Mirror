//! Validation Functions
//!
//! Model flag validation and authentication failure advice.

use super::detection::strip_model_flag_prefix;
use super::types::OpenCodeProviderType;

/// Validate a model flag and return provider-specific warnings if any issues detected.
///
/// Returns a vector of warning messages (empty if no issues).
#[must_use]
pub fn validate_model_flag(model_flag: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    let model = strip_model_flag_prefix(model_flag);
    if model.is_empty() {
        return warnings;
    }

    // Ensure model flag has provider prefix
    if !model.contains('/') {
        warnings.push(format!(
            "Model '{model}' has no provider prefix. Expected format: 'provider/model' (e.g., 'opencode/glm-4.7-free')"
        ));
        return warnings;
    }

    let provider_type = OpenCodeProviderType::from_model_flag(model);

    // Warn about Z.AI vs Zen confusion
    if provider_type == OpenCodeProviderType::OpenCodeZen && model.to_lowercase().contains("zai") {
        warnings.push(
            "Model flag uses 'opencode/' prefix but contains 'zai'. \
             For Z.AI Direct access, use 'zai/' prefix instead."
                .to_string(),
        );
    }

    // Warn about providers requiring cloud configuration
    if provider_type.requires_cloud() {
        warnings.push(format!(
            "{} provider requires cloud configuration. {}",
            provider_type.name(),
            provider_type.auth_command()
        ));
    }

    // Warn about custom/unknown providers
    if provider_type == OpenCodeProviderType::Custom {
        let prefix = model.split('/').next().unwrap_or("");
        warnings.push(format!(
            "Unknown provider prefix '{prefix}'. This may work if OpenCode supports it. \
             Run 'ralph --list-providers' to see known providers."
        ));
    }

    // Info about local providers
    if provider_type.is_local() {
        warnings.push(format!(
            "{} is a local provider. {}",
            provider_type.name(),
            provider_type.auth_command()
        ));
    }

    warnings
}

/// Get provider-specific authentication failure advice based on model flag.
#[must_use]
pub fn auth_failure_advice(model_flag: Option<&str>) -> String {
    match model_flag {
        Some(flag) => {
            let model = strip_model_flag_prefix(flag);
            let prefix = model.split('/').next().unwrap_or("").to_lowercase();
            if matches!(prefix.as_str(), "zai" | "zhipuai") {
                return "Authentication failed for Z.AI provider. Run: opencode auth login -> select 'Z.AI' or 'Z.AI Coding Plan'".to_string();
            }
            let provider = OpenCodeProviderType::from_model_flag(flag);
            format!(
                "Authentication failed for {} provider. Run: {}",
                provider.name(),
                provider.auth_command()
            )
        }
        None => "Check API key or run 'opencode auth login' to authenticate.".to_string(),
    }
}
