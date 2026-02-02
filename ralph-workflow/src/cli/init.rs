//! Configuration initialization handlers.
//!
//! This module handles the `--init` and `--init-global` flags for creating
//! default unified configuration files and PROMPT.md from templates.
//!
//! # Dependency Injection
//!
//! All init handlers accept a [`ConfigEnvironment`] for path resolution, enabling
//! tests to inject custom paths without relying on environment variables.
//!
//! For convenience, wrapper functions without the resolver parameter are provided
//! that use [`RealConfigEnvironment`] internally.

use crate::config::{ConfigEnvironment, RealConfigEnvironment};
use crate::logger::Colors;
use crate::templates::{get_template, list_templates, ALL_TEMPLATES};
use std::io::IsTerminal;
use std::path::Path;

/// Minimum similarity threshold for suggesting alternatives (0-100 percentage).
const MIN_SIMILARITY_PERCENT: u32 = 40;

// Include project detection logic (Levenshtein distance, similarity, fuzzy matching)
include!("init/project_detection.rs");

// Include configuration generation logic (handlers for creating config files and PROMPT.md)
include!("init/config_generation.rs");

/// Prompt the user to confirm overwriting an existing PROMPT.md.
///
/// Returns `true` if the user confirms, `false` otherwise.
///
/// Requires stdin to be a terminal and at least one output stream (stdout/stderr)
/// to be a terminal so prompts are visible.
fn can_prompt_user() -> bool {
    prompt_output_target().is_some()
}

#[derive(Clone, Copy)]
enum PromptOutputTarget {
    Stdout,
    Stderr,
}

fn prompt_output_target() -> Option<PromptOutputTarget> {
    if !std::io::stdin().is_terminal() {
        return None;
    }

    if std::io::stdout().is_terminal() {
        return Some(PromptOutputTarget::Stdout);
    }
    if std::io::stderr().is_terminal() {
        return Some(PromptOutputTarget::Stderr);
    }

    None
}

fn with_prompt_writer<T>(
    target: PromptOutputTarget,
    f: impl FnOnce(&mut dyn std::io::Write) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    use std::io;

    match target {
        PromptOutputTarget::Stdout => {
            let mut out = io::stdout().lock();
            f(&mut out)
        }
        PromptOutputTarget::Stderr => {
            let mut err = io::stderr().lock();
            f(&mut err)
        }
    }
}

fn prompt_overwrite_confirmation(prompt_path: &Path, colors: Colors) -> anyhow::Result<bool> {
    use std::io;

    let Some(target) = prompt_output_target() else {
        return Ok(false);
    };

    with_prompt_writer(target, |w| {
        writeln!(
            w,
            "{}PROMPT.md already exists:{} {}",
            colors.yellow(),
            colors.reset(),
            prompt_path.display()
        )?;
        write!(w, "Do you want to overwrite it? [y/N]: ")?;
        w.flush()?;
        Ok(())
    })?;

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(0) => return Ok(false),
        Ok(_) => {}
        Err(_) => return Ok(false),
    }

    let response = input.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}

/// Print a short list of common Work Guides.
///
/// Shows the most commonly used Work Guides with a note to use --list-work-guides for more.
fn print_common_work_guides(colors: Colors) {
    println!("{}Common Work Guides:{}", colors.bold(), colors.reset());
    println!(
        "  {}quick{}           Quick/small changes (typos, minor fixes)",
        colors.cyan(),
        colors.reset()
    );
    println!(
        "  {}bug-fix{}         Bug fix with investigation guidance",
        colors.cyan(),
        colors.reset()
    );
    println!(
        "  {}feature-spec{}    Comprehensive product specification",
        colors.cyan(),
        colors.reset()
    );
    println!(
        "  {}refactor{}        Code refactoring with behavior preservation",
        colors.cyan(),
        colors.reset()
    );
    println!();
    println!(
        "Use {}--list-work-guides{} for the complete list of Work Guides.",
        colors.cyan(),
        colors.reset()
    );
    println!();
}

