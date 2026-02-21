//! PROMPT.md permission lifecycle tracking.
//!
//! This state tracks the permission protection lifecycle for PROMPT.md:
//! - Locked at pipeline start (best-effort read-only)
//! - Restored on all graceful termination paths (success and failure)
//!
//! # Cleanup Guarantees
//!
//! Multiple safety layers ensure PROMPT.md is restored to writable state:
//!
//! 1. **Reducer-driven (normal path)**: Effect::RestorePromptPermissions during
//!    Finalizing phase or on interrupt (Ctrl+C or programmatic). Note: On interrupt,
//!    restoration is attempted unconditionally regardless of `restore_needed` state,
//!    since prior crashed runs may have left PROMPT.md read-only.
//!
//! 2. **RAII guard (panic/early return)**: `AgentPhaseGuard::drop()` restores
//!    permissions even when the reducer event loop doesn't complete.
//!
//! 3. **Startup cleanup (SIGKILL/crash)**: On next Ralph run, startup code
//!    restores PROMPT.md permissions from prior crashed runs.
//!
//! Note: SIGKILL and power loss cannot be intercepted; recovery happens on
//! next startup.

use serde::{Deserialize, Serialize};

/// Tracks PROMPT.md permission lifecycle for reducer-driven protection.
///
/// # State Transitions
///
/// 1. **Startup**: `locked=false, restore_needed=false, restored=false`
/// 2. **After LockPromptPermissions effect**: `locked=true, restore_needed=true`
/// 3. **After RestorePromptPermissions effect**: `locked=false, restore_needed=false, restored=true`
///
/// # Resume Safety
///
/// All fields are checkpointed. On resume:
/// - If locked but not restored, orchestration will derive RestorePromptPermissions
/// - If already restored, no further action needed
///
/// This state is serialized in `PipelineCheckpoint.prompt_permissions` to ensure
/// pending restores are honored after resume.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PromptPermissionsState {
    /// True if LockPromptPermissions effect has been attempted.
    pub locked: bool,

    /// True if restoration is needed (set when lock is attempted, even if it fails).
    pub restore_needed: bool,

    /// True if RestorePromptPermissions effect has completed.
    pub restored: bool,

    /// Warning message from last permission operation (for observability).
    pub last_warning: Option<String>,
}
