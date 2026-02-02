//! Mock implementation of EffectHandler for testing.
//!
//! This module provides a mock handler that captures all executed effects
//! for later inspection, returning appropriate mock PipelineEvents without
//! performing any real side effects (no git calls, no file I/O, no agent execution).
//!
//! # Usage
//!
//! ```ignore
//! use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
//! use ralph_workflow::reducer::{Effect, EffectHandler, PipelineState};
//!
//! let state = PipelineState::initial(1, 0);
//! let mut handler = MockEffectHandler::new(state);
//!
//! // Execute an effect - no real side effects occur
//! let event = handler.execute(Effect::CreateCommit {
//!     message: "test".to_string()
//! }, &mut ctx)?;
//!
//! // Verify effect was captured
//! assert!(handler.captured_effects().iter().any(|e|
//!     matches!(e, Effect::CreateCommit { .. })
//! ));
//! ```

#![cfg(any(test, feature = "test-utils"))]

use super::effect::{Effect, EffectHandler, EffectResult};
use super::event::{PipelineEvent, PipelinePhase};
use super::state::PipelineState;
use super::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};
use crate::phases::PhaseContext;
use anyhow::Result;
use std::cell::RefCell;

include!("mock_effect_handler/mock_types.rs");
include!("mock_effect_handler/mock_handler.rs");
include!("mock_effect_handler/tests.rs");
