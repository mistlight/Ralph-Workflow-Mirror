//! Review phase reducer functions.
//!
//! This module contains pure reducer functions for the review phase of the pipeline.
//! The review phase handles code review passes with the following workflow:
//!
//! 1. **Phase Started** - Initialize review state and agent chain
//! 2. **Pass Started** - Begin a new review pass (multiple passes supported)
//! 3. **Context Prepared** - Prepare diff and plan context for review
//! 4. **Prompt Prepared** - Generate review prompt with context
//! 5. **XML Cleaned** - Clean any existing XML before agent invocation
//! 6. **Agent Invoked** - Run reviewer agent with prepared prompt
//! 7. **XML Extracted** - Extract XML output from agent response
//! 8. **XML Validated** - Validate XML structure and extract issues
//! 9. **Markdown Written** - Write issues to ISSUES.md file
//! 10. **Snippets Extracted** - Extract code snippets for each issue
//! 11. **XML Archived** - Archive validated XML for reference
//! 12. **Pass Completed** - Complete pass (repeat or advance)
//!
//! ## State Transitions
//!
//! All reducer functions follow the pure reducer contract:
//! - Input: `PipelineState` + event data
//! - Output: new `PipelineState`
//! - No side effects (no I/O, no environment access, no logging)
//! - Deterministic (same input → same output)
//!
//! ## Module Organization
//!
//! - `state_transitions` - Phase and pass lifecycle transitions
//! - `pass_management` - Pass completion and phase transitions
//! - `validation_handling` - XSD validation failure and retry logic
//!
//! ## See Also
//!
//! - `docs/architecture/event-loop-and-reducers.md` - Reducer architecture
//! - `docs/architecture/effect-system.md` - Effect handler separation

// NOTE: This module was split from a single 527-line file into three focused modules
// to improve maintainability and clarity. All functions remain pure reducers.

mod pass_management;
mod state_transitions;
mod validation_handling;

// Re-export all reducer functions to maintain the existing API
pub(super) use pass_management::*;
pub(super) use state_transitions::*;
pub(super) use validation_handling::*;