/// Print a template category section.
///
/// Helper function to reduce the length of `handle_list_work_guides`.
fn print_template_category(category_name: &str, templates: &[(&str, &str)], colors: Colors) {
    println!("{}{}:{}", colors.bold(), category_name, colors.reset());
    for (name, description) in templates {
        println!(
            "  {}{}{}  {}",
            colors.cyan(),
            name,
            colors.reset(),
            description
        );
    }
    println!();
}

/// Handle the `--list-work-guides` (or `--list-templates`) flag.
///
/// Lists all available PROMPT.md Work Guides with descriptions, organized by category.
///
/// # Arguments
///
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `true` if the flag was handled (program should exit after).
pub fn handle_list_work_guides(colors: Colors) -> bool {
    println!("PROMPT.md Work Guides (use: ralph --init <work-guide>)");
    println!();

    // Common templates (most frequently used)
    print_template_category(
        "Common Templates",
        &[
            ("quick", "Quick/small changes (typos, minor fixes)"),
            ("bug-fix", "Bug fix with investigation guidance"),
            ("feature-spec", "Comprehensive product specification"),
            ("refactor", "Code refactoring with behavior preservation"),
        ],
        colors,
    );

    // Testing and documentation
    print_template_category(
        "Testing & Documentation",
        &[
            ("test", "Test writing with edge case considerations"),
            ("docs", "Documentation update with completeness checklist"),
            ("code-review", "Structured code review for pull requests"),
        ],
        colors,
    );

    // Specialized development
    print_template_category(
        "Specialized Development",
        &[
            ("cli-tool", "CLI tool with argument parsing and completion"),
            ("web-api", "REST/HTTP API with error handling"),
            (
                "ui-component",
                "UI component with accessibility and responsive design",
            ),
            ("onboarding", "Learn a new codebase efficiently"),
        ],
        colors,
    );

    // Advanced/Infrastructure
    print_template_category(
        "Advanced & Infrastructure",
        &[
            (
                "performance-optimization",
                "Performance optimization with benchmarking",
            ),
            (
                "security-audit",
                "Security audit with OWASP Top 10 coverage",
            ),
            (
                "api-integration",
                "API integration with retry logic and resilience",
            ),
            (
                "database-migration",
                "Database migration with zero-downtime strategies",
            ),
            (
                "dependency-update",
                "Dependency update with breaking change handling",
            ),
            ("data-pipeline", "Data pipeline with ETL and monitoring"),
        ],
        colors,
    );

    // Maintenance
    print_template_category(
        "Maintenance & Operations",
        &[
            (
                "debug-triage",
                "Systematic issue investigation and diagnosis",
            ),
            (
                "tech-debt",
                "Technical debt refactoring with prioritization",
            ),
            (
                "release",
                "Release preparation with versioning and changelog",
            ),
        ],
        colors,
    );

    println!("Usage: ralph --init <work-guide>");
    println!();
    println!("Example:");
    println!("  ralph --init bug-fix              # Create bug fix Work Guide");
    println!("  ralph --init feature-spec         # Create feature spec Work Guide");
    println!("  ralph --init quick                # Create quick change Work Guide");
    println!();
    println!("{}Tip:{}", colors.yellow(), colors.reset());
    println!("  Use --init without a value to auto-detect what you need.");
    println!("  Use --force-overwrite to overwrite an existing PROMPT.md.");
    println!("  Run ralph --extended-help to learn about Work Guides vs Agent Prompts.");

    true
}

/// Handle the smart `--init` flag with a custom path resolver.
///
/// This function intelligently determines what the user wants to initialize:
/// - If a value is provided and matches a known template name -> create PROMPT.md
/// - If config doesn't exist and no template specified -> create config
/// - If config exists but PROMPT.md doesn't -> prompt to create PROMPT.md
/// - If both exist -> show helpful message about what's already set up
///
/// # Arguments
///
/// * `template_arg` - Optional template name from `--init=TEMPLATE`
/// * `force` - If true, overwrite existing PROMPT.md without prompting
/// * `colors` - Terminal color configuration for output
/// * `resolver` - Path resolver for determining config file locations
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or `Ok(false)` if not handled, or an error if initialization failed.
pub fn handle_smart_init_with<R: ConfigEnvironment>(
    template_arg: Option<&str>,
    force: bool,
    colors: Colors,
    env: &R,
) -> anyhow::Result<bool> {
    let config_path = env
        .unified_config_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory (no home directory)"))?;
    let prompt_path = env.prompt_path();
    handle_smart_init_at_paths_with_env(
        template_arg,
        force,
        colors,
        &config_path,
        &prompt_path,
        env,
    )
}

