/// Shell completion generation flag.
#[derive(Parser, Debug, Default)]
pub struct CompletionFlag {
    /// Generate shell completion script
    #[arg(
        long,
        value_name = "SHELL",
        value_enum,
        help = "Generate shell completion script (bash, zsh, fish, elvish, powershell)",
        hide = true
    )]
    pub generate_completion: Option<Shell>,
}

/// Supported shell types for completion generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    /// Bash shell
    Bash,
    /// Zsh shell
    Zsh,
    /// Fish shell
    Fish,
    /// Elvish shell
    Elvish,
    /// `pwsh` (`PowerShell`) shell
    Pwsh,
}
