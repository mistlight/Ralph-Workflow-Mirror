use super::MainEffectHandler;
use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{ConflictStrategy, PipelineEvent, RebasePhase};
use anyhow::Result;

impl MainEffectHandler {
    pub(super) fn run_rebase(
        &self,
        ctx: &mut PhaseContext<'_>,
        phase: RebasePhase,
        target_branch: String,
    ) -> Result<EffectResult> {
        use crate::git_helpers::{get_conflicted_files, rebase_onto};

        if matches!(phase, RebasePhase::Initial) {
            let run_context = ctx.run_context.clone();
            let outcome = crate::app::rebase::run_initial_rebase(ctx, &run_context, ctx.executor)?;

            let event = match outcome {
                crate::app::rebase::InitialRebaseOutcome::Succeeded { new_head } => {
                    PipelineEvent::rebase_succeeded(phase, new_head)
                }
                crate::app::rebase::InitialRebaseOutcome::Skipped { reason } => {
                    PipelineEvent::rebase_skipped(phase, reason)
                }
            };

            return Ok(EffectResult::event(event));
        }

        match rebase_onto(&target_branch, ctx.executor) {
            Ok(_) => {
                let conflicted_files = get_conflicted_files().unwrap_or_default();

                if conflicted_files.is_empty() {
                    let new_head = git2::Repository::open(ctx.repo_root).map_or_else(
                        |_| "unknown".to_string(),
                        |repo| repo
                            .head()
                            .ok()
                            .and_then(|head| head.peel_to_commit().ok())
                            .map_or_else(
                                || "unknown".to_string(),
                                |commit| commit.id().to_string(),
                            )
                    );

                    Ok(EffectResult::event(PipelineEvent::rebase_succeeded(
                        phase, new_head,
                    )))
                } else {
                    let files = conflicted_files
                        .into_iter()
                        .map(std::convert::Into::into)
                        .collect();
                    Ok(EffectResult::event(
                        PipelineEvent::rebase_conflict_detected(files),
                    ))
                }
            }
            Err(e) => Ok(EffectResult::event(PipelineEvent::rebase_failed(
                phase,
                e.to_string(),
            ))),
        }
    }

    pub(super) fn resolve_rebase_conflicts(
        &self,
        ctx: &PhaseContext<'_>,
        strategy: ConflictStrategy,
    ) -> Result<EffectResult> {
        use crate::git_helpers::{abort_rebase, continue_rebase, get_conflicted_files};

        match strategy {
            ConflictStrategy::Continue => match continue_rebase(ctx.executor) {
                Ok(()) => {
                    let files = get_conflicted_files()
                        .unwrap_or_default()
                        .into_iter()
                        .map(std::convert::Into::into)
                        .collect();

                    Ok(EffectResult::event(
                        PipelineEvent::rebase_conflict_resolved(files),
                    ))
                }
                Err(e) => Ok(EffectResult::event(PipelineEvent::rebase_failed(
                    RebasePhase::PostReview,
                    e.to_string(),
                ))),
            },
            ConflictStrategy::Abort => match abort_rebase(ctx.executor) {
                Ok(()) => {
                    let restored_to = match git2::Repository::open(ctx.repo_root) {
                        Ok(repo) => repo
                            .head()
                            .ok()
                            .and_then(|head| head.peel_to_commit().ok())
                            .map_or_else(|| "HEAD".to_string(), |commit| commit.id().to_string()),
                        Err(_) => "HEAD".to_string(),
                    };

                    Ok(EffectResult::event(PipelineEvent::rebase_aborted(
                        RebasePhase::PostReview,
                        restored_to,
                    )))
                }
                Err(e) => Ok(EffectResult::event(PipelineEvent::rebase_failed(
                    RebasePhase::PostReview,
                    e.to_string(),
                ))),
            },
            ConflictStrategy::Skip => Ok(EffectResult::event(
                PipelineEvent::rebase_conflict_resolved(Vec::new()),
            )),
        }
    }
}