/// Handle the smart `--init` flag using the default path resolver.
///
/// This is a convenience wrapper that uses [`RealConfigEnvironment`] internally.
///
/// # Arguments
///
/// * `template_arg` - Optional template name from `--init=TEMPLATE`
/// * `force` - If true, overwrite existing PROMPT.md without prompting
/// * `colors` - Terminal color configuration for output
///
/// # Returns
///
/// Returns `Ok(true)` if the flag was handled (program should exit after),
/// or `Ok(false)` if not handled, or an error if initialization failed.
pub fn handle_smart_init(
    template_arg: Option<&str>,
    force: bool,
    colors: Colors,
) -> anyhow::Result<bool> {
    handle_smart_init_with(template_arg, force, colors, &RealConfigEnvironment)
}

fn handle_smart_init_at_paths_with_env<R: ConfigEnvironment>(
    template_arg: Option<&str>,
    force: bool,
    colors: Colors,
    config_path: &std::path::Path,
    prompt_path: &Path,
    env: &R,
) -> anyhow::Result<bool> {
    let config_exists = env.file_exists(config_path);
    let prompt_exists = env.file_exists(prompt_path);

    // If a template name is provided (non-empty), treat it as --init <template>
    if let Some(template_name) = template_arg {
        if !template_name.is_empty() {
            return handle_init_template_arg_at_path_with_env(
                template_name,
                prompt_path,
                force,
                colors,
                env,
            );
        }
        // Empty string means --init was used without a value, fall through to smart inference
    }

    // No template provided - use smart inference based on current state
    handle_init_state_inference_with_env(
        config_path,
        prompt_path,
        config_exists,
        prompt_exists,
        force,
        colors,
        env,
    )
}

/// Handle --init when both config and PROMPT.md exist.
fn handle_init_both_exist(
    config_path: &std::path::Path,
    prompt_path: &Path,
    force: bool,
    colors: Colors,
) -> bool {
    // If force is set, show that they can use --force-overwrite to overwrite
    if force {
        println!(
            "{}Note:{} --force-overwrite has no effect when not specifying a Work Guide.",
            colors.yellow(),
            colors.reset()
        );
        println!("Use: ralph --init <work-guide> --force-overwrite  to overwrite PROMPT.md");
        println!();
    }

    println!("{}Setup complete!{}", colors.green(), colors.reset());
    println!();
    println!(
        "  Config: {}{}{}",
        colors.dim(),
        config_path.display(),
        colors.reset()
    );
    println!(
        "  PROMPT: {}{}{}",
        colors.dim(),
        prompt_path.display(),
        colors.reset()
    );
    println!();
    println!("You're ready to run Ralph:");
    println!("  ralph \"your commit message\"");
    println!();
    println!("Other commands:");
    println!("  ralph --list-work-guides   # Show all Work Guides");
    println!("  ralph --init <work-guide> --force-overwrite  # Overwrite PROMPT.md");
    true
}

