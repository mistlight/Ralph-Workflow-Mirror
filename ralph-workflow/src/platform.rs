//! Platform detection and installation guidance
//!
//! Provides OS-specific suggestions for installing missing dependencies.

use std::env::consts::OS;
use std::process::Command;

/// Detected platform type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    /// macOS with Homebrew available
    MacWithBrew,
    /// macOS without Homebrew
    MacWithoutBrew,
    /// Debian/Ubuntu Linux (apt-based)
    DebianLinux,
    /// RHEL/Fedora Linux (dnf-based)
    RhelLinux,
    /// Arch Linux (pacman-based)
    ArchLinux,
    /// Generic Linux (unknown package manager)
    GenericLinux,
    /// Windows
    Windows,
    /// Unknown platform
    Unknown,
}

impl Platform {
    /// Detect the current platform
    pub(crate) fn detect() -> Self {
        match OS {
            "macos" => {
                if has_command("brew") {
                    Platform::MacWithBrew
                } else {
                    Platform::MacWithoutBrew
                }
            }
            "linux" => detect_linux_distro(),
            "windows" => Platform::Windows,
            _ => Platform::Unknown,
        }
    }
}

/// Check if a command exists in PATH
fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect Linux distribution based on available package managers
fn detect_linux_distro() -> Platform {
    // Check for package managers in order of specificity
    if has_command("apt-get") || has_command("apt") {
        Platform::DebianLinux
    } else if has_command("dnf") || has_command("yum") {
        Platform::RhelLinux
    } else if has_command("pacman") {
        Platform::ArchLinux
    } else {
        Platform::GenericLinux
    }
}

/// Installation guidance for a missing binary
#[derive(Debug)]
pub(crate) struct InstallGuidance {
    /// The binary that was not found
    pub(crate) binary: String,
    /// Primary suggested command to install it
    pub(crate) install_cmd: Option<String>,
    /// Alternative installation method
    pub(crate) alternative: Option<String>,
    /// Additional helpful context
    pub(crate) notes: Vec<String>,
}

impl InstallGuidance {
    /// Generate installation guidance for a missing binary on the current platform
    pub(crate) fn for_binary(binary: &str) -> Self {
        let platform = Platform::detect();
        Self::for_binary_on_platform(binary, platform)
    }

