//! CLI argument definitions.
//!
//! Contains the `Args` struct with clap configuration for command-line parsing.

use clap::Parser;

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
#[command(after_help = "WORKFLOW:\n\
    1. Create PROMPT.md with your requirements\n\
    2. Run: ralph \"feat: implement my feature\"\n\
    3. Ralph runs developer agent (N iterations)\n\
    4. Ralph runs reviewer agent (review -> fix -> re-review)\n\
    5. Changes are committed with the provided message\n\n\
CONFIGURATION:\n\
    Primary config: ~/.config/ralph-workflow.toml (recommended)\n\
    Environment variables (RALPH_*) override config file settings.\n\
    CCS aliases can be defined in the config and used as 'ccs/alias-name'.\n\
    Run 'ralph --init-global' to create the unified config file.\n\
    Run 'ralph --list-agents' to see all configured agents.\n\n\
VERBOSITY LEVELS (-v LEVEL):\n\
    0 = quiet    Minimal output, hide tool inputs (--quiet or -q)\n\
    1 = normal   Balanced output, show tool inputs\n\
    2 = verbose  Default - generous limits for full context\n\
    3 = full     No truncation (--full)\n\
    4 = debug    Max verbosity with raw JSON (--debug)\n\n\
EXAMPLES:\n\
    ralph \"feat: add login button\"              Basic usage\n\
    ralph --quick \"fix: small bug\"              Quick mode (1 dev + 1 review)\n\
    ralph -Q \"feat: rapid prototype\"            Quick mode (shorthand)\n\
    ralph -q \"fix: typo\"                        Quiet mode (same as -v0)\n\
    ralph --full \"feat: complex change\"         Full output (same as -v3)\n\
    ralph --debug \"debug: investigate\"          Debug mode with raw JSON\n\
    ralph --developer-iters 3                    Custom iterations\n\
    ralph --preset opencode                      Use opencode for both\n\
    ralph --developer-agent aider                Use a different agent\n\n\
PLUMBING COMMANDS (for scripting):\n\
    ralph --generate-commit-msg                  Generate message only\n\
    ralph --show-commit-msg                      Display generated message\n\
    ralph --apply-commit                         Commit using generated message\n\n\
ENVIRONMENT VARIABLES:\n\
    RALPH_DEVELOPER_AGENT    Developer agent (from agent_chain)\n\
    RALPH_REVIEWER_AGENT     Reviewer agent (from agent_chain)\n\
    RALPH_DEVELOPER_ITERS    Developer iterations (default: 5)\n\
    RALPH_REVIEWER_REVIEWS   Re-review passes (default: 2)\n\
    RALPH_VERBOSITY          Verbosity level 0-4 (default: 2)\n\
    RALPH_ISOLATION_MODE     Isolation mode on/off (default: 1=on)")]