/// Prompt the user to select a template interactively.
///
/// Returns `Some(template_name)` if the user selected a template,
/// or `None` if the user declined or entered invalid input.
fn prompt_for_template(colors: Colors) -> Option<String> {
    use std::io;

    let target = prompt_output_target()?;
    if with_prompt_writer(target, |w| {
        let _ = writeln!(
            w,
            "PROMPT.md contains your task specification for the AI agents."
        );
        let _ = write!(w, "Would you like to create one from a Work Guide? [Y/n]: ");
        w.flush()?;
        Ok(())
    })
    .is_err()
    {
        return None;
    };

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(0) | Err(_) => return None,
        Ok(_) => {}
    }

    let response = input.trim().to_lowercase();
    if response == "n" || response == "no" || response == "skip" {
        return None;
    }

    // Show available templates
    let templates: Vec<(&str, &str)> = list_templates();
    if with_prompt_writer(target, |w| {
        let _ = writeln!(w);
        let _ = writeln!(w, "Available Work Guides:");

        for (i, (name, description)) in templates.iter().enumerate() {
            let _ = writeln!(
                w,
                "  {}{}{}  {}{}{}",
                colors.cyan(),
                name,
                colors.reset(),
                colors.dim(),
                description,
                colors.reset()
            );
            if (i + 1) % 5 == 0 {
                let _ = writeln!(w); // Group templates in sets of 5 for readability
            }
        }

        let _ = writeln!(w);
        let _ = writeln!(w, "Common choices:");
        let _ = writeln!(
            w,
            "  {}quick{}           - Quick/small changes (typos, minor fixes)",
            colors.cyan(),
            colors.reset()
        );
        let _ = writeln!(
            w,
            "  {}bug-fix{}         - Bug fix with investigation guidance",
            colors.cyan(),
            colors.reset()
        );
        let _ = writeln!(
            w,
            "  {}feature-spec{}    - Product specification",
            colors.cyan(),
            colors.reset()
        );
        let _ = writeln!(w);
        let _ = write!(w, "Enter Work Guide name (or press Enter to use 'quick'): ");
        w.flush()?;
        Ok(())
    })
    .is_err()
    {
        return None;
    };

    let mut template_input = String::new();
    match io::stdin().read_line(&mut template_input) {
        Ok(0) | Err(_) => return None,
        Ok(_) => {}
    }

    let template_name = template_input.trim();
    if template_name.is_empty() {
        // Default to 'quick' template
        return Some("quick".to_string());
    }

    // Validate the template exists
    if get_template(template_name).is_some() {
        Some(template_name.to_string())
    } else {
        let _ = with_prompt_writer(target, |w| {
            writeln!(
                w,
                "{}Unknown Work Guide: '{}'{}",
                colors.red(),
                template_name,
                colors.reset()
            )?;
            writeln!(
                w,
                "Run 'ralph --list-work-guides' to see all available Work Guides."
            )?;
            Ok(())
        });
        None
    }
}

