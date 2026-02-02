/// Unified config initialization flags.
#[derive(Parser, Debug, Default)]
pub struct UnifiedInitFlags {
    /// Smart initialization: creates setup based on current state
    ///
    /// This is the RECOMMENDED way to get started with Ralph.
    ///
    /// Behavior:
    ///   --init              (no value)   → Smart mode: infers what you need
    ///   --init bug-fix      (with value) → Create PROMPT.md from specific Work Guide
    ///
    /// Smart mode (no value):
    ///   - No config? Creates config at ~/.config/ralph-workflow.toml
    ///   - Config exists, no PROMPT.md? Creates or prompts for PROMPT.md
    ///   - Both exist? Shows helpful status message and exits
    ///
    /// Work Guides (for PROMPT.md):
    ///   quick, bug-fix, feature-spec, refactor, test, docs, cli-tool, web-api,
    ///   performance-optimization, security-audit, api-integration, database-migration,
    ///   dependency-update, data-pipeline, ui-component, code-review, debug-triage,
    ///   release, tech-debt, onboarding
    ///
    /// Note: These are Work Guides for YOUR work descriptions, NOT Agent Prompts.
    /// See --help for details on the difference.
    #[arg(
        long,
        conflicts_with_all = ["init_global", "init_config"],
        help = "Smart init: create config or PROMPT.md (infers from current state)",
        value_name = "TEMPLATE",
        num_args = 0..=1,
        default_missing_value = "",
        // Cannot use possible_values here due to Option<String> type with optional value
        // Completion is handled via --generate-completion
    )]
    pub init: Option<String>,

    /// Force overwrite existing PROMPT.md when using --init
    #[arg(
        long = "force-overwrite",
        visible_alias = "overwrite",
        help = "Overwrite existing PROMPT.md without prompting (use with --init)",
        hide = true
    )]
    pub force_init: bool,

    /// Initialize unified config file and exit (explicit alias for config creation)
    #[arg(
        long,
        conflicts_with_all = ["init", "init_global"],
        help = "Create ~/.config/ralph-workflow.toml with default settings (recommended)",
        hide = true
    )]
    pub init_config: bool,

    /// Initialize unified config file and exit
    #[arg(
        long,
        conflicts_with_all = ["init", "init_config"],
        help = "Create ~/.config/ralph-workflow.toml with default settings (recommended)",
        hide = true
    )]
    pub init_global: bool,
}

/// Work Guide listing flag.
#[derive(Parser, Debug, Default)]
pub struct WorkGuideListFlag {
    /// List available PROMPT.md Work Guides and exit
    #[arg(
        long = "list-work-guides",
        visible_alias = "list-templates",
        help = "Show all available Work Guides for PROMPT.md"
    )]
    pub list_work_guides: bool,
}
