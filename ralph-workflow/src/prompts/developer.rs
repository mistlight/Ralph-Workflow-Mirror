//! Developer prompts.
//!
//! Prompts for developer agent actions including iteration and planning.

use std::collections::HashMap;
use std::path::Path;

use super::partials::get_shared_partials;
use super::template_context::TemplateContext;
use super::template_engine::Template;
#[cfg(any(test, feature = "test-utils"))]
use super::types::ContextLevel;
use crate::files::llm_output_extraction::file_based_extraction::resolve_absolute_path;
use crate::workspace::Workspace;

include!("developer/context_injection.rs");
include!("developer/system_prompt.rs");

#[cfg(test)]
mod tests {
    include!("developer/tests.rs");
}
