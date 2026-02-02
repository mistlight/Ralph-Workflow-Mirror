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
    #[arg(long, help = "Validate all Agent Prompt templates for syntax errors", hide = true)]
    pub validate: bool,

    /// Show template content and metadata
    #[arg(long, value_name = "NAME", help = "Show Agent Prompt template content and metadata", hide = true)]
    pub show: Option<String>,

    /// List all prompt templates with their variables
    #[arg(
        long,
        help = "List all Agent Prompt templates with their variables",
        hide = true
    )]
    pub list: bool,

    /// List all templates including deprecated ones
    #[arg(long, help = "List all Agent Prompt templates including deprecated ones")]
    pub list_all: bool,

    /// Extract variables from a template
    #[arg(long, value_name = "NAME", help = "Extract variables from an Agent Prompt template", hide = true)]
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
