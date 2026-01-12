//! Agent listing handlers.
//!
//! This module provides handlers for listing agents and their configurations.

use crate::agents::{is_ccs_ref, AgentRegistry};

/// Handle --list-agents command.
///
/// Lists all registered agents with their configuration details including:
/// - Agent name
/// - Command to invoke the agent
/// - JSON parser type
/// - Whether the agent can create commits (can_commit flag)
///
/// CCS aliases (ccs/...) are displayed separately for clarity.
/// Output is sorted alphabetically by agent name within each section.
pub fn handle_list_agents(registry: &AgentRegistry) {
    let mut items = registry.list();
    items.sort_by(|(a, _), (b, _)| a.cmp(b));

    // Separate regular agents from CCS aliases
    let (ccs_aliases, regular_agents): (Vec<_>, Vec<_>) =
        items.into_iter().partition(|(name, _)| is_ccs_ref(name));

    // Print regular agents
    if !regular_agents.is_empty() {
        println!("Agents:");
        for (name, cfg) in regular_agents {
            println!(
                "  {}\tcmd={}\tparser={}\tcan_commit={}",
                name, cfg.cmd, cfg.json_parser, cfg.can_commit
            );
        }
    }

    // Print CCS aliases
    if !ccs_aliases.is_empty() {
        println!("\nCCS Aliases:");
        for (name, cfg) in ccs_aliases {
            println!("  {}\t→ \"{}\"", name, cfg.cmd);
        }
    }
}

/// Handle --list-available-agents command.
///
/// Lists only agents whose commands are available on the system PATH.
/// This helps users quickly identify which agents they can use without
/// additional setup.
///
/// CCS aliases are shown separately to distinguish them from regular agents.
/// Output is sorted alphabetically by agent name within each section.
pub fn handle_list_available_agents(registry: &AgentRegistry) {
    let mut items = registry.list_available();
    items.sort();

    // Separate regular agents from CCS aliases
    let (ccs_aliases, regular_agents): (Vec<_>, Vec<_>) =
        items.into_iter().partition(|name| is_ccs_ref(name));

    // Print regular agents
    if !regular_agents.is_empty() {
        println!("Available agents:");
        for name in regular_agents {
            println!("  {}", name);
        }
    }

    // Print CCS aliases
    if !ccs_aliases.is_empty() {
        println!("\nAvailable CCS aliases:");
        for name in ccs_aliases {
            println!("  {}", name);
        }
    }
}
