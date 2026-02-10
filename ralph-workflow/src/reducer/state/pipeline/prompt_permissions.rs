//! PROMPT.md permission lifecycle tracking.
//!
//! This state tracks the permission protection lifecycle for PROMPT.md:
//! - Locked at pipeline start (best-effort read-only)
//! - Restored on all graceful termination paths (success and failure)
//!
//! Note: Non-graceful termination (SIGKILL, power loss) may leave PROMPT.md
//! read-only. Manual recovery is required (e.g. `chmod +w PROMPT.md` on Unix
//! or clearing the read-only attribute on Windows).

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
