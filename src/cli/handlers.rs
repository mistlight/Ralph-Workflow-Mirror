//! CLI command handlers.
//!
//! Contains handler functions for CLI commands like --list-agents,
//! --diagnose, and --dry-run.

use crate::agents::{global_agents_config_path, AgentRegistry, AgentRole};
use crate::colors::Colors;
use crate::config::Config;
use crate::guidelines::{CheckSeverity, ReviewGuidelines};
use crate::language_detector;
use crate::utils::{load_checkpoint, Logger};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Handle --list-agents command.
pub fn handle_list_agents(registry: &AgentRegistry) {
    let mut items = registry.list();
    items.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (name, cfg) in items {
        println!(
            "{}\tcmd={}\tparser={}\tcan_commit={}",
            name, cfg.cmd, cfg.json_parser, cfg.can_commit
        );
    }
}

/// Handle --list-available-agents command.
pub fn handle_list_available_agents(registry: &AgentRegistry) {
    let mut items = registry.list_available();
    items.sort();
    for name in items {
        println!("{}", name);
    }
}

/// Handle --diagnose command.
///
/// Outputs comprehensive diagnostic information including:
/// - System information (OS, architecture, working directory)
/// - Git status and configuration
/// - Agent configuration and availability
/// - PROMPT.md validation
/// - Checkpoint status
/// - Project stack detection
pub fn handle_diagnose(
    colors: &Colors,
    config: &Config,
    registry: &AgentRegistry,
    agents_config_path: &Path,
    config_sources: &[crate::agents::ConfigSource],
) {
    println!(
        "{}=== Ralph Diagnostic Report ==={}",
        colors.bold(),
        colors.reset()
    );
    println!();

    // System Information
    println!("{}System:{}", colors.bold(), colors.reset());
    println!("  OS: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    if let Ok(cwd) = std::env::current_dir() {
        println!("  Working directory: {}", cwd.display());
    }
    if let Ok(shell) = std::env::var("SHELL") {
        println!("  Shell: {}", shell);
    }
    println!();

    // Git Information
    println!("{}Git:{}", colors.bold(), colors.reset());
    if let Ok(output) = Command::new("git").args(["--version"]).output() {
        if let Ok(version) = std::str::from_utf8(&output.stdout) {
            println!("  Version: {}", version.trim());
        }
    }
    let is_repo = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    println!("  In git repo: {}", if is_repo { "yes" } else { "no" });
    if is_repo {
        if let Ok(output) = Command::new("git")
            .args(["branch", "--show-current"])
            .output()
        {
            if let Ok(branch) = std::str::from_utf8(&output.stdout) {
                println!("  Current branch: {}", branch.trim());
            }
        }
        // Check for uncommitted changes
        if let Ok(output) = Command::new("git").args(["status", "--porcelain"]).output() {
            if let Ok(status) = std::str::from_utf8(&output.stdout) {
                let changes = status.lines().count();
                println!("  Uncommitted changes: {}", changes);
            }
        }
    }
    println!();

    // Configuration
    println!("{}Configuration:{}", colors.bold(), colors.reset());
    println!("  Config file: {}", agents_config_path.display());
    println!("  Config exists: {}", agents_config_path.exists());
    println!(
        "  Review depth: {:?} ({})",
        config.review_depth,
        config.review_depth.description()
    );
    if let Some(global_path) = global_agents_config_path() {
        println!("  Global config: {}", global_path.display());
        println!("  Global exists: {}", global_path.exists());
    }
    if !config_sources.is_empty() {
        println!("  Loaded sources:");
        for src in config_sources {
            println!(
                "    - {} ({} agents)",
                src.path.display(),
                src.agents_loaded
            );
        }
    }
    println!();

    // Agent Chain Configuration
    println!("{}Agent Chain:{}", colors.bold(), colors.reset());
    let fallback = registry.fallback_config();
    let dev_chain = fallback.get_fallbacks(AgentRole::Developer);
    let rev_chain = fallback.get_fallbacks(AgentRole::Reviewer);
    println!("  Developer chain: {:?}", dev_chain);
    println!("  Reviewer chain: {:?}", rev_chain);
    println!("  Max retries: {}", fallback.max_retries);
    println!("  Retry delay: {}ms", fallback.retry_delay_ms);
    println!();

    // Agent Availability
    println!("{}Agent Availability:{}", colors.bold(), colors.reset());
    let all_agents = registry.list();
    let mut sorted_agents: Vec<_> = all_agents.into_iter().collect();
    sorted_agents.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (name, cfg) in sorted_agents {
        let available = registry.is_agent_available(name);
        let status_color = if available {
            colors.green()
        } else {
            colors.red()
        };
        let status_icon = if available { "✓" } else { "✗" };
        println!(
            "  {}{}{} {} (parser: {}, cmd: {})",
            status_color,
            status_icon,
            colors.reset(),
            name,
            cfg.json_parser,
            cfg.cmd.split_whitespace().next().unwrap_or(&cfg.cmd)
        );
    }
    println!();

    // PROMPT.md Status
    println!("{}PROMPT.md:{}", colors.bold(), colors.reset());
    let prompt_path = Path::new("PROMPT.md");
    if prompt_path.exists() {
        if let Ok(content) = fs::read_to_string(prompt_path) {
            println!("  Exists: yes");
            println!("  Size: {} bytes", content.len());
            println!("  Lines: {}", content.lines().count());
            let has_goal = content.contains("## Goal") || content.contains("# Goal");
            let has_acceptance =
                content.contains("## Acceptance") || content.contains("Acceptance Criteria");
            println!(
                "  Has Goal section: {}",
                if has_goal { "yes" } else { "no" }
            );
            println!(
                "  Has Acceptance section: {}",
                if has_acceptance { "yes" } else { "no" }
            );
        }
    } else {
        println!("  Exists: no");
    }
    println!();

    // Checkpoint Status
    println!("{}Checkpoint:{}", colors.bold(), colors.reset());
    let checkpoint_path = Path::new(".agent/checkpoint.json");
    if checkpoint_path.exists() {
        println!("  Exists: yes");
        if let Ok(Some(cp)) = load_checkpoint() {
            println!("  Phase: {:?}", cp.phase);
            println!("  Developer agent: {}", cp.developer_agent);
            println!("  Reviewer agent: {}", cp.reviewer_agent);
            println!(
                "  Iterations: {}/{} dev, {}/{} review",
                cp.iteration, cp.total_iterations, cp.reviewer_pass, cp.total_reviewer_passes
            );
        }
    } else {
        println!("  Exists: no (no interrupted run to resume)");
    }
    println!();

    // Language Detection
    println!("{}Project Stack:{}", colors.bold(), colors.reset());
    if let Ok(cwd) = std::env::current_dir() {
        match language_detector::detect_stack(&cwd) {
            Ok(stack) => {
                println!("  Primary language: {}", stack.primary_language);
                if !stack.secondary_languages.is_empty() {
                    println!("  Secondary languages: {:?}", stack.secondary_languages);
                }
                if !stack.frameworks.is_empty() {
                    println!("  Frameworks: {:?}", stack.frameworks);
                }
                if let Some(pm) = &stack.package_manager {
                    println!("  Package manager: {}", pm);
                }
                if let Some(tf) = &stack.test_framework {
                    println!("  Test framework: {}", tf);
                }

                // Show language type indicators (useful for debugging language detection)
                let language_types: Vec<&str> = [
                    if stack.is_rust() { Some("Rust") } else { None },
                    if stack.is_python() {
                        Some("Python")
                    } else {
                        None
                    },
                    if stack.is_javascript_or_typescript() {
                        Some("JS/TS")
                    } else {
                        None
                    },
                    if stack.is_go() { Some("Go") } else { None },
                ]
                .into_iter()
                .flatten()
                .collect();
                if !language_types.is_empty() {
                    println!("  Language flags: {}", language_types.join(", "));
                }

                // Show review guidelines summary
                let guidelines = ReviewGuidelines::for_stack(&stack);
                println!("  Review checks: {} total", guidelines.total_checks());

                // Show severity breakdown from get_all_checks
                let all_checks = guidelines.get_all_checks();
                let critical_count = all_checks
                    .iter()
                    .filter(|c| matches!(c.severity, CheckSeverity::Critical))
                    .count();
                let high_count = all_checks
                    .iter()
                    .filter(|c| matches!(c.severity, CheckSeverity::High))
                    .count();
                if critical_count > 0 || high_count > 0 {
                    println!(
                        "  Check severities: {} critical, {} high",
                        critical_count, high_count
                    );
                }
            }
            Err(e) => {
                println!("  Detection failed: {}", e);
            }
        }
    }
    println!();

    // Recent errors (if log exists)
    let log_path = Path::new(".agent/logs/pipeline.log");
    if log_path.exists() {
        println!(
            "{}Recent Log Entries (last 10):{}",
            colors.bold(),
            colors.reset()
        );
        if let Ok(content) = fs::read_to_string(log_path) {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(10);
            for line in &lines[start..] {
                println!("  {}", line);
            }
        }
    }

    println!();
    println!(
        "{}Copy this output for bug reports: https://github.com/anthropics/ralph/issues{}",
        colors.dim(),
        colors.reset()
    );
}

