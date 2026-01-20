//! CLI argument definitions.
//!
//! Contains the `Args` struct with clap configuration for command-line parsing.

use clap::Parser;

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

/// Quick preset mode flags.
#[derive(Parser, Debug, Default)]
pub struct QuickPresets {
    /// Quick mode: 1 developer iteration, 1 review pass (fast turnaround)
    #[arg(
        long,
        short = 'Q',
        help = "Quick mode: 1 dev iteration + 1 review (for rapid prototyping)"
    )]
    pub quick: bool,

    /// Rapid mode: 2 developer iterations, 1 review pass (between quick and standard)
    #[arg(
        long,
        short = 'U',
        help = "Rapid mode: 2 dev iterations + 1 review (fast but more thorough than quick)"
    )]
    pub rapid: bool,

    /// Long mode: 15 developer iterations, 10 review passes (for thorough development)
    #[arg(
        long,
        short = 'L',
        help = "Long mode: 15 dev iterations + 10 reviews (for thorough development)"
    )]
    pub long: bool,
}

/// Standard preset mode flags.
#[derive(Parser, Debug, Default)]
pub struct StandardPresets {
    /// Standard mode: 5 developer iterations, 2 review passes (default workflow)
    #[arg(
        long,
        short = 'S',
        help = "Standard mode: 5 dev iterations + 2 reviews (default workflow)"
    )]
    pub standard: bool,

    /// Thorough mode: 10 developer iterations, 5 review passes (balanced but more than default)
    #[arg(
        long,
        short = 'T',
        help = "Thorough mode: 10 dev iterations + 5 reviews (balanced but thorough)"
    )]
    pub thorough: bool,
}

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
        conflicts_with_all = ["init_global", "init_config", "init_legacy", "init_prompt"],
        help = "Smart init: create config or PROMPT.md (infers from current state)",
        value_name = "TEMPLATE",
        num_args = 0..=1,
        default_missing_value = "",
        // Cannot use possible_values here due to Option<String> type with optional value
        // Completion is handled via --generate-completion
    )]
    pub init: Option<String>,

    /// Force overwrite existing PROMPT.md when using --init or --init-prompt
    #[arg(
        long = "force-overwrite",
        visible_alias = "overwrite",
        help = "Overwrite existing PROMPT.md without prompting (use with --init or --init-prompt)",
        hide = true
    )]
    pub force_init: bool,

    /// Initialize unified config file and exit (explicit alias for config creation)
    #[arg(
        long,
        conflicts_with_all = ["init", "init_global", "init_legacy", "init_prompt"],
        help = "Create ~/.config/ralph-workflow.toml with default settings (recommended)",
        hide = true
    )]
    pub init_config: bool,

    /// Initialize unified config file and exit
    #[arg(
        long,
        conflicts_with_all = ["init", "init_config", "init_legacy", "init_prompt"],
        help = "Create ~/.config/ralph-workflow.toml with default settings (recommended)",
        hide = true
    )]
    pub init_global: bool,
}

/// Legacy initialization flag.
#[derive(Parser, Debug, Default)]
pub struct LegacyInitFlag {
    /// Initialize legacy per-repo agents.toml and exit
    #[arg(
        long,
        conflicts_with_all = ["init", "init_global", "init_prompt"],
        help = "(Legacy) Create .agent/agents.toml with default settings (not recommended)",
        hide = true
    )]
    pub init_legacy: bool,
}

/// Agent listing flags.
#[derive(Parser, Debug, Default)]
pub struct AgentListFlags {
    /// List all configured agents and exit
    #[arg(
        long,
        help = "Show all agents from registry and config file",
        hide = true
    )]
    pub list_agents: bool,

    /// List only agents found in PATH and exit
    #[arg(
        long,
        help = "Show only agents that are installed and available",
        hide = true
    )]
    pub list_available_agents: bool,
}

/// Provider listing flag.
#[derive(Parser, Debug, Default)]
pub struct ProviderListFlag {
    /// List `OpenCode` provider types and their configuration
    #[arg(
        long,
        help = "Show OpenCode provider types with model prefixes and auth commands",
        hide = true
    )]
    pub list_providers: bool,
}

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

