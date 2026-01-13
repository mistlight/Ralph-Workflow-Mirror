//! Git identity resolution with fallback chain.
//!
//! This module provides a comprehensive git identity resolution system that:
//! 1. Reads git config through libgit2's standard mechanisms
//! 2. Adds Ralph-specific configuration options (config file, env vars, CLI args)
//! 3. Implements sensible fallbacks (system username, default values)
//! 4. Provides clear error messages when identity cannot be determined
//!
//! # Priority Chain
//!
//! The identity is resolved in the following order (highest to lowest priority):
//! 1. Explicit CLI args (highest priority)
//! 2. Environment variables (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`)
//! 3. Ralph config file (`[general]` section with `git_user_name`, `git_user_email`)
//! 4. Git config (via libgit2) - maintains backward compatibility
//! 5. System username + derived email (sane fallback)
//! 6. Default values ("Ralph Workflow", "ralph@localhost") - last resort

#![deny(unsafe_code)]

use std::env;

/// Git user identity information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitIdentity {
    /// The user's name for git commits.
    pub name: String,
    /// The user's email for git commits.
    pub email: String,
}

impl GitIdentity {
    /// Create a new GitIdentity with the given name and email.
    pub fn new(name: String, email: String) -> Self {
        Self { name, email }
    }

    /// Validate that the identity is well-formed.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Git user name cannot be empty".to_string());
        }
        if self.email.trim().is_empty() {
            return Err("Git user email cannot be empty".to_string());
        }
        // Basic email validation - check for @ and at least one . after @
        let email = self.email.trim();
        if !email.contains('@') {
            return Err(format!("Invalid email format: '{}'", email));
        }
        let parts: Vec<&str> = email.split('@').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid email format: '{}'", email));
        }
        if parts[0].trim().is_empty() {
            return Err(format!("Invalid email format: '{}' (missing local part)", email));
        }
        if parts[1].trim().is_empty() || !parts[1].contains('.') {
            return Err(format!(
                "Invalid email format: '{}' (invalid domain)",
                email
            ));
        }
        Ok(())
    }
}

/// Get the system username as a fallback.
///
/// Uses platform-specific methods:
/// - On Unix: `whoami` command, fallback to `$USER` env var
/// - On Windows: `%USERNAME%` env var
pub fn fallback_username() -> String {
    // Try environment variables first (fastest)
    if cfg!(unix) {
        if let Ok(user) = env::var("USER") {
            if !user.trim().is_empty() {
                return user.trim().to_string();
            }
        }
        if let Ok(user) = env::var("LOGNAME") {
            if !user.trim().is_empty() {
                return user.trim().to_string();
            }
        }
    } else if cfg!(windows) {
        if let Ok(user) = env::var("USERNAME") {
            if !user.trim().is_empty() {
                return user.trim().to_string();
            }
        }
    }

    // As a last resort, try to run whoami (Unix only)
    if cfg!(unix) {
        match std::process::Command::new("whoami").output() {
            Ok(output) => {
                let username = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !username.is_empty() {
                    return username;
                }
            }
            Err(_) => {} // Fall through to default
        }
    }

    // Ultimate fallback
    "Unknown User".to_string()
}

/// Get a fallback email based on the username.
///
/// Format: `{username}@{hostname}` or `{username}@localhost`
pub fn fallback_email(username: &str) -> String {
    // Try to get hostname
    let hostname = match get_hostname() {
        Some(host) if !host.is_empty() => host,
        _ => "localhost".to_string(),
    };

    format!("{}@{}", username, hostname)
}

/// Get the system hostname.
fn get_hostname() -> Option<String> {
    // Try HOSTNAME environment variable first (fastest)
    if let Ok(hostname) = env::var("HOSTNAME") {
        let hostname = hostname.trim();
        if !hostname.is_empty() {
            return Some(hostname.to_string());
        }
    }

    // Try the `hostname` command
    match std::process::Command::new("hostname").output() {
        Ok(output) => {
            let hostname = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !hostname.is_empty() {
                return Some(hostname);
            }
        }
        Err(_) => {}
    }

    None
}

/// Source of the resolved git identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentitySource {
    /// Identity from CLI arguments.
    CliArgs,
    /// Identity from environment variables.
    Environment,
    /// Identity from Ralph config file.
    RalphConfig,
    /// Identity from git config.
    GitConfig,
    /// Identity from system username/hostname.
    SystemFallback,
    /// Identity from default values.
    Default,
}

