//! Cloud mode effect handlers.
//!
//! This module implements effect handlers for cloud-specific operations:
//! - Git authentication configuration
//! - Remote push operations
//! - Pull request creation
//! - Progress reporting
//!
//! All handlers follow the reducer architecture contract:
//! - Execute a single operation
//! - Emit events describing outcomes
//! - No retry logic (reducer decides)

use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{LifecycleEvent, PipelineEvent};
use anyhow::Result;

use super::MainEffectHandler;

impl MainEffectHandler {
    /// Configure git authentication for remote operations.
    ///
    /// This handler sets up git credentials based on the auth method:
    /// - SSH key: Configure GIT_SSH_COMMAND environment variable
    /// - Token: Set up git credential helper
    /// - Credential helper: Configure external helper
    pub(super) fn handle_configure_git_auth(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        auth_method: String,
    ) -> Result<EffectResult> {
        ctx.logger
            .info(&format!("Configuring git authentication: {}", auth_method));

        // Parse auth method string (format: "method:param")
        let parts: Vec<&str> = auth_method.splitn(2, ':').collect();
        let method = parts.first().unwrap_or(&"ssh-key");
        let param = parts.get(1).unwrap_or(&"default");

        match *method {
            "ssh-key" => {
                // Configure SSH key authentication
                if *param != "default" {
                    // Set GIT_SSH_COMMAND to use specific key
                    let ssh_command =
                        format!("ssh -i {} -o StrictHostKeyChecking=accept-new", param);
                    std::env::set_var("GIT_SSH_COMMAND", &ssh_command);
                    ctx.logger
                        .info(&format!("Set GIT_SSH_COMMAND to use key: {}", param));
                } else {
                    // Use default SSH key (SSH_AUTH_SOCK or ~/.ssh/id_rsa)
                    ctx.logger
                        .info("Using default SSH authentication (SSH_AUTH_SOCK or ~/.ssh/id_rsa)");
                }
            }
            "token" => {
                // Configure token-based authentication
                // The token itself is in the cloud config, not passed here for security
                ctx.logger.info(&format!(
                    "Configuring token authentication for user: {}",
                    param
                ));
                // Git credential helper will be configured via environment
                // This is handled by the token being present in the config
            }
            "credential-helper" => {
                // Configure external credential helper
                ctx.logger
                    .info(&format!("Using credential helper: {}", param));
                // Helper is configured via git config or environment
            }
            _ => {
                ctx.logger.warn(&format!(
                    "Unknown auth method: {}, falling back to default SSH",
                    method
                ));
            }
        }

        Ok(EffectResult::event(PipelineEvent::Lifecycle(
            LifecycleEvent::GitAuthConfigured,
        )))
    }

    /// Push commits to remote repository.
    ///
    /// Executes git push command and reports success/failure.
    pub(super) fn handle_push_to_remote(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        remote: String,
        branch: String,
        force: bool,
        commit_sha: String,
    ) -> Result<EffectResult> {
        ctx.logger.info(&format!(
            "Pushing commit {} to {}/{}{}",
            &commit_sha[..7.min(commit_sha.len())],
            remote,
            branch,
            if force { " (force)" } else { "" }
        ));

        // Build git push command
        let mut args = vec!["push", remote.as_str(), branch.as_str()];
        if force {
            args.push("--force");
        }

        // Execute push via executor
        let result = ctx.executor.execute("git", &args, &[], Some(ctx.repo_root));

        match result {
            Ok(output) if output.status.success() => {
                ctx.logger
                    .info(&format!("Successfully pushed to {}/{}", remote, branch));
                Ok(EffectResult::event(PipelineEvent::Lifecycle(
                    LifecycleEvent::PushCompleted {
                        remote,
                        branch,
                        commit_sha,
                    },
                )))
            }
            Ok(output) => {
                ctx.logger
                    .warn(&format!("Git push failed: {}", output.stderr));
                Ok(EffectResult::event(PipelineEvent::Lifecycle(
                    LifecycleEvent::PushFailed {
                        remote,
                        branch,
                        error: output.stderr,
                    },
                )))
            }
            Err(e) => {
                ctx.logger
                    .warn(&format!("Git push execution failed: {}", e));
                Ok(EffectResult::event(PipelineEvent::Lifecycle(
                    LifecycleEvent::PushFailed {
                        remote,
                        branch,
                        error: e.to_string(),
                    },
                )))
            }
        }
    }

