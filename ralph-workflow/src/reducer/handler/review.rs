use super::MainEffectHandler;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{AgentEvent, PipelineEvent};
use crate::reducer::state::PromptMode;
use crate::reducer::ui_event::{UIEvent, XmlCodeSnippet, XmlOutputContext, XmlOutputType};
use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

impl MainEffectHandler {
    const DIFF_BASELINE_PATH: &str = ".agent/DIFF.base";
}

include!("review/review_flow.rs");
include!("review/fix_flow.rs");
include!("review/snippets.rs");
