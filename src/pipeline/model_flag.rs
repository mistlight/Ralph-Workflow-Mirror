//! Model flag parsing and provider override utilities.

use crate::agents::strip_model_flag_prefix;

/// Style of model flag used in agent commands.
#[derive(Clone, Copy)]
enum ModelFlagStyle {
    /// `-m <model>` (space-separated)
    DashMSpace,
    /// `-m=<model>` (equals-separated)
    DashMEquals,
    /// `--model <model>` (space-separated)
    DoubleDashModelSpace,
    /// `--model=<model>` (equals-separated)
    DoubleDashModelEquals,
}

/// Detect the model flag style from a model flag string.
fn detect_model_flag_style(model_flag: &str) -> Option<ModelFlagStyle> {
    let s = model_flag.trim_start();
    if s.starts_with("-m=") {
        return Some(ModelFlagStyle::DashMEquals);
    }
    if s.starts_with("--model=") {
        return Some(ModelFlagStyle::DoubleDashModelEquals);
    }
    if s == "-m" || s.starts_with("-m ") || s.starts_with("-m\t") {
        return Some(ModelFlagStyle::DashMSpace);
    }
    if s == "--model" || s.starts_with("--model ") || s.starts_with("--model\t") {
        return Some(ModelFlagStyle::DoubleDashModelSpace);
    }
    None
}

/// Format a model flag using the given style.
fn format_model_flag(style: ModelFlagStyle, model: &str) -> String {
    match style {
        ModelFlagStyle::DashMSpace => format!("-m {}", model),
        ModelFlagStyle::DashMEquals => format!("-m={}", model),
        ModelFlagStyle::DoubleDashModelSpace => format!("--model {}", model),
        ModelFlagStyle::DoubleDashModelEquals => format!("--model={}", model),
    }
}

/// Extract model name from a model flag or full model string.
fn extract_model_name(model_flag: &str) -> &str {
    let model = strip_model_flag_prefix(model_flag);
    // Extract model name after provider prefix (provider/model)
    model.rsplit('/').next().unwrap_or(model)
}

/// Normalize a provider override string.
///
/// Returns `None` if the provider is empty or invalid (contains '/').
fn normalize_provider_override(provider: &str) -> Option<String> {
    let trimmed = provider.trim().trim_matches('/');
    if trimmed.is_empty() || trimmed.contains('/') {
        return None;
    }
    Some(trimmed.to_string())
}

/// Resolve the effective model flag considering provider override.
///
/// Priority:
/// 1. If provider is specified, construct "{provider}/{model_name}"
/// 2. If model is specified, use it directly
/// 3. Otherwise, use agent's configured model_flag
pub(crate) fn resolve_model_with_provider(
    cli_provider: Option<&str>,
    cli_model: Option<&str>,
    agent_model_flag: Option<&str>,
) -> Option<String> {
    let style = detect_model_flag_style(cli_model.unwrap_or(""))
        .or_else(|| detect_model_flag_style(agent_model_flag.unwrap_or("")))
        .unwrap_or(ModelFlagStyle::DashMSpace);

    let base_model = cli_model
        .map(|m| strip_model_flag_prefix(m).trim())
        .filter(|m| !m.is_empty())
        .or_else(|| {
            agent_model_flag
                .map(|m| strip_model_flag_prefix(m).trim())
                .filter(|m| !m.is_empty())
        })?;

    let provider_override = cli_provider.and_then(normalize_provider_override);
    match (provider_override.as_deref(), cli_model) {
        // Provider + model: construct full model flag
        (Some(provider), Some(model)) => {
            let model_name = extract_model_name(model);
            if model_name.is_empty() {
                return Some(format_model_flag(style, base_model));
            }
            Some(format_model_flag(
                style,
                &format!("{}/{}", provider, model_name),
            ))
        }
        // Provider only: use provider with agent's default model
        (Some(provider), None) => {
            let model_name = extract_model_name(base_model);
            if model_name.is_empty() {
                return Some(format_model_flag(style, base_model));
            }
            Some(format_model_flag(
                style,
                &format!("{}/{}", provider, model_name),
            ))
        }
        // Model only: normalize to a full model flag (preserve -m/--model style if present)
        (None, Some(_model)) => Some(format_model_flag(style, base_model)),
        // Neither: use agent's configured model (normalized)
        (None, None) => Some(format_model_flag(style, base_model)),
    }
}

