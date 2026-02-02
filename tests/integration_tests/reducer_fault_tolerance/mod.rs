//! Fault tolerance integration tests for reducer architecture.
//!
//! Tests verify that agent failures (including panics, segfaults, I/O errors)
//! never crash the pipeline and always trigger proper fallback behavior.

mod agent_crash_handling;
mod helpers;
mod model_fallback;
