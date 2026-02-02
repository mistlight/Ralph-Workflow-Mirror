/// Check if a string contains a GLM-like model name.
///
/// GLM-like models include GLM, ZhipuAI, ZAI, Qwen, and DeepSeek.
/// Use this for detecting GLM models in any context (e.g., prompt selection).
/// For detecting CCS/Claude-based GLM agents specifically (error handling),
/// use `is_glm_like_agent` instead.
pub fn contains_glm_model(s: &str) -> bool {
    let s_lower = s.to_lowercase();
    s_lower.contains("glm")
        || s_lower.contains("zhipuai")
        || s_lower.contains("zai")
        || s_lower.contains("qwen")
        || s_lower.contains("deepseek")
}

/// Check if an agent is a CCS/Claude-based agent using a GLM-like model.
///
/// These agents have known compatibility issues because they use Claude CLI
/// with GLM models via CCS (Claude Code Switch). They require:
/// - The `-p` flag for non-interactive mode
/// - Special error handling for GLM-specific quirks
///
/// This does NOT match OpenCode agents using GLM models, as OpenCode has
/// its own mechanism (`--auto-approve`) and JSON format.
pub fn is_glm_like_agent(s: &str) -> bool {
    let s_lower = s.to_lowercase();

    // Must contain a GLM-like model name
    if !contains_glm_model(&s_lower) {
        return false;
    }

    // Exclude OpenCode agents - they have their own mechanism
    if s_lower.starts_with("opencode") {
        return false;
    }

    // Match CCS agents (ccs/glm, ccs/zai, etc.) or claude-based commands
    s_lower.starts_with("ccs") || s_lower.contains("claude")
}