pub struct Args {
    /// Commit message for the final commit
    #[arg(
        default_value = "chore: apply PROMPT loop + review/fix/review",
        help = "Commit message for the final commit"
    )]
    pub commit_msg: String,

    /// Number of developer iterations (default: 5)
    #[arg(
        long = "developer-iters",
        env = "RALPH_DEVELOPER_ITERS",
        value_name = "N",
        help = "Number of developer agent iterations"
    )]
    pub developer_iters: Option<u32>,

    /// Number of review-fix cycles (N=0 skips review, N=1 is one review-fix cycle, etc.)
    #[arg(
        long = "reviewer-reviews",
        env = "RALPH_REVIEWER_REVIEWS",
        value_name = "N",
        help = "Number of review-fix cycles (0=skip review, 1=one cycle, default: 2)"
    )]
    pub reviewer_reviews: Option<u32>,

    /// Preset for common agent combinations
    #[arg(
        long,
        env = "RALPH_PRESET",
        value_name = "NAME",
        help = "Use a preset agent combination (default, opencode)"
    )]
    pub preset: Option<super::presets::Preset>,

    /// Developer/driver agent to use (from agent_chain.developer)
    #[arg(
        long,
        env = "RALPH_DEVELOPER_AGENT",
        aliases = ["driver-agent"],
        value_name = "AGENT",
        help = "Developer agent for code implementation (default: first in agent_chain.developer)"
    )]
    pub developer_agent: Option<String>,

    /// Reviewer agent to use (from agent_chain.reviewer)
    #[arg(
        long,
        env = "RALPH_REVIEWER_AGENT",
        value_name = "AGENT",
        help = "Reviewer agent for code review (default: first in agent_chain.reviewer)"
    )]
    pub reviewer_agent: Option<String>,

    /// Developer model/provider override (e.g., "-m opencode/glm-4.7-free")
    #[arg(
        long,
        env = "RALPH_DEVELOPER_MODEL",
        value_name = "MODEL_FLAG",
        help = "Model flag for developer agent (e.g., '-m opencode/glm-4.7-free')"
    )]
    pub developer_model: Option<String>,

    /// Reviewer model/provider override (e.g., "-m opencode/claude-sonnet-4")
    #[arg(
        long,
        env = "RALPH_REVIEWER_MODEL",
        value_name = "MODEL_FLAG",
        help = "Model flag for reviewer agent (e.g., '-m opencode/claude-sonnet-4')"
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
        help = "Provider for developer agent: 'opencode' (Zen), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)"
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
        help = "Provider for reviewer agent: 'opencode' (Zen), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)"
    )]
    pub reviewer_provider: Option<String>,

    /// Verbosity level (0=quiet, 1=normal, 2=verbose, 3=full, 4=debug)
    #[arg(
        short,
        long,
        value_name = "LEVEL",
        value_parser = clap::value_parser!(u8).range(0..=4),
        help = "Output verbosity (0=quiet, 1=normal, 2=verbose [default], 3=full, 4=debug); overrides RALPH_VERBOSITY"
    )]
    pub verbosity: Option<u8>,

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
        conflicts_with = "verbosity",
        help = "Full output mode, no truncation (same as -v3)"
    )]
    pub full: bool,

    /// Shorthand for --verbosity=4 (maximum verbosity with raw JSON)
    #[arg(long, conflicts_with = "verbosity", help = "Debug mode (same as -v4)")]
    pub debug: bool,

    /// Quick mode: 1 developer iteration, 1 review pass (fast turnaround)
    #[arg(
        long,
        short = 'Q',
        help = "Quick mode: 1 dev iteration + 1 review (for rapid prototyping)"
    )]
    pub quick: bool,

    /// Disable isolation mode (allow NOTES.md and ISSUES.md to persist)
    #[arg(
        long,
        help = "Disable isolation mode: keep NOTES.md and ISSUES.md between runs"
    )]
    pub no_isolation: bool,

    /// List all configured agents and exit
    #[arg(long, help = "Show all agents from registry and config file")]
    pub list_agents: bool,

    /// List only agents found in PATH and exit
    #[arg(long, help = "Show only agents that are installed and available")]
    pub list_available_agents: bool,

    /// List OpenCode provider types and their configuration
    #[arg(
        long,
        help = "Show OpenCode provider types with model prefixes and auth commands"
    )]
    pub list_providers: bool,

    /// Initialize unified config file and exit (alias for --init-global)
    #[arg(
        long,
        conflicts_with_all = ["init_global", "init_legacy"],
        help = "Create ~/.config/ralph-workflow.toml with default settings (recommended)"
    )]
    pub init: bool,

    /// Initialize unified config file and exit
    #[arg(
        long,
        conflicts_with_all = ["init", "init_legacy"],
        help = "Create ~/.config/ralph-workflow.toml with default settings (recommended)"
    )]
    pub init_global: bool,

    /// Initialize legacy per-repo agents.toml and exit
    #[arg(
        long,
        conflicts_with_all = ["init", "init_global"],
        help = "(Legacy) Create .agent/agents.toml with default settings (not recommended)"
    )]
    pub init_legacy: bool,

    // === Plumbing Commands ===
    // These are low-level operations for scripting and automation
    /// Generate commit message only (writes to .agent/commit-message.txt)
    #[arg(long, help = "Run only the commit message generation phase, then exit")]
    pub generate_commit_msg: bool,

    /// Apply commit using existing .agent/commit-message.txt
    #[arg(
        long,
        help = "Stage all changes and commit using .agent/commit-message.txt"
    )]
    pub apply_commit: bool,

    /// Show the generated commit message and exit
    #[arg(long, help = "Read and display .agent/commit-message.txt")]
    pub show_commit_msg: bool,

    // === Recovery Commands ===
    /// Resume from last checkpoint after an interruption
    #[arg(
        long,
        help = "Resume from last checkpoint (if one exists from a previous interrupted run)"
    )]
    pub resume: bool,

    /// Validate setup without running agents (dry run)
    #[arg(
        long,
        help = "Validate configuration and PROMPT.md without running agents"
    )]
    pub dry_run: bool,

    /// Output comprehensive diagnostic information
    #[arg(
        long,
        help = "Show system info, agent status, and config for troubleshooting"
    )]
    pub diagnose: bool,

    /// Review depth level (standard, comprehensive, security, incremental)
    #[arg(
        long,
        value_name = "LEVEL",
        help = "Review depth: standard (balanced), comprehensive (thorough), security (OWASP-focused), incremental (changed files only)"
    )]
    pub review_depth: Option<String>,

    /// Path to configuration file (default: ~/.config/ralph-workflow.toml)
    #[arg(
        long,
        short = 'c',
        value_name = "PATH",
        help = "Path to configuration file (default: ~/.config/ralph-workflow.toml)"
    )]
    pub config: Option<std::path::PathBuf>,
}
