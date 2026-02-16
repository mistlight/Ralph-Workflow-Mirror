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
use crate::reducer::event::{CommitEvent, PipelineEvent};
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
                    // Set GIT_SSH_COMMAND to use specific key.
                    // Git may execute this via a shell; treat the key path as untrusted.
                    if let Some(cmd) = build_git_ssh_command(param) {
                        std::env::set_var("GIT_SSH_COMMAND", &cmd);
                        ctx.logger
                            .info("Set GIT_SSH_COMMAND to use provided SSH key");
                    } else {
                        ctx.logger.warn(
                            "Invalid SSH key path for cloud git auth; falling back to default SSH",
                        );
                    }
                } else {
                    // Use default SSH key (SSH_AUTH_SOCK or ~/.ssh/id_rsa)
                    ctx.logger
                        .info("Using default SSH authentication (SSH_AUTH_SOCK or ~/.ssh/id_rsa)");
                }
            }
            "token" => {
                // Configure token-based authentication.
                // We intentionally do NOT embed or log the token.
                // Push operations use a non-persistent credential helper that reads the token
                // from environment variables at runtime.
                ctx.logger.info(&format!(
                    "Configuring token authentication for user: {}",
                    param
                ));
                std::env::set_var("GIT_TERMINAL_PROMPT", "0");
            }
            "credential-helper" => {
                // Configure external credential helper
                ctx.logger
                    .info(&format!("Using credential helper: {}", param));
                std::env::set_var("GIT_TERMINAL_PROMPT", "0");
            }
            _ => {
                ctx.logger.warn(&format!(
                    "Unknown auth method: {}, falling back to default SSH",
                    method
                ));
            }
        }

        Ok(EffectResult::event(PipelineEvent::Commit(
            CommitEvent::GitAuthConfigured,
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

        // Build git push command.
        // Auth is configured in a checkpoint-safe way:
        // - ssh-key: via GIT_SSH_COMMAND (set in ConfigureGitAuth)
        // - token: via ephemeral credential helper that reads token from env
        // - credential-helper: via per-command credential.helper override
        let mut argv: Vec<String> = Vec::new();

        match &ctx.cloud_config.git_remote.auth_method {
            crate::config::types::GitAuthMethod::SshKey { .. } => {}
            crate::config::types::GitAuthMethod::Token { .. } => {
                // This helper is executed by git via `sh -c` (leading '!').
                // It prints credentials without ever storing secrets in repo files.
                argv.push("-c".to_string());
                argv.push(
                    "credential.helper=!f() { echo username=$RALPH_GIT_TOKEN_USERNAME; echo password=$RALPH_GIT_TOKEN; }; f"
                        .to_string(),
                );
                argv.push("-c".to_string());
                argv.push("credential.useHttpPath=true".to_string());
            }
            crate::config::types::GitAuthMethod::CredentialHelper { helper } => {
                argv.push("-c".to_string());
                argv.push(format!("credential.helper={helper}"));
                argv.push("-c".to_string());
                argv.push("credential.useHttpPath=true".to_string());
            }
        }

        argv.push("push".to_string());
        argv.push(remote.clone());
        argv.push(branch.clone());
        if force {
            argv.push("--force".to_string());
        }

        let args: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();

        // Execute push via executor
        let result = ctx.executor.execute("git", &args, &[], Some(ctx.repo_root));

        match result {
            Ok(output) if output.status.success() => {
                ctx.logger
                    .info(&format!("Successfully pushed to {}/{}", remote, branch));
                Ok(EffectResult::event(PipelineEvent::Commit(
                    CommitEvent::PushCompleted {
                        remote,
                        branch,
                        commit_sha,
                    },
                )))
            }
            Ok(output) => {
                let error = crate::cloud::redaction::redact_secrets(&output.stderr);
                ctx.logger.warn(&format!("Git push failed: {error}"));
                Ok(EffectResult::event(PipelineEvent::Commit(
                    CommitEvent::PushFailed {
                        remote,
                        branch,
                        error,
                    },
                )))
            }
            Err(e) => {
                let error = crate::cloud::redaction::redact_secrets(&e.to_string());
                ctx.logger
                    .warn(&format!("Git push execution failed: {error}"));
                Ok(EffectResult::event(PipelineEvent::Commit(
                    CommitEvent::PushFailed {
                        remote,
                        branch,
                        error,
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

                Ok(EffectResult::event(PipelineEvent::Commit(
                    CommitEvent::PullRequestCreated { url, number },
                )))
            }
            Ok(output) => {
                let error = crate::cloud::redaction::redact_secrets(&output.stderr);
                ctx.logger.warn(&format!("PR creation failed: {error}"));
                Ok(EffectResult::event(PipelineEvent::Commit(
                    CommitEvent::PullRequestFailed { error },
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

                        Ok(EffectResult::event(PipelineEvent::Commit(
                            CommitEvent::PullRequestCreated { url, number },
                        )))
                    }
                    Ok(output) => {
                        let error = crate::cloud::redaction::redact_secrets(&output.stderr);
                        ctx.logger.warn(&format!("MR creation failed: {error}"));
                        Ok(EffectResult::event(PipelineEvent::Commit(
                            CommitEvent::PullRequestFailed { error },
                        )))
                    }
                    Err(e2) => {
                        let e = crate::cloud::redaction::redact_secrets(&e.to_string());
                        let e2 = crate::cloud::redaction::redact_secrets(&e2.to_string());
                        ctx.logger.warn(&format!(
                            "Neither gh nor glab CLI available: gh error: {e}, glab error: {e2}",
                        ));
                        Ok(EffectResult::event(PipelineEvent::Commit(
                            CommitEvent::PullRequestFailed {
                                error: format!(
                                    "Neither gh nor glab CLI available (gh: {e}, glab: {e2})",
                                ),
                            },
                        )))
                    }
                }
            }
        }
    }
}

fn build_git_ssh_command(key_path: &str) -> Option<String> {
    if key_path.trim().is_empty() {
        return None;
    }
    // Reject control characters that could smuggle new shell tokens.
    if key_path.contains('\0') || key_path.contains('\n') || key_path.contains('\r') {
        return None;
    }

    let escaped = shell_escape_posix(key_path);
    Some(format!(
        "ssh -i {escaped} -o StrictHostKeyChecking=accept-new",
    ))
}

fn shell_escape_posix(s: &str) -> String {
    // POSIX shell escaping via single quotes.
    // Example: abc'def -> 'abc'"'"'def'
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\"'\"'");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod shell_escape_tests {
    use super::{build_git_ssh_command, shell_escape_posix};

    #[test]
    fn shell_escape_wraps_in_single_quotes() {
        assert_eq!(shell_escape_posix("/a b"), "'/a b'");
    }

    #[test]
    fn shell_escape_handles_single_quotes() {
        assert_eq!(shell_escape_posix("a'b"), "'a'\"'\"'b'");
    }

    #[test]
    fn build_git_ssh_command_rejects_newlines() {
        assert!(build_git_ssh_command("/tmp/key\n-oProxyCommand=evil").is_none());
    }
}
