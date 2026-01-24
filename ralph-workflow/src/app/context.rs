//! Pipeline context types.
//!
//! This module defines the context structures used throughout the pipeline execution.

use crate::agents::AgentRegistry;
use crate::cli::Args;
use crate::config::Config;
use crate::logger::Colors;
use crate::logger::Logger;
use crate::prompts::template_context::TemplateContext;

/// Context for running the pipeline.
///
/// Groups together the various parameters needed to run the development/review/commit
/// pipeline, reducing function parameter count and improving maintainability.
pub struct PipelineContext {
    pub args: Args,
    pub config: Config,
    pub registry: AgentRegistry,
    pub developer_agent: String,
    pub reviewer_agent: String,
    pub developer_display: String,
    pub reviewer_display: String,
    pub repo_root: std::path::PathBuf,
    pub logger: Logger,
    pub colors: Colors,
    pub template_context: TemplateContext,
    pub executor: std::sync::Arc<dyn crate::executor::ProcessExecutor>,
}
