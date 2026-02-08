//! Git commit execution and skipping.
//!
//! This module handles the final step of the commit phase:
//! - Creating git commits with generated messages
//! - Skipping commits when no changes are staged
//! - Handling commit hook failures
//!
//! ## Process
//!
//! 1. Run `git add -A` to stage all changes
//! 2. Run `git commit -m <message>` with generated commit message
//! 3. Emit success/failure events based on outcome
//!
//! ## Commit Skipping
//!
//! If `git commit` reports no changes to commit, emits `commit_skipped`
//! event instead of failure. This is not an error condition.

use super::super::MainEffectHandler;
use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::ErrorEvent;
use crate::reducer::event::PipelineEvent;
use crate::reducer::event::WorkspaceIoErrorKind;
use anyhow::Result;

impl MainEffectHandler {
    /// Create git commit with generated message.
    ///
    /// Stages all changes with `git add -A` and creates commit.
    ///
    /// # Events Emitted
    ///
    /// - `commit_created` - Commit successfully created with hash
    /// - `commit_skipped` - No changes to commit (not an error)
    /// - `commit_generation_failed` - Git commit command failed
    ///
    /// # Errors
    ///
    /// - `GitAddAllFailed` - Failed to stage changes
    pub(in crate::reducer::handler) fn create_commit(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        message: String,
    ) -> Result<EffectResult> {
        use crate::git_helpers::{git_add_all_in_repo, git_commit_in_repo};

        git_add_all_in_repo(ctx.repo_root).map_err(|err| ErrorEvent::GitAddAllFailed {
            kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
        })?;

        match git_commit_in_repo(ctx.repo_root, &message, None, None, Some(ctx.executor)) {
            Ok(Some(hash)) => Ok(EffectResult::event(PipelineEvent::commit_created(
                hash.to_string(),
                message,
            ))),
            Ok(None) => Ok(EffectResult::event(PipelineEvent::commit_skipped(
                "No changes to commit".to_string(),
            ))),
            Err(e) => Ok(EffectResult::event(
                PipelineEvent::commit_generation_failed(e.to_string()),
            )),
        }
    }

    /// Skip commit with a reason.
    ///
    /// Used when the orchestrator determines a commit should be skipped
    /// (e.g., empty diff, user-requested skip).
    ///
    /// # Events Emitted
    ///
    /// - `commit_skipped` - Commit skipped with reason
    pub(in crate::reducer::handler) fn skip_commit(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
        reason: String,
    ) -> Result<EffectResult> {
        Ok(EffectResult::event(PipelineEvent::commit_skipped(reason)))
    }
}
