//! Agent listing handlers.
//!
//! This module provides handlers for listing agents and their configurations.

use crate::agents::AgentRegistry;

/// Handle --list-agents command.
///
/// Lists all registered agents with their configuration details including:
/// - Agent name
/// - Command to invoke the agent
/// - JSON parser type
/// - Whether the agent can create commits (can_commit flag)
///
/// Output is sorted alphabetically by agent name.
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
///
/// Lists only agents whose commands are available on the system PATH.
/// This helps users quickly identify which agents they can use without
/// additional setup.
///
/// Output is sorted alphabetically by agent name.
pub fn handle_list_available_agents(registry: &AgentRegistry) {
    let mut items = registry.list_available();
    items.sort();
    for name in items {
        println!("{}", name);
    }
}
