/// Agent listing flags.
#[derive(Parser, Debug, Default)]
pub struct AgentListFlags {
    /// List all configured agents and exit
    #[arg(long, help = "Show all agents from registry and config file", hide = true)]
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
