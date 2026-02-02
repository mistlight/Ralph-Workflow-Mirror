/// Verbosity level shorthand flags (--quiet, --full).
#[derive(Parser, Debug, Default)]
pub struct VerbosityShorthand {
    /// Shorthand for --verbosity=0 (minimal output)
    #[arg(
        short,
        long,
        conflicts_with = "verbosity",
        help = "Quiet mode (same as -v0)"
    )]
    pub quiet: bool,

    /// Shorthand for --verbosity=3 (no truncation)
    #[arg(
        long,
        short,
        conflicts_with = "verbosity",
        help = "Full output mode, no truncation (same as -v3)"
    )]
    pub full: bool,
}

/// Debug verbosity flag.
#[derive(Parser, Debug, Default)]
pub struct DebugVerbosity {
    /// Shorthand for --verbosity=4 (maximum verbosity with raw JSON)
    #[arg(
        long,
        conflicts_with = "verbosity",
        help = "Debug mode (same as -v4)",
        hide = true
    )]
    pub debug: bool,
}
