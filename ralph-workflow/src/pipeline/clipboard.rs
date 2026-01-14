//! Platform-specific clipboard command configuration.

use crate::platform::Platform;
use std::process::Command;

/// Platform-specific clipboard command configuration.
pub struct ClipboardCommand {
    pub binary: &'static str,
    pub args: &'static [&'static str],
    pub paste_hint: &'static str,
}

/// Get the platform-specific clipboard command.
///
/// Returns None if no clipboard command is available for the current platform.
pub fn get_platform_clipboard_command() -> Option<ClipboardCommand> {
    let platform = Platform::detect();

    match platform {
        Platform::MacWithBrew | Platform::MacWithoutBrew => Some(ClipboardCommand {
            binary: "pbcopy",
            args: &[],
            paste_hint: "pbpaste to view",
        }),
        Platform::DebianLinux
        | Platform::RhelLinux
        | Platform::ArchLinux
        | Platform::GenericLinux => {
            // Try wl-copy (Wayland) first, then xclip (X11)
            if Command::new("which")
                .arg("wl-copy")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                Some(ClipboardCommand {
                    binary: "wl-copy",
                    args: &[],
                    paste_hint: "wl-paste to view",
                })
            } else if Command::new("which")
                .arg("xclip")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                Some(ClipboardCommand {
                    binary: "xclip",
                    args: &["-selection", "clipboard"],
                    paste_hint: "xclip -o -selection clipboard to view",
                })
            } else {
                None
            }
        }
        Platform::Windows => Some(ClipboardCommand {
            binary: "clip",
            args: &[],
            paste_hint: "paste to view",
        }),
        Platform::Unknown => None,
    }
}