/// Template management subcommands.
#[derive(Parser, Debug, Default)]
pub struct TemplateCommands {
    /// Initialize user templates directory with Agent Prompts (backend AI prompts)
    #[arg(
        long = "init-system-prompts",
        alias = "init-templates",
        help = "Create ~/.config/ralph/templates/ with default Agent Prompts (backend AI behavior configuration, NOT Work Guides for PROMPT.md)",
        default_missing_value = "false",
        num_args = 0..=1,
        require_equals = true,
        hide = true
    )]
    pub init_templates: Option<bool>,

    /// Force overwrite existing templates when initializing
    #[arg(
        long,
        requires = "init_templates",
        help = "Overwrite existing system prompt templates during init (use with caution)",
        hide = true
    )]
    pub force: bool,

    /// Validate all templates for syntax errors
    #[arg(
        long,
        help = "Validate all Agent Prompt templates for syntax errors",
        hide = true
    )]
    pub validate: bool,

    /// Show template content and metadata
    #[arg(
        long,
        value_name = "NAME",
        help = "Show Agent Prompt template content and metadata",
        hide = true
    )]
    pub show: Option<String>,

    /// List all prompt templates with their variables
    #[arg(
        long,
        help = "List all Agent Prompt templates with their variables",
        hide = true
    )]
    pub list: bool,

    /// List all templates including deprecated ones
    #[arg(
        long,
        help = "List all Agent Prompt templates including deprecated ones"
    )]
    pub list_all: bool,

    /// Extract variables from a template
    #[arg(
        long,
        value_name = "NAME",
        help = "Extract variables from an Agent Prompt template",
        hide = true
    )]
    pub variables: Option<String>,

    /// Test render a template with provided variables
    #[arg(
        long,
        value_name = "NAME",
        help = "Test render a system prompt template with provided variables",
        hide = true
    )]
    pub render: Option<String>,
}

impl TemplateCommands {
    /// Check if --init-system-prompts or --init-templates flag was provided.
    pub const fn init_templates_enabled(&self) -> bool {
        self.init_templates.is_some()
    }
}

/// Commit message plumbing flags.
#[derive(Parser, Debug, Default)]
pub struct CommitPlumbingFlags {
    /// Generate commit message only (writes to .agent/commit-message.txt)
    #[arg(
        long,
        help = "Run only the commit message generation phase, then exit",
        hide = true
    )]
    pub generate_commit_msg: bool,

    /// Apply commit using existing .agent/commit-message.txt
    #[arg(
        long,
        help = "Stage all changes and commit using .agent/commit-message.txt",
        hide = true
    )]
    pub apply_commit: bool,
}

/// Commit display plumbing flags.
///
/// This groups flags for displaying commit-related information.
#[derive(Parser, Debug, Default)]
pub struct CommitDisplayFlags {
    /// Show the generated commit message and exit
    #[arg(long, help = "Read and display .agent/commit-message.txt", hide = true)]
    pub show_commit_msg: bool,

    /// Reset the starting commit reference to merge-base with main branch
    #[arg(
        long,
        help = "Reset .agent/start_commit to merge-base with main branch (for incremental diff generation)",
        hide = true
    )]
    pub reset_start_commit: bool,

    /// Show the current baseline state and exit
    /// Handler: `cli/handlers/baseline.rs::handle_show_baseline()`
    #[arg(long, help = "Show current review baseline and start commit state")]
    pub show_baseline: bool,
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
    #[arg(
        long,
        short = 'd',
        help = "Show system info, agent status, and config for troubleshooting"
    )]
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
    #[arg(
        long,
        help = "Enable automatic rebase to main branch before and after pipeline"
    )]
    pub with_rebase: bool,

    /// Only perform rebase and exit
    #[arg(
        long,
        help = "Only rebase to main branch, then exit (no pipeline execution)",
        hide = true
    )]
    pub rebase_only: bool,

    /// Skip automatic rebase before/after pipeline (deprecated: use default behavior or --with-rebase)
    #[arg(
        long,
        help = "Skip automatic rebase to main branch before and after pipeline",
        hide = true
    )]
    #[deprecated(
        since = "0.4.2",
        note = "Rebase is now disabled by default; use --with-rebase to enable"
    )]
    pub skip_rebase: bool,
}

/// Ralph: PROMPT-driven agent orchestrator for git repos
#[derive(Parser, Debug)]
#[command(name = "ralph")]
#[command(about = "PROMPT-driven multi-agent orchestrator for git repos")]
#[command(
    long_about = "Ralph orchestrates AI coding agents to implement changes based on PROMPT.md.\n\n\
    It runs a developer agent for code implementation, then a reviewer agent for\n\
    quality assurance, automatically staging and committing the final result."
)]
#[command(version)]
#[command(after_help = "GETTING STARTED:\n\
    ralph --init                 Smart init (infers what you need)\n\
    ralph --init <work-guide>    Create PROMPT.md from a Work Guide\n\
    ralph \"fix: my bug\"         Run the orchestrator\n\
\n\
More help: ralph --extended-help  (shell completion, all presets, troubleshooting)")]
// CLI arguments naturally use many boolean flags. These represent independent
// user choices, not a state machine, so bools are the appropriate type.
pub struct Args {
    /// Verbosity shorthand flags (--quiet, --full)
    #[command(flatten)]
    pub verbosity_shorthand: VerbosityShorthand,

