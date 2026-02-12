// Common imports and helpers for development_prompt tests
use crate::agents::{AgentRegistry, AgentRole};
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::executor::{MockProcessExecutor, ProcessExecutor};
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::event::{AgentEvent, DevelopmentEvent, PipelineEvent, PromptInputEvent};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{ContinuationState, PipelineState, PromptMode, SameAgentRetryReason};
use crate::workspace::MemoryWorkspace;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

mod context_inclusion;
mod continuation_prompt;
mod initial_prompt;
