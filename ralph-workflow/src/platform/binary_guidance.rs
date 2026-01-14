//! Binary-specific installation guidance
//!
//! Provides installation instructions for known AI coding tools.

use super::{known_binaries, Platform};

/// Installation guidance for a missing binary
#[derive(Debug)]
pub struct InstallGuidance {
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
        let mut guidance = Self {
            binary: binary.to_string(),
            install_cmd: None,
            alternative: None,
            notes: Vec::new(),
        };

        // Check if this is a known binary with specific guidance
        if !known_binaries::add_known_binary_guidance(&mut guidance, binary, platform) {
            // Generic binary - provide platform-specific package manager hints
            add_generic_guidance(&mut guidance, binary, platform);
        }

        guidance
    }

    /// Format the guidance as a user-friendly message
    pub(crate) fn format(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!("Binary '{}' not found in PATH.", self.binary));
        lines.push(String::new());

        for note in &self.notes {
            lines.push(format!("  {note}"));
        }

        if let Some(ref cmd) = self.install_cmd {
            lines.push(String::new());
            lines.push("  To install:".to_string());
            lines.push(format!("    {cmd}"));
        }

        if let Some(ref alt) = self.alternative {
            lines.push(format!("  Or: {alt}"));
        }

        lines.join("\n")
    }
}

/// Add generic platform-specific installation guidance for unknown binaries
fn add_generic_guidance(guidance: &mut InstallGuidance, binary: &str, platform: Platform) {
    match platform {
        Platform::MacWithBrew => {
            guidance.install_cmd = Some(format!("brew install {binary}"));
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
            guidance.alternative = Some(format!("Then: brew install {binary}"));
        }
        Platform::DebianLinux => {
            guidance.install_cmd = Some(format!("sudo apt-get install {binary}"));
            guidance
                .notes
                .push("Or check if available via npm/pip".to_string());
        }
        Platform::RhelLinux => {
            guidance.install_cmd = Some(format!("sudo dnf install {binary}"));
            guidance
                .notes
                .push("Or check if available via npm/pip".to_string());
        }
        Platform::ArchLinux => {
            guidance.install_cmd = Some(format!("sudo pacman -S {binary}"));
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
            guidance.install_cmd = Some(format!("winget install {binary}"));
            guidance.alternative = Some("Or use Chocolatey: choco install ...".to_string());
        }
        Platform::Unknown => {
            guidance
                .notes
                .push("Check the tool's documentation for installation instructions".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