    /// Debug verbosity flag
    #[command(flatten)]
    pub debug_verbosity: DebugVerbosity,

    /// Quick preset mode flags
    #[command(flatten)]
    pub quick_presets: QuickPresets,

    /// Standard preset mode flags
    #[command(flatten)]
    pub standard_presets: StandardPresets,

    /// Unified config initialization flags
    #[command(flatten)]
    pub unified_init: UnifiedInitFlags,

    /// Legacy initialization flag
    #[command(flatten)]
    pub legacy_init: LegacyInitFlag,

    /// Agent listing flags
    #[command(flatten)]
    pub agent_list: AgentListFlags,

    /// Provider listing flag
    #[command(flatten)]
    pub provider_list: ProviderListFlag,

    /// Shell completion generation flag
    #[command(flatten)]
    pub completion: CompletionFlag,

    /// Work Guide listing flag
    #[command(flatten)]
    pub work_guide_list: WorkGuideListFlag,

    /// Template management commands
    #[command(flatten)]
    pub template_commands: TemplateCommands,

    /// Commit message plumbing flags
    #[command(flatten)]
    pub commit_plumbing: CommitPlumbingFlags,

    /// Commit display plumbing flags
    #[command(flatten)]
    pub commit_display: CommitDisplayFlags,

    /// Recovery command flags
    #[command(flatten)]
    pub recovery: RecoveryFlags,

    /// Rebase control flags
    #[command(flatten)]
    pub rebase_flags: RebaseFlags,