/// Handle --dry-run command.
///
/// Validates the setup without running any agents:
/// - Checks PROMPT.md exists and has required sections
/// - Validates agent configuration
/// - Reports detected project stack
pub fn handle_dry_run(
    logger: &Logger,
    _colors: &Colors,
    config: &Config,
    developer_agent: &str,
    reviewer_agent: &str,
    repo_root: &Path,
) -> anyhow::Result<()> {
    use crate::language_detector::detect_stack_summary;
    use crate::utils::{checkpoint_exists, validate_prompt_md};

    logger.header("DRY RUN: Validation", |c| c.cyan());

    // Validate PROMPT.md using the utility function
    let validation = validate_prompt_md(config.strict_validation);

    // Report errors first
    for err in &validation.errors {
        logger.error(err);
    }

    // Report warnings
    for warn in &validation.warnings {
        logger.warn(&format!("{} (recommended)", warn));
    }

    // Bail if validation failed
    if !validation.is_valid() {
        anyhow::bail!("Dry run failed: PROMPT.md validation errors");
    }

    // Report successes
    if validation.has_goal {
        logger.success("PROMPT.md has Goal section");
    }
    if validation.has_acceptance {
        logger.success("PROMPT.md has acceptance checks section");
    }
    if validation.is_perfect() {
        logger.success("PROMPT.md validation passed with no warnings");
    }

    logger.success(&format!("Developer agent: {}", developer_agent));
    logger.success(&format!("Reviewer agent: {}", reviewer_agent));
    logger.success(&format!("Developer iterations: {}", config.developer_iters));
    logger.success(&format!("Reviewer passes: {}", config.reviewer_reviews));

    // Check for checkpoint
    if checkpoint_exists() {
        logger.info("Checkpoint found - can resume with --resume");
        if let Ok(Some(cp)) = load_checkpoint() {
            logger.info(&format!("  Phase: {}", cp.phase));
            logger.info(&format!("  Progress: {}", cp.description()));
            logger.info(&format!("  Saved at: {}", cp.timestamp));
        }
    }

    // Detect stack - use the convenience function for simple display
    logger.success(&format!(
        "Detected stack: {}",
        detect_stack_summary(repo_root)
    ));

    logger.success("Dry run validation complete");
    Ok(())
}
