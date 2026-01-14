//! Known binary installation guidance
//!
//! Provides installation instructions for specific AI coding tools.

use super::{InstallGuidance, Platform};

/// Add guidance for known AI coding tools
pub(super) fn add_known_binary_guidance(
    guidance: &mut InstallGuidance,
    binary: &str,
    platform: Platform,
) -> bool {
    match binary {
        "claude" => add_claude_guidance(guidance, platform),
        "codex" => add_codex_guidance(guidance),
        "aider" => add_aider_guidance(guidance, platform),
        "opencode" => add_opencode_guidance(guidance),
        "goose" => add_goose_guidance(guidance, platform),
        _ => return false,
    }
    true
}

fn add_claude_guidance(guidance: &mut InstallGuidance, platform: Platform) {
    guidance
        .notes
        .push("Claude Code is Anthropic's AI coding assistant".to_string());
    match platform {
        Platform::MacWithBrew
        | Platform::DebianLinux
        | Platform::RhelLinux
        | Platform::ArchLinux
        | Platform::GenericLinux => {
            guidance.install_cmd = Some("npm install -g @anthropic/claude-code".to_string());
            guidance.alternative = Some("npx @anthropic/claude-code".to_string());
        }
        Platform::MacWithoutBrew => {
            guidance.install_cmd = Some("npm install -g @anthropic/claude-code".to_string());
            guidance
                .notes
                .push("Requires Node.js. Install via: https://nodejs.org".to_string());
        }
        Platform::Windows | Platform::Unknown => {
            guidance.install_cmd = Some("npm install -g @anthropic/claude-code".to_string());
        }
    }
    guidance
        .notes
        .push("After installing, run 'claude auth' to authenticate".to_string());
}

fn add_codex_guidance(guidance: &mut InstallGuidance) {
    guidance
        .notes
        .push("Codex is OpenAI's AI coding assistant".to_string());
    guidance.install_cmd = Some("npm install -g @openai/codex".to_string());
    guidance
        .notes
        .push("Requires OPENAI_API_KEY environment variable".to_string());
}

fn add_aider_guidance(guidance: &mut InstallGuidance, platform: Platform) {
    guidance
        .notes
        .push("Aider is an AI pair programming tool".to_string());
    if platform == Platform::MacWithBrew {
        guidance.install_cmd = Some("brew install aider".to_string());
        guidance.alternative = Some("pip install aider-chat".to_string());
    } else {
        guidance.install_cmd = Some("pip install aider-chat".to_string());
        guidance.alternative = Some("pipx install aider-chat".to_string());
    }
}

fn add_opencode_guidance(guidance: &mut InstallGuidance) {
    guidance
        .notes
        .push("OpenCode is an AI coding tool".to_string());
    guidance.install_cmd = Some("See https://opencode.ai for installation".to_string());
}

fn add_goose_guidance(guidance: &mut InstallGuidance, platform: Platform) {
    guidance
        .notes
        .push("Goose is an AI developer agent".to_string());
    match platform {
        Platform::MacWithBrew => {
            guidance.install_cmd = Some("brew install goose".to_string());
        }
        _ => {
            guidance.install_cmd = Some("pip install goose-ai".to_string());
        }
    }
}
