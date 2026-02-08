//! Core types and builder methods for MockEffectHandler.
//!
//! This module contains the `MockEffectHandler` struct definition, its builder
//! pattern methods for configuration, and inspection helpers for verifying
//! captured effects and UI events.

use super::*;

/// Mock implementation of EffectHandler for testing.
///
/// This handler captures all executed effects for later inspection while
/// returning appropriate mock PipelineEvents. It performs NO real side effects:
/// - No git operations
/// - No file I/O
/// - No agent execution
/// - No subprocess spawning
///
/// # Thread Safety
///
/// Uses `RefCell` for interior mutability, allowing effect capture even
/// when handler is borrowed.
///
/// # Examples
///
/// ```ignore
/// let state = PipelineState::initial(1, 0);
/// let mut handler = MockEffectHandler::new(state)
///     .with_empty_diff(); // Configure mock behavior
///
/// // Execute effects and verify
/// let result = handler.execute(effect, &mut ctx)?;
/// assert!(handler.was_effect_executed(|e| matches!(e, Effect::CreateCommit { .. })));
/// ```
pub struct MockEffectHandler {
    /// The pipeline state (updated by reducer, not handler).
    pub state: PipelineState,
    /// All effects that have been executed, in order.
    pub(super) captured_effects: RefCell<Vec<Effect>>,
    /// All UI events that have been emitted, in order.
    pub(super) captured_ui_events: RefCell<Vec<UIEvent>>,
    /// When true, PrepareCommitPrompt returns CommitSkipped instead of proceeding.
    pub(super) simulate_empty_diff: bool,
}

impl MockEffectHandler {
    /// Create a new mock handler with the given initial state.
    ///
    /// # Arguments
    ///
    /// * `state` - Initial pipeline state to use
    ///
    /// # Returns
    ///
    /// A new `MockEffectHandler` with empty effect/event capture buffers
    pub fn new(state: PipelineState) -> Self {
        Self {
            state,
            captured_effects: RefCell::new(Vec::new()),
            captured_ui_events: RefCell::new(Vec::new()),
            simulate_empty_diff: false,
        }
    }

    /// Configure the mock to simulate empty diff scenario.
    ///
    /// When enabled, `CheckCommitDiff` effect returns a diff-empty event,
    /// causing the pipeline to skip commit message generation.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let handler = MockEffectHandler::new(state)
    ///     .with_empty_diff();
    /// ```
    pub fn with_empty_diff(mut self) -> Self {
        self.simulate_empty_diff = true;
        self
    }

    /// Get all captured effects in execution order.
    ///
    /// Returns a clone of the captured effects vector. Effects are captured
    /// in the order they were executed.
    pub fn captured_effects(&self) -> Vec<Effect> {
        self.captured_effects.borrow().clone()
    }

    /// Get all captured UI events in emission order.
    ///
    /// Returns a clone of the captured UI events vector. UI events are captured
    /// in the order they were emitted by effect handlers.
    pub fn captured_ui_events(&self) -> Vec<UIEvent> {
        self.captured_ui_events.borrow().clone()
    }

    /// Check if a specific effect type was captured.
    ///
    /// # Arguments
    ///
    /// * `predicate` - Function that returns `true` for matching effects
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert!(handler.was_effect_executed(|e|
    ///     matches!(e, Effect::CreateCommit { .. })
    /// ));
    /// ```
    pub fn was_effect_executed<F>(&self, predicate: F) -> bool
    where
        F: Fn(&Effect) -> bool,
    {
        self.captured_effects.borrow().iter().any(predicate)
    }

    /// Check if a specific UI event was emitted.
    ///
    /// # Arguments
    ///
    /// * `predicate` - Function that returns `true` for matching UI events
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert!(handler.was_ui_event_emitted(|e|
    ///     matches!(e, UIEvent::PhaseTransition { .. })
    /// ));
    /// ```
    pub fn was_ui_event_emitted<F>(&self, predicate: F) -> bool
    where
        F: Fn(&UIEvent) -> bool,
    {
        self.captured_ui_events.borrow().iter().any(predicate)
    }

    /// Clear all captured effects and UI events.
    ///
    /// Useful for resetting the mock between test cases when reusing
    /// the same handler instance.
    pub fn clear_captured(&self) {
        self.captured_effects.borrow_mut().clear();
        self.captured_ui_events.borrow_mut().clear();
    }

    /// Get the number of captured effects.
    pub fn effect_count(&self) -> usize {
        self.captured_effects.borrow().len()
    }

    /// Get the number of captured UI events.
    pub fn ui_event_count(&self) -> usize {
        self.captured_ui_events.borrow().len()
    }
}