    /// Generate installation guidance for a specific platform
    pub(crate) fn for_binary_on_platform(binary: &str, platform: Platform) -> Self {
        let mut guidance = InstallGuidance {
            binary: binary.to_string(),
            install_cmd: None,
            alternative: None,
            notes: Vec::new(),
        };

        // Known agent binaries with specific installation instructions
        match binary {
            "claude" => {
                guidance
                    .notes
                    .push("Claude Code is Anthropic's AI coding assistant".to_string());
                match platform {
                    Platform::MacWithBrew
                    | Platform::DebianLinux
                    | Platform::RhelLinux
                    | Platform::ArchLinux
                    | Platform::GenericLinux => {
                        guidance.install_cmd =
                            Some("npm install -g @anthropic/claude-code".to_string());
                        guidance.alternative = Some("npx @anthropic/claude-code".to_string());
                    }
                    Platform::MacWithoutBrew => {
                        guidance.install_cmd =
                            Some("npm install -g @anthropic/claude-code".to_string());
                        guidance
                            .notes
                            .push("Requires Node.js. Install via: https://nodejs.org".to_string());
                    }
                    Platform::Windows => {
                        guidance.install_cmd =
                            Some("npm install -g @anthropic/claude-code".to_string());
                    }
                    Platform::Unknown => {
                        guidance.install_cmd =
                            Some("npm install -g @anthropic/claude-code".to_string());
                    }
                }
                guidance
                    .notes
                    .push("After installing, run 'claude auth' to authenticate".to_string());
            }
            "codex" => {
                guidance
                    .notes
                    .push("Codex is OpenAI's AI coding assistant".to_string());
                guidance.install_cmd = Some("npm install -g @openai/codex".to_string());
                guidance
                    .notes
                    .push("Requires OPENAI_API_KEY environment variable".to_string());
            }
            "aider" => {
                guidance
                    .notes
                    .push("Aider is an AI pair programming tool".to_string());
                match platform {
                    Platform::MacWithBrew => {
                        guidance.install_cmd = Some("brew install aider".to_string());
                        guidance.alternative = Some("pip install aider-chat".to_string());
                    }
                    _ => {
                        guidance.install_cmd = Some("pip install aider-chat".to_string());
                        guidance.alternative = Some("pipx install aider-chat".to_string());
                    }
                }
            }
            "opencode" => {
                guidance
                    .notes
                    .push("OpenCode is an AI coding tool".to_string());
                guidance.install_cmd = Some("See https://opencode.ai for installation".to_string());
            }
            "goose" => {
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
            // Generic binary - provide platform-specific package manager hints
            _ => match platform {
                Platform::MacWithBrew => {
                    guidance.install_cmd = Some(format!("brew install {}", binary));
                    guidance
                        .notes
                        .push("Or check if available via npm/pip".to_string());
                }
                Platform::MacWithoutBrew => {
                    guidance
                        .notes
                        .push("Consider installing Homebrew first:".to_string());
                    guidance.install_cmd = Some(
                            "/bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"".to_string()
                        );
                    guidance.alternative = Some(format!("Then: brew install {}", binary));
                }
                Platform::DebianLinux => {
                    guidance.install_cmd = Some(format!("sudo apt-get install {}", binary));
                    guidance
                        .notes
                        .push("Or check if available via npm/pip".to_string());
                }
                Platform::RhelLinux => {
                    guidance.install_cmd = Some(format!("sudo dnf install {}", binary));
                    guidance
                        .notes
                        .push("Or check if available via npm/pip".to_string());
                }
                Platform::ArchLinux => {
                    guidance.install_cmd = Some(format!("sudo pacman -S {}", binary));
                    guidance
                        .notes
                        .push("Or check the AUR if not in official repos".to_string());
                }
                Platform::GenericLinux => {
                    guidance
                        .notes
                        .push("Check your distribution's package manager".to_string());
                    guidance
                        .notes
                        .push("Or try: npm/pip/cargo install".to_string());
                }
                Platform::Windows => {
                    guidance.install_cmd = Some(format!("winget install {}", binary));
                    guidance.alternative = Some("Or use Chocolatey: choco install ...".to_string());
                }
                Platform::Unknown => {
                    guidance.notes.push(
                        "Check the tool's documentation for installation instructions".to_string(),
                    );
                }
            },
        }

        guidance
    }

    /// Format the guidance as a user-friendly message
    pub(crate) fn format(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!("Binary '{}' not found in PATH.", self.binary));
        lines.push(String::new());

        for note in &self.notes {
            lines.push(format!("  {}", note));
        }

        if let Some(ref cmd) = self.install_cmd {
            lines.push(String::new());
            lines.push("  To install:".to_string());
            lines.push(format!("    {}", cmd));
        }

        if let Some(ref alt) = self.alternative {
            lines.push(format!("  Or: {}", alt));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detect_returns_valid_platform() {
        let platform = Platform::detect();
        // Should return some valid platform based on current OS
        assert!(matches!(
            platform,
            Platform::MacWithBrew
                | Platform::MacWithoutBrew
                | Platform::DebianLinux
                | Platform::RhelLinux
                | Platform::ArchLinux
                | Platform::GenericLinux
                | Platform::Windows
                | Platform::Unknown
        ));
    }

    #[test]
    fn test_install_guidance_for_claude() {
        let guidance = InstallGuidance::for_binary_on_platform("claude", Platform::MacWithBrew);
        assert_eq!(guidance.binary, "claude");
        assert!(guidance.install_cmd.is_some());
        assert!(guidance.install_cmd.as_ref().unwrap().contains("npm"));
        assert!(!guidance.notes.is_empty());
    }

    #[test]
    fn test_install_guidance_for_aider_mac() {
        let guidance = InstallGuidance::for_binary_on_platform("aider", Platform::MacWithBrew);
        assert!(guidance.install_cmd.as_ref().unwrap().contains("brew"));
    }

    #[test]
    fn test_install_guidance_for_aider_linux() {
        let guidance = InstallGuidance::for_binary_on_platform("aider", Platform::DebianLinux);
        assert!(guidance.install_cmd.as_ref().unwrap().contains("pip"));
    }

    #[test]
    fn test_install_guidance_for_unknown_binary_mac_with_brew() {
        let guidance =
            InstallGuidance::for_binary_on_platform("unknown-tool", Platform::MacWithBrew);
        assert!(guidance
            .install_cmd
            .as_ref()
            .unwrap()
            .contains("brew install unknown-tool"));
    }

    #[test]
    fn test_install_guidance_for_unknown_binary_debian() {
        let guidance =
            InstallGuidance::for_binary_on_platform("unknown-tool", Platform::DebianLinux);
        assert!(guidance.install_cmd.as_ref().unwrap().contains("apt-get"));
    }

    #[test]
    fn test_install_guidance_mac_without_brew() {
        let guidance =
            InstallGuidance::for_binary_on_platform("unknown-tool", Platform::MacWithoutBrew);
        // Should suggest installing Homebrew first
        assert!(guidance.install_cmd.as_ref().unwrap().contains("Homebrew"));
    }

    #[test]
    fn test_install_guidance_format() {
        let guidance = InstallGuidance::for_binary_on_platform("claude", Platform::MacWithBrew);
        let formatted = guidance.format();
        assert!(formatted.contains("claude"));
        assert!(formatted.contains("not found"));
        assert!(formatted.contains("install"));
    }
}
