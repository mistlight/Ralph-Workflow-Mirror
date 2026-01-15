//! Platform detection and installation guidance
//!
//! Provides OS-specific suggestions for installing missing dependencies.

mod binary_guidance;
mod detection;
mod known_binaries;

pub use binary_guidance::InstallGuidance;

/// Detected platform type
///
/// This enum is `pub(crate)` because it is only used internally by the
/// `InstallGuidance` functionality. External code should use `InstallGuidance`
/// which handles platform detection automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
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