/// Handle the `--extended-help` / `--man` flag.
///
/// Displays comprehensive help including shell completion, all presets,
/// troubleshooting information, and the difference between Work Guides and Agent Prompts.
pub fn handle_extended_help() {
    println!(
        r#"RALPH EXTENDED HELP
===============================================================================

Ralph is a PROMPT-driven multi-agent orchestrator for git repos. It runs a
developer agent for code implementation, then a reviewer agent for quality
assurance and fixes (default), automatically staging and committing the final result.

===============================================================================
GETTING STARTED
===============================================================================

  1. Initialize config:
       ralph --init                      # Smart init (infers what you need)

  2. Create a PROMPT.md from a Work Guide:
       ralph --init feature-spec         # Or: bug-fix, refactor, quick, etc.

  3. Edit PROMPT.md with your task details

  4. Run Ralph:
       ralph "fix: my bug description"   # Commit message for the final commit

===============================================================================
WORK GUIDES VS AGENT PROMPTS
===============================================================================

  Ralph has two types of templates - understanding the difference is key:

  1. WORK GUIDES (for PROMPT.md - YOUR task descriptions)
     -------------------------------------------------
     These are templates for describing YOUR work to the AI.
     You fill them in with your specific task requirements.

     Examples: quick, bug-fix, feature-spec, refactor, test, docs

     Commands:
       ralph --init <work-guide>      Create PROMPT.md from a Work Guide
       ralph --list-work-guides       Show all available Work Guides

  2. AGENT PROMPTS (backend AI behavior configuration)
     -------------------------------------------------
     These configure HOW the AI agents behave (internal system prompts).
     You probably don't need to touch these unless customizing agent behavior.

     Commands:
       ralph --init-system-prompts    Create default Agent Prompts
       ralph --list                   Show Agent Prompt templates
       ralph --show <name>            Show a specific Agent Prompt

===============================================================================
PRESET MODES
===============================================================================

  Pick how thorough the AI should be:

    -Q  Quick:      1 dev iteration  + 1 review   (typos, small fixes)
    -U  Rapid:      2 dev iterations + 1 review   (minor changes)
    -S  Standard:   5 dev iterations + 2 reviews  (default for most tasks)
    -T  Thorough:  10 dev iterations + 5 reviews  (complex features)
    -L  Long:      15 dev iterations + 10 reviews (most thorough)

  Custom iterations:
    ralph -D 3 -R 2 "feat: feature"   # 3 dev iterations, 2 review cycles
    ralph -D 10 -R 0 "feat: no review"  # Skip review phase entirely

===============================================================================
COMMON OPTIONS
===============================================================================

  Iterations:
    -D N, --developer-iters N   Set developer iterations
    -R N, --reviewer-reviews N  Set review cycles (0 = skip review)

  Agents:
    -a AGENT, --developer-agent AGENT   Pick developer agent
    -r AGENT, --reviewer-agent AGENT    Pick reviewer agent

  Verbosity:
    -q, --quiet          Quiet mode (minimal output)
    -f, --full           Full output (no truncation)
    -v N, --verbosity N  Set verbosity (0-4)

  Other:
    -d, --diagnose       Show system info and agent status

===============================================================================
ADVANCED OPTIONS
===============================================================================

  These options are hidden from the main --help to reduce clutter.

  Initialization:
    --force-overwrite            Overwrite PROMPT.md without prompting
    -i, --interactive            Prompt for PROMPT.md if missing

  Git Control:
    --with-rebase                Enable automatic rebase to main branch (disabled by default)
    --rebase-only                Only rebase, then exit (no pipeline)
    --git-user-name <name>       Override git user name for commits
    --git-user-email <email>     Override git user email for commits

  Recovery:
    --resume                     Resume from last checkpoint
    --dry-run                    Validate setup without running agents

  Agent Prompt Management:
    --init-system-prompts        Create default Agent Prompt templates
    --list                       List all Agent Prompt templates
    --show <name>                Show Agent Prompt content
    --validate                   Validate Agent Prompt templates
    --variables <name>           Extract variables from template
    --render <name>              Test render a template

  Debugging:
    --show-streaming-metrics     Show JSON streaming quality metrics
    -c PATH, --config PATH       Use specific config file

===============================================================================
SHELL COMPLETION
===============================================================================

  Enable tab-completion for faster command entry:

    Bash:
      ralph --generate-completion=bash > ~/.local/share/bash-completion/completions/ralph

    Zsh:
      ralph --generate-completion=zsh > ~/.zsh/completion/_ralph

    Fish:
      ralph --generate-completion=fish > ~/.config/fish/completions/ralph.fish

  Then restart your shell or source the file.

===============================================================================
TROUBLESHOOTING
===============================================================================

  Common issues:

    "PROMPT.md not found"
      -> Run: ralph --init <work-guide>  (e.g., ralph --init bug-fix)

    "No agents available"
      -> Run: ralph -d  (diagnose) to check agent status
      -> Ensure at least one agent is installed (claude, codex, opencode)

    "Config file not found"
      -> Run: ralph --init  to create ~/.config/ralph-workflow.toml

    Resume after interruption:
      -> Run: ralph --resume  to continue from last checkpoint

    Validate setup without running:
      -> Run: ralph --dry-run

===============================================================================
EXAMPLES
===============================================================================

    ralph "fix: typo"                 Run with default settings
    ralph -Q "fix: small bug"         Quick mode for tiny fixes
    ralph -U "feat: add button"       Rapid mode for minor features
    ralph -a claude "fix: bug"        Use specific agent
    ralph --list-work-guides          See all Work Guides
    ralph --init bug-fix              Create PROMPT.md from a Work Guide
    ralph --init bug-fix --force-overwrite  Overwrite existing PROMPT.md

===============================================================================
"#
    );
}

#[cfg(test)]
mod tests {
    include!("init/tests.rs");
}
