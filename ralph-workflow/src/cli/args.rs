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
#[command(
    after_help = "╺━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
\n\
NEW TO RALPH?\n\
    Just want to get started? Run:\n\
        ralph --init-prompt feature-spec    # Create a prompt template\n\
        ralph \"fix: my bug\"                # Run with AI agents\n\
\n\
╺━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
\n\
PRESET MODES (pick how thorough AI should be):\n\
    -Q  Quick:      1 dev + 1 review     (typos, small fixes)\n\
    -U  Rapid:      2 dev + 1 review     (minor changes)\n\
    -S  Standard:   5 dev + 2 reviews    (default for most tasks)\n\
    -T  Thorough:  10 dev + 5 reviews    (complex features)\n\
    -L  Long:      15 dev + 10 reviews   (most thorough)\n\
\n\
╺━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
\n\
COMMON SHORTHANDS:\n\
    -D N, -R N       Set dev iterations and review cycles\n\
    -a AGENT         Pick developer agent (claude, opencode, etc.)\n\
    -r AGENT         Pick reviewer agent\n\
    -v N / -q / -f   Set verbosity (quiet/normal/full)\n\
    -d               Diagnose/show system info\n\
    -i               Interactive mode (prompt if PROMPT.md missing)\n\
    -c PATH          Use specific config file\n\
\n\
╺━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
\n\
QUICK EXAMPLES:\n\
    ralph \"fix: typo\"                Run with default settings\n\
    ralph -Q \"fix: small bug\"        Quick mode for tiny fixes\n\
    ralph -U \"feat: add button\"      Rapid mode for minor features\n\
    ralph -a claude \"fix: bug\"       Use specific agent\n\
    ralph --list-templates            See all prompt templates\n\
╺━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
)]
#[expect(clippy::struct_excessive_bools)]
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

    /// Shorthand for --verbosity=4 (maximum verbosity with raw JSON)
    #[arg(
        long,
        conflicts_with = "verbosity",
        help = "Debug mode (same as -v4)",
        hide = true
    )]
    pub debug: bool,

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

    /// Disable isolation mode (allow NOTES.md and ISSUES.md to persist)
    #[arg(
        long,
        help = "Disable isolation mode: keep NOTES.md and ISSUES.md between runs",
        hide = true
    )]
    pub no_isolation: bool,

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

    /// List `OpenCode` provider types and their configuration
    #[arg(
        long,
        help = "Show OpenCode provider types with model prefixes and auth commands",
        hide = true
    )]
    pub list_providers: bool,

    /// Initialize unified config file and exit (alias for --init-global)
    #[arg(
        long,
        conflicts_with_all = ["init_global", "init_legacy", "init_prompt"],
        help = "Create ~/.config/ralph-workflow.toml with default settings (recommended)",
        hide = true
    )]
    pub init: bool,

    /// Initialize unified config file and exit
    #[arg(
        long,
        conflicts_with_all = ["init", "init_legacy", "init_prompt"],
        help = "Create ~/.config/ralph-workflow.toml with default settings (recommended)",
        hide = true
    )]
    pub init_global: bool,

    /// Initialize legacy per-repo agents.toml and exit
    #[arg(
        long,
        conflicts_with_all = ["init", "init_global", "init_prompt"],
        help = "(Legacy) Create .agent/agents.toml with default settings (not recommended)",
        hide = true
    )]
    pub init_legacy: bool,

    // === Plumbing Commands ===
    // These are low-level operations for scripting and automation
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

    /// Show the generated commit message and exit
    #[arg(long, help = "Read and display .agent/commit-message.txt", hide = true)]
    pub show_commit_msg: bool,

    /// Reset the starting commit reference to current HEAD
    #[arg(
        long,
        help = "Reset .agent/start_commit to current HEAD (for incremental diff generation)",
        hide = true
    )]
    pub reset_start_commit: bool,

    // === Recovery Commands ===
    /// Resume from last checkpoint after an interruption
    #[arg(
        long,
        help = "Resume from last checkpoint (if one exists from a previous interrupted run)",
        hide = true
    )]
    pub resume: bool,

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

    /// Initialize PROMPT.md from template and exit
    #[arg(
        long,
        value_name = "TEMPLATE",
        help = "Create PROMPT.md from a template (use --list-templates to see options)"
    )]
    pub init_prompt: Option<String>,

    /// List available PROMPT.md templates and exit
    #[arg(
        long,
        help = "Show all available PROMPT.md templates with descriptions"
    )]
    pub list_templates: bool,

    /// Interactive mode: prompt to create PROMPT.md from template when missing
    #[arg(
        long,
        short = 'i',
        help = "Interactive mode: prompt to create PROMPT.md from template when missing"
    )]
    pub interactive: bool,

    /// Git user name override (highest priority in identity resolution chain)
    #[arg(
        long,
        env = "RALPH_GIT_USER_NAME",
        value_name = "NAME",
        help = "Git user name for commits (overrides config, env, and git config)"
    )]
    pub git_user_name: Option<String>,

    /// Git user email override (highest priority in identity resolution chain)
    #[arg(
        long,
        env = "RALPH_GIT_USER_EMAIL",
        value_name = "EMAIL",
        help = "Git user email for commits (overrides config, env, and git config)"
    )]
    pub git_user_email: Option<String>,
}