    /// Commit message for the final commit
    #[arg(
        default_value = "chore: apply PROMPT loop + review/fix/review",
        help = "Commit message for the final commit"
    )]
    pub commit_msg: String,

    /// Number of developer iterations (default: 5)
    #[arg(
        long = "developer-iters",
        short = 'D',
        env = "RALPH_DEVELOPER_ITERS",
        value_name = "N",
        help = "Number of developer agent iterations",
        aliases = ["developer-iteration", "dev-iter", "d-iters"]
    )]
    pub developer_iters: Option<u32>,

    /// Number of review-fix cycles (N=0 skips review, N=1 is one review-fix cycle, etc.)
    #[arg(
        long = "reviewer-reviews",
        short = 'R',
        env = "RALPH_REVIEWER_REVIEWS",
        value_name = "N",
        help = "Number of review-fix cycles (0=skip review, 1=one cycle, default: 2)",
        aliases = ["reviewer-count", "reviewer-review"]
    )]
    pub reviewer_reviews: Option<u32>,

    /// Preset for common agent combinations
    #[arg(
        long,
        env = "RALPH_PRESET",
        value_name = "NAME",
        help = "Use a preset agent combination (default, opencode)",
        hide = true
    )]
    pub preset: Option<super::presets::Preset>,

    /// Developer/driver agent to use (from `agent_chain.developer`)
    #[arg(
        long,
        short = 'a',
        env = "RALPH_DEVELOPER_AGENT",
        aliases = ["driver-agent", "dev-agent", "developer"],
        value_name = "AGENT",
        help = "Developer agent for code implementation (default: first in agent_chain.developer)"
    )]
    pub developer_agent: Option<String>,

    /// Reviewer agent to use (from `agent_chain.reviewer`)
    #[arg(
        long,
        short = 'r',
        env = "RALPH_REVIEWER_AGENT",
        aliases = ["rev-agent", "reviewer"],
        value_name = "AGENT",
        help = "Reviewer agent for code review (default: first in agent_chain.reviewer)"
    )]
    pub reviewer_agent: Option<String>,

    /// Developer model/provider override (e.g., "-m opencode/glm-4.7-free")
    #[arg(
        long,
        env = "RALPH_DEVELOPER_MODEL",
        value_name = "MODEL_FLAG",
        help = "Model flag for developer agent (e.g., '-m opencode/glm-4.7-free')",
        hide = true
    )]
    pub developer_model: Option<String>,

    /// Reviewer model/provider override (e.g., "-m opencode/claude-sonnet-4")
    #[arg(
        long,
        env = "RALPH_REVIEWER_MODEL",
        value_name = "MODEL_FLAG",
        help = "Model flag for reviewer agent (e.g., '-m opencode/claude-sonnet-4')",
        hide = true
    )]
    pub reviewer_model: Option<String>,

    /// Developer provider override (e.g., "opencode", "zai", "anthropic", "openai")
    /// Use this to switch providers at runtime without changing agent config.
    /// Combined with the agent's model to form the full model flag.
    /// Provider types: 'opencode' (Zen gateway), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)
    #[arg(
        long,
        env = "RALPH_DEVELOPER_PROVIDER",
        value_name = "PROVIDER",
        help = "Provider for developer agent: 'opencode' (Zen), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)",
        hide = true
    )]
    pub developer_provider: Option<String>,

    /// Reviewer provider override (e.g., "opencode", "zai", "anthropic", "openai")
    /// Use this to switch providers at runtime without changing agent config.
    /// Combined with the agent's model to form the full model flag.
    /// Provider types: 'opencode' (Zen gateway), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)
    #[arg(
        long,
        env = "RALPH_REVIEWER_PROVIDER",
        value_name = "PROVIDER",
        help = "Provider for reviewer agent: 'opencode' (Zen), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)",
        hide = true
    )]
    pub reviewer_provider: Option<String>,

    /// JSON parser for the reviewer agent (overrides agent config)
    /// Useful for testing different parsers with problematic agents
    #[arg(
        long,
        env = "RALPH_REVIEWER_JSON_PARSER",
        value_name = "PARSER",
        help = "JSON parser for reviewer (claude, codex, gemini, opencode, generic); overrides agent config",
        hide = true
    )]
    pub reviewer_json_parser: Option<String>,

    /// Verbosity level (0=quiet, 1=normal, 2=verbose, 3=full, 4=debug)
    #[arg(
        short,
        long,
        value_name = "LEVEL",
        value_parser = clap::value_parser!(u8).range(0..=4),
        help = "Output verbosity (0=quiet, 1=normal, 2=verbose [default], 3=full, 4=debug); overrides RALPH_VERBOSITY"
    )]
    pub verbosity: Option<u8>,

    /// Disable isolation mode (allow NOTES.md and ISSUES.md to persist)
    #[arg(
        long,
        help = "Disable isolation mode: keep NOTES.md and ISSUES.md between runs",
        hide = true
    )]
    pub no_isolation: bool,

    /// Review depth level (standard, comprehensive, security, incremental)
    #[arg(
        long,
        value_name = "LEVEL",
        help = "Review depth: standard (balanced), comprehensive (thorough), security (OWASP-focused), incremental (changed files only)",
        hide = true
    )]
    pub review_depth: Option<String>,

    /// Path to configuration file (default: ~/.config/ralph-workflow.toml)
    #[arg(
        long,
        short = 'c',
        value_name = "PATH",
        help = "Path to configuration file (default: ~/.config/ralph-workflow.toml)",
        hide = true
    )]
    pub config: Option<std::path::PathBuf>,

    /// Initialize PROMPT.md from a Work Guide and exit
    ///
    /// This is a legacy alias for `--init <template>`. Consider using `--init` instead.
    ///
    /// Work Guides describe YOUR work to the AI (e.g., bug-fix, feature-spec).
    /// These are different from Agent Prompts which configure AI behavior.
    ///
    /// Available Work Guides:
    /// quick, bug-fix, feature-spec, refactor, test, docs, cli-tool, web-api,
    /// performance-optimization, security-audit, api-integration, database-migration,
    /// dependency-update, data-pipeline, ui-component, code-review, debug-triage,
    /// release, tech-debt, onboarding
    #[arg(
        long,
        value_name = "TEMPLATE",
        help = "Create PROMPT.md from a Work Guide (use --list-work-guides to see options)",
        hide = true
    )]
    pub init_prompt: Option<String>,

    /// Interactive mode: prompt to create PROMPT.md from template when missing
    #[arg(
        long,
        short = 'i',
        help = "Interactive mode: prompt to create PROMPT.md from template when missing",
        hide = true
    )]
    pub interactive: bool,

    /// Git user name override (highest priority in identity resolution chain)
    #[arg(
        long,
        env = "RALPH_GIT_USER_NAME",
        value_name = "NAME",
        help = "Git user name for commits (overrides config, env, and git config)",
        hide = true
    )]
    pub git_user_name: Option<String>,

    /// Git user email override (highest priority in identity resolution chain)
    #[arg(
        long,
        env = "RALPH_GIT_USER_EMAIL",
        value_name = "EMAIL",
        help = "Git user email for commits (overrides config, env, and git config)",
        hide = true
    )]
    pub git_user_email: Option<String>,

    /// Show streaming quality metrics at the end of agent output
    #[arg(
        long,
        help = "Display streaming quality metrics (delta stats, repairs, violations) after agent completion",
        hide = true
    )]
    pub show_streaming_metrics: bool,
}