    /// Create a pull request on the remote platform.
    ///
    /// Uses gh CLI for GitHub or glab CLI for GitLab.
    pub(super) fn handle_create_pull_request(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        base_branch: String,
        head_branch: String,
        title: String,
        body: String,
    ) -> Result<EffectResult> {
        ctx.logger
            .info(&format!("Creating PR: {} -> {}", head_branch, base_branch));

        // Try gh CLI first (GitHub)
        let gh_result = ctx.executor.execute(
            "gh",
            &[
                "pr",
                "create",
                "--base",
                &base_branch,
                "--head",
                &head_branch,
                "--title",
                &title,
                "--body",
                &body,
            ],
            &[],
            Some(ctx.repo_root),
        );

        match gh_result {
            Ok(output) if output.status.success() => {
                let url = output.stdout.trim().to_string();
                ctx.logger.info(&format!("Pull request created: {}", url));

                // Extract PR number from URL if possible
                let number = url
                    .rsplit('/')
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);

                Ok(EffectResult::event(PipelineEvent::Lifecycle(
                    LifecycleEvent::PullRequestCreated { url, number },
                )))
            }
            Ok(output) => {
                ctx.logger
                    .warn(&format!("PR creation failed: {}", output.stderr));
                Ok(EffectResult::event(PipelineEvent::Lifecycle(
                    LifecycleEvent::PullRequestFailed {
                        error: output.stderr,
                    },
                )))
            }
            Err(e) => {
                // gh CLI not available, try glab (GitLab)
                ctx.logger
                    .info("gh CLI not available, trying glab for GitLab");

                let glab_result = ctx.executor.execute(
                    "glab",
                    &[
                        "mr",
                        "create",
                        "--target-branch",
                        &base_branch,
                        "--source-branch",
                        &head_branch,
                        "--title",
                        &title,
                        "--description",
                        &body,
                    ],
                    &[],
                    Some(ctx.repo_root),
                );

                match glab_result {
                    Ok(output) if output.status.success() => {
                        let url = output.stdout.trim().to_string();
                        ctx.logger.info(&format!("Merge request created: {}", url));

                        let number = url
                            .rsplit('/')
                            .next()
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);

                        Ok(EffectResult::event(PipelineEvent::Lifecycle(
                            LifecycleEvent::PullRequestCreated { url, number },
                        )))
                    }
                    Ok(output) => {
                        ctx.logger
                            .warn(&format!("MR creation failed: {}", output.stderr));
                        Ok(EffectResult::event(PipelineEvent::Lifecycle(
                            LifecycleEvent::PullRequestFailed {
                                error: output.stderr,
                            },
                        )))
                    }
                    Err(e2) => {
                        ctx.logger.warn(&format!(
                            "Neither gh nor glab CLI available: gh error: {}, glab error: {}",
                            e, e2
                        ));
                        Ok(EffectResult::event(PipelineEvent::Lifecycle(
                            LifecycleEvent::PullRequestFailed {
                                error: format!(
                                    "Neither gh nor glab CLI available (gh: {}, glab: {})",
                                    e, e2
                                ),
                            },
                        )))
                    }
                }
            }
        }
    }

    /// Report progress to cloud API.
    ///
    /// Sends progress update to cloud reporter if available.
    pub(super) fn handle_report_cloud_progress(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        update: crate::cloud::types::ProgressUpdate,
    ) -> Result<EffectResult> {
        if let Some(reporter) = ctx.cloud_reporter {
            match reporter.report_progress(&update) {
                Ok(()) => Ok(EffectResult::event(PipelineEvent::Lifecycle(
                    LifecycleEvent::CloudProgressReported,
                ))),
                Err(e) => {
                    // Graceful degradation: log but don't fail pipeline
                    ctx.logger
                        .warn(&format!("Cloud progress report failed: {}", e));
                    Ok(EffectResult::event(PipelineEvent::Lifecycle(
                        LifecycleEvent::CloudProgressFailed {
                            error: e.to_string(),
                        },
                    )))
                }
            }
        } else {
            // No cloud reporter configured (CLI mode) - this is a no-op
            Ok(EffectResult::event(PipelineEvent::Lifecycle(
                LifecycleEvent::CloudProgressReported,
            )))
        }
    }
}
