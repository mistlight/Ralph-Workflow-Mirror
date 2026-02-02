/// Recovery strategy for checkpoint validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum RecoveryStrategyArg {
    /// Fail fast - require user intervention
    #[default]
    Fail,
    /// Attempt automatic recovery where possible
    Auto,
    /// Warn but continue (not recommended)
    Force,
}

impl From<RecoveryStrategyArg> for crate::checkpoint::recovery::RecoveryStrategy {
    fn from(arg: RecoveryStrategyArg) -> Self {
        match arg {
            RecoveryStrategyArg::Fail => Self::Fail,
            RecoveryStrategyArg::Auto => Self::Auto,
            RecoveryStrategyArg::Force => Self::Force,
        }
    }
}

/// Recovery command flags.
#[derive(Parser, Debug, Default)]
pub struct RecoveryFlags {
    /// Resume from last checkpoint after an interruption
    #[arg(
        long,
        help = "Resume from last checkpoint (if one exists from a previous interrupted run)",
        hide = true
    )]
    pub resume: bool,

    /// Do not offer to resume even if a checkpoint exists
    #[arg(
        long = "no-resume",
        help = "Skip interactive resume prompt even if a checkpoint exists (for CI/automation)",
        hide = true
    )]
    pub no_resume: bool,

    /// Inspect checkpoint without resuming
    #[arg(
        long = "inspect-checkpoint",
        help = "Display checkpoint information without resuming (use with --resume to see saved state)",
        hide = true
    )]
    pub inspect_checkpoint: bool,

    /// Recovery strategy when validation fails (requires hardened-resume feature)
    #[arg(
        long,
        value_enum,
        default_value = "fail",
        help = "Recovery strategy when checkpoint validation fails (fail=stop, auto=attempt recovery, force=continue anyway)"
    )]
    pub recovery_strategy: RecoveryStrategyArg,

    /// Validate setup without running agents (dry run)
    #[arg(
        long,
        help = "Validate configuration and PROMPT.md without running agents",
        hide = true
    )]
    pub dry_run: bool,

    /// Output comprehensive diagnostic information
    #[arg(long, short = 'd', help = "Show system info, agent status, and config for troubleshooting")]
    pub diagnose: bool,

    /// Show extended help with more details
    #[arg(
        long = "extended-help",
        visible_alias = "man",
        help = "Show extended help with shell completion, all presets, and troubleshooting"
    )]
    pub extended_help: bool,
}

/// Rebase control flags.
#[derive(Parser, Debug, Default)]
pub struct RebaseFlags {
    /// Enable automatic rebase before/after pipeline
    ///
    /// When enabled, ralph will automatically rebase to the main branch before
    /// starting development and after the review phase. Default is disabled to
    /// keep operations fast and avoid conflicts.
    #[arg(long, help = "Enable automatic rebase to main branch before and after pipeline")]
    pub with_rebase: bool,

    /// Only perform rebase and exit
    #[arg(
        long,
        help = "Only rebase to main branch, then exit (no pipeline execution)",
        hide = true
    )]
    pub rebase_only: bool,
}
