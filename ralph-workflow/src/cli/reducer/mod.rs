//! CLI argument reducer module.
//!
//! This module implements a reducer-based architecture for processing CLI arguments,
//! following the same patterns as the pipeline reducer in `crate::reducer`.
//!
//! # Architecture
//!
//! ```text
//! Args (clap) → args_to_events() → [CliEvent] → reduce() → CliState → apply_to_config() → Config
//! ```
//!
//! ## Benefits
//!
//! - **Testable**: Pure reducer function is easy to unit test
//! - **Maintainable**: Adding new CLI args = add event + reducer case
//! - **Consistent**: Matches existing pipeline reducer architecture
//! - **Traceable**: Event sequence can be logged/debugged
//!
//! # Example
//!
//! ```ignore
//! use crate::cli::reducer::{args_to_events, reduce, CliState, apply_cli_state_to_config};
//!
//! let events = args_to_events(&args);
//! let mut state = CliState::initial();
//! for event in events {
//!     state = reduce(state, event);
//! }
//! apply_cli_state_to_config(&state, &mut config);
//! ```

pub mod apply;
pub mod event;
pub mod parser;
pub mod state;
pub mod state_reduction;

// Re-export key types for convenience
pub use apply::apply_cli_state_to_config;
pub use parser::args_to_events;
pub use state::CliState;
pub use state_reduction::reduce;

// Public API is exposed through presets::apply_args_to_config
// Modules are made public to allow imports from presets.rs
// Note: Only re-export items that are actually used to avoid unused-import suppressions.
