// NOTE: split from reducer/effect/types.rs (EffectHandler trait).

use crate::phases::PhaseContext;
use anyhow::Result;

use super::effect_enum::Effect;
use super::effect_result::EffectResult;

/// Trait for executing effects.
///
/// Returns `EffectResult` containing both `PipelineEvent` (for state) and
/// `UIEvents` (for display). This allows mocking in tests.
pub trait EffectHandler<'ctx> {
    /// # Errors
    ///
    /// Returns an error if effect execution fails.
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext<'_>) -> Result<EffectResult>;
}
