//! Platform detection
//!
//! Provides OS-specific detection capabilities.

use std::env::consts::OS;
use std::process::Command;

use super::Platform;

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

impl Platform {
    /// Detect the current platform
    pub(crate) fn detect() -> Self {
        match OS {
            "macos" => {
                if has_command("brew") {
                    Self::MacWithBrew
                } else {
                    Self::MacWithoutBrew
                }
            }
            "linux" => detect_linux_distro(),
            "windows" => Self::Windows,
            _ => Self::Unknown,
        }
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
}