/// Resolve git identity with the full priority chain.
///
/// # Arguments
///
/// * `cli_name` - Optional name from CLI arguments
/// * `cli_email` - Optional email from CLI arguments
/// * `config_name` - Optional name from Ralph config
/// * `config_email` - Optional email from Ralph config
///
/// # Returns
///
/// Returns `Ok((GitIdentity, IdentitySource))` with the resolved identity and its source,
/// or `Err(String)` with an error message if identity cannot be determined.
pub fn resolve_git_identity(
    cli_name: Option<&str>,
    cli_email: Option<&str>,
    config_name: Option<&str>,
    config_email: Option<&str>,
) -> Result<(GitIdentity, IdentitySource), String> {
    // Priority 1: CLI arguments (highest)
    if let (Some(name), Some(email)) = (cli_name, cli_email) {
        let identity = GitIdentity::new(name.to_string(), email.to_string());
        if let Err(e) = identity.validate() {
            return Err(format!("CLI git identity validation failed: {}", e));
        }
        return Ok((identity, IdentitySource::CliArgs));
    }

    // Priority 2: Environment variables
    let env_name = env::var("RALPH_GIT_USER_NAME").ok();
    let env_email = env::var("RALPH_GIT_USER_EMAIL").ok();

    if let (Some(name), Some(email)) = (env_name.as_ref(), env_email.as_ref()) {
        let name = name.trim();
        let email = email.trim();
        if !name.is_empty() && !email.is_empty() {
            let identity = GitIdentity::new(name.to_string(), email.to_string());
            if let Err(e) = identity.validate() {
                return Err(format!("Environment git identity validation failed: {}", e));
            }
            return Ok((identity, IdentitySource::Environment));
        }
    }

    // Priority 3: Ralph config file
    if let (Some(name), Some(email)) = (config_name, config_email) {
        let name = name.trim();
        let email = email.trim();
        if !name.is_empty() && !email.is_empty() {
            let identity = GitIdentity::new(name.to_string(), email.to_string());
            if let Err(e) = identity.validate() {
                return Err(format!("Config git identity validation failed: {}", e));
            }
            return Ok((identity, IdentitySource::RalphConfig));
        }
    }

    // Priority 4: Git config (via libgit2)
    // Note: This is handled by the caller (git_commit function) because
    // it needs access to the git2::Repository

    // For now, we'll return that git config should be checked
    // If git config fails, we fall through to our own fallbacks

    // Priority 5: System username + derived email
    let username = fallback_username();
    let email = fallback_email(&username);
    let identity = GitIdentity::new(username.clone(), email);
    if let Err(e) = identity.validate() {
        // Shouldn't happen with our fallbacks, but handle it
        return Err(format!("System fallback git identity validation failed: {}", e));
    }
    Ok((identity, IdentitySource::SystemFallback))
}

/// Get the default git identity (last resort).
///
/// This should never be reached if the fallback chain is working correctly.
pub fn default_identity() -> GitIdentity {
    GitIdentity::new("Ralph Workflow".to_string(), "ralph@localhost".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_identity_validation_valid() {
        let identity = GitIdentity::new("Test User".to_string(), "test@example.com".to_string());
        assert!(identity.validate().is_ok());
    }

    #[test]
    fn test_git_identity_validation_empty_name() {
        let identity = GitIdentity::new("".to_string(), "test@example.com".to_string());
        assert!(identity
            .validate()
            .contains_err(&"Git user name cannot be empty"));
    }

    #[test]
    fn test_git_identity_validation_empty_email() {
        let identity = GitIdentity::new("Test User".to_string(), "".to_string());
        assert!(identity
            .validate()
            .contains_err(&"Git user email cannot be empty"));
    }

    #[test]
    fn test_git_identity_validation_invalid_email_no_at() {
        let identity = GitIdentity::new("Test User".to_string(), "invalidemail".to_string());
        assert!(identity.validate().contains_err(&"Invalid email format"));
    }

    #[test]
    fn test_git_identity_validation_invalid_email_no_domain() {
        let identity = GitIdentity::new("Test User".to_string(), "user@".to_string());
        assert!(identity.validate().contains_err(&"Invalid email format"));
    }

    #[test]
    fn test_fallback_username_not_empty() {
        let username = fallback_username();
        assert!(!username.is_empty());
    }

    #[test]
    fn test_fallback_email_format() {
        let username = "testuser";
        let email = fallback_email(username);
        assert!(email.contains('@'));
        assert!(email.starts_with(username));
    }

    #[test]
    fn test_resolve_git_identity_cli_args() {
        let (identity, source) =
            resolve_git_identity(Some("CLI User"), Some("cli@example.com"), None, None)
                .unwrap();
        assert_eq!(identity.name, "CLI User");
        assert_eq!(identity.email, "cli@example.com");
        assert_eq!(source, IdentitySource::CliArgs);
    }

    #[test]
    fn test_resolve_git_identity_config() {
        let (identity, source) =
            resolve_git_identity(None, None, Some("Config User"), Some("config@example.com"))
                .unwrap();
        assert_eq!(identity.name, "Config User");
        assert_eq!(identity.email, "config@example.com");
        assert_eq!(source, IdentitySource::RalphConfig);
    }

    #[test]
    fn test_resolve_git_identity_fallback() {
        let (identity, source) = resolve_git_identity(None, None, None, None).unwrap();
        assert!(!identity.name.is_empty());
        assert!(identity.email.contains('@'));
        assert_eq!(source, IdentitySource::SystemFallback);
    }

    #[test]
    fn test_default_identity() {
        let identity = default_identity();
        assert_eq!(identity.name, "Ralph Workflow");
        assert_eq!(identity.email, "ralph@localhost");
    }
}

/// Helper trait for error checking in tests
#[cfg(test)]
trait ContainsErr {
    fn contains_err(&self, needle: &str) -> bool;
}

#[cfg(test)]
impl ContainsErr for Result<(), String> {
    fn contains_err(&self, needle: &str) -> bool {
        match self {
            Err(e) => e.contains(needle),
            _ => false,
        }
    }
}
