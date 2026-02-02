//! Configuration initialization handlers.
//!
//! This module handles the `--init` and `--init-global` flags for creating
//! default unified configuration files and PROMPT.md from templates.
//!
//! # Dependency Injection
//!
//! All init handlers accept a [`ConfigEnvironment`] for path resolution, enabling
//! tests to inject custom paths without relying on environment variables.
//!
//! For convenience, wrapper functions without the resolver parameter are provided
//! that use [`RealConfigEnvironment`] internally.

use crate::config::{ConfigEnvironment, RealConfigEnvironment};
use crate::logger::Colors;
use crate::templates::{get_template, list_templates, ALL_TEMPLATES};
use std::io::IsTerminal;
use std::path::Path;

/// Minimum similarity threshold for suggesting alternatives (0-100 percentage).
const MIN_SIMILARITY_PERCENT: u32 = 40;

// Include project detection logic (Levenshtein distance, similarity, fuzzy matching)
include!("init/project_detection.rs");

// Include configuration generation logic (handlers for creating config files and PROMPT.md)
include!("init/config_generation.rs");

// Include init prompting utilities for interactive flows.
include!("init/prompting.rs");

// Include Work Guide listing for `--list-work-guides`.
include!("init/work_guides.rs");

// Include the smart `--init` orchestration and helpers.
include!("init/smart_init.rs");

// Include `--extended-help` output.
include!("init/extended_help.rs");

#[cfg(test)]
mod tests {
    include!("init/tests.rs");
}
