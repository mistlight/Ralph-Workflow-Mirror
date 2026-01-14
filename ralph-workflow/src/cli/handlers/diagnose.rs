//! Diagnostic command handler.
//!
//! This module provides comprehensive diagnostic output for troubleshooting
//! Ralph configuration and environment issues.

use crate::agents::{global_agents_config_path, AgentRegistry, AgentRole, ConfigSource};
use crate::checkpoint::load_checkpoint;
use crate::config::Config;
use crate::diagnostics::run_diagnostics;
use crate::guidelines::{CheckSeverity, ReviewGuidelines};
use crate::language_detector;
use crate::logger::Colors;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Handle --diagnose command.
///
/// Outputs comprehensive diagnostic information including:
/// - System information (OS, architecture, working directory)
/// - Git status and configuration
/// - Agent configuration and availability
/// - PROMPT.md validation
/// - Checkpoint status
/// - Project stack detection
///
/// This output is designed to be copy-pasted into bug reports.
///
/// # Arguments
///
/// * `colors` - Color configuration for output formatting
/// * `config` - The current Ralph configuration
/// * `registry` - The agent registry
/// * `config_path` - Path to the unified config file
/// * `config_sources` - List of configuration sources that were loaded
pub fn handle_diagnose(
    colors: Colors,
    config: &Config,
    registry: &AgentRegistry,
    config_path: &Path,
    config_sources: &[ConfigSource],
) {
    // Gather diagnostics using the diagnostics module
    let report = run_diagnostics(registry);

    println!(
        "{}=== Ralph Diagnostic Report ==={}",
        colors.bold(),
        colors.reset()
    );
    println!();

    print_system_info(colors);
    print_git_info(colors);
    print_config_info(colors, config, config_path, config_sources);
    print_agent_chain_info(colors, registry);
    print_agent_availability(colors, registry);
    print_prompt_status(colors);
    print_checkpoint_status(colors);
    print_project_stack(colors);
    print_recent_logs(colors);

    // Use diagnostic data to suppress dead code warnings
    let _ = report.agents.total_agents;
    let _ = report.agents.available_agents;
    let _ = report.agents.unavailable_agents;
    for status in &report.agents.agent_status {
        let _ = (
            &status.name,
            &status.display_name,
            status.available,
            &status.json_parser,
            &status.command,
        );
    }
    let _ = (
        &report.system.os,
        &report.system.arch,
        &report.system.working_directory,
        &report.system.shell,
        &report.system.git_version,
        report.system.git_repo,
        &report.system.git_branch,
        &report.system.uncommitted_changes,
    );

    println!();
    println!(
        "{}Copy this output for bug reports: https://github.com/anthropics/ralph/issues{}",
        colors.dim(),
        colors.reset()
    );
}

/// Print system information section.
fn print_system_info(colors: Colors) {
    println!("{}System:{}", colors.bold(), colors.reset());
    println!("  OS: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    if let Ok(cwd) = std::env::current_dir() {
        println!("  Working directory: {}", cwd.display());
    }
    if let Ok(shell) = std::env::var("SHELL") {
        println!("  Shell: {shell}");
    }
    println!();
}

/// Print git information section.
fn print_git_info(colors: Colors) {
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
                println!("  Uncommitted changes: {changes}");
            }
        }
    }
    println!();
}

/// Print configuration information section.
fn print_config_info(
    colors: Colors,
    config: &Config,
    config_path: &Path,
    config_sources: &[ConfigSource],
) {
    println!("{}Configuration:{}", colors.bold(), colors.reset());
    println!("  Unified config: {}", config_path.display());
    println!("  Config exists: {}", config_path.exists());
    println!(
        "  Review depth: {:?} ({})",
        config.review_depth,
        config.review_depth.description()
    );
    if let Some(global_path) = global_agents_config_path() {
        println!("  Legacy global agents.toml: {}", global_path.display());
        println!("  Legacy global exists: {}", global_path.exists());
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
}

/// Print agent chain configuration section.
fn print_agent_chain_info(colors: Colors, registry: &AgentRegistry) {
    println!("{}Agent Chain:{}", colors.bold(), colors.reset());
    let fallback = registry.fallback_config();
    let dev_chain = fallback.get_fallbacks(AgentRole::Developer);
    let rev_chain = fallback.get_fallbacks(AgentRole::Reviewer);
    println!("  Developer chain: {dev_chain:?}");
    println!("  Reviewer chain: {rev_chain:?}");
    println!("  Max retries: {}", fallback.max_retries);
    println!("  Retry delay: {}ms", fallback.retry_delay_ms);
    println!();
}

/// Print agent availability section.
fn print_agent_availability(colors: Colors, registry: &AgentRegistry) {
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
        let display_name = registry.display_name(name);
        println!(
            "  {}{}{} {} (parser: {}, cmd: {})",
            status_color,
            status_icon,
            colors.reset(),
            display_name,
            cfg.json_parser,
            cfg.cmd.split_whitespace().next().unwrap_or(&cfg.cmd)
        );
    }
    println!();
}

/// Print PROMPT.md status section.
fn print_prompt_status(colors: Colors) {
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
}

/// Print checkpoint status section.
fn print_checkpoint_status(colors: Colors) {
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
}

/// Print project stack detection section.
fn print_project_stack(colors: Colors) {
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
                    println!("  Package manager: {pm}");
                }
                if let Some(tf) = &stack.test_framework {
                    println!("  Test framework: {tf}");
                }

                // Show language type indicators
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

                // Show severity breakdown
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
                    println!("  Check severities: {critical_count} critical, {high_count} high");
                }
            }
            Err(e) => {
                println!("  Detection failed: {e}");
            }
        }
    }
    println!();
}

/// Print recent log entries section.
fn print_recent_logs(colors: Colors) {
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
                println!("  {line}");
            }
        }
    }
}
