//! CCS (Claude Code Switch) environment variable loading.
//!
//! This module provides support for loading environment variables from CCS
//! settings files. CCS stores profile -> settings file mappings in
//! `~/.ccs/config.json` and/or `~/.ccs/config.yaml`, and stores environment
//! variables inside the settings file under the `env` key.
//!
//! Source (CCS): `dist/utils/config-manager.js` and `dist/types/config.d.ts`.

use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Subset of CCS' legacy `~/.ccs/config.json` format.
///
/// Source (CCS): `dist/types/config.d.ts` and `dist/utils/config-manager.js`.
#[derive(Debug, Deserialize)]
struct CcsConfigJson {
    profiles: HashMap<String, String>,
}

/// Errors that can occur when loading CCS environment variables.
#[derive(Debug, thiserror::Error)]
pub enum CcsEnvVarsError {
    #[error("Invalid CCS profile name '{profile}' (must be non-empty)")]
    InvalidProfile { profile: String },
    #[error("Could not determine home directory for CCS settings")]
    MissingHomeDir,
    #[error("No CCS settings file found for profile '{profile}' in {ccs_dir}")]
    ProfileNotFound { profile: String, ccs_dir: PathBuf },
    #[error("Failed to read CCS config at {path}: {source}")]
    ReadConfig { path: PathBuf, source: io::Error },
    #[error("Failed to parse CCS config JSON at {path}: {source}")]
    ParseConfigJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("Failed to read CCS settings file at {path}: {source}")]
    ReadFile { path: PathBuf, source: io::Error },
    #[error("Failed to parse CCS settings JSON at {path}: {source}")]
    ParseJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("Could not find an environment-variable map in CCS settings JSON at {path}")]
    MissingEnv { path: PathBuf },
    #[error("CCS settings JSON at {path} contains invalid env var name '{key}'")]
    InvalidEnvVarName { path: PathBuf, key: String },
    #[error("CCS settings JSON at {path} has non-string env value for key '{key}'")]
    NonStringEnvVarValue { path: PathBuf, key: String },
    #[error("CCS settings JSON at {path} contains dangerous env var '{key}' (not allowed from external config)")]
    DangerousEnvVar { path: PathBuf, key: String },
    #[error("CCS settings JSON at {path} contains unsafe env value for key '{key}'")]
    UnsafeEnvVarValue { path: PathBuf, key: String },
    #[error("CCS config at {path} contains unsafe settings path '{settings_path}' (path traversal not allowed)")]
    UnsafeSettingsPath {
        path: PathBuf,
        settings_path: String,
    },
}

/// List of dangerous environment variable names that should not be allowed from external config.
const DANGEROUS_ENV_VAR_NAMES: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "IFS",
    "PATH",
    "SHELL",
    "ENV",
    "BASH_ENV",
];

/// Check if an environment variable name is dangerous (could be used for injection).
fn is_dangerous_env_var_name(name: &str) -> bool {
    DANGEROUS_ENV_VAR_NAMES
        .iter()
        .any(|&dangerous| name.eq_ignore_ascii_case(dangerous))
}

fn is_valid_env_var_name_portable(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.contains('\0') || name.contains('=') {
        return false;
    }
    // On Windows, environment variable names cannot start with '='.
    #[cfg(windows)]
    {
        if name.starts_with('=') {
            return false;
        }
    }
    true
}

/// Validate environment variable value for safety.
/// Only allows printable ASCII and common Unicode characters, no control characters
/// or characters that could escape the value context in shells.
fn is_safe_env_var_value(value: &str) -> bool {
    // Reject null bytes and newlines (could be used for injection)
    if value.contains('\0') || value.contains('\n') || value.contains('\r') {
        return false;
    }
    // Reject backticks (command substitution in shells)
    if value.contains('`') {
        return false;
    }
    // Allow most other characters - environment variable values typically
    // don't need strict character restrictions beyond these injection checks
    true
}

fn derive_ccs_profile_name_from_filename(filename: &str) -> Option<String> {
    filename
        .strip_suffix(".settings.json")
        .or_else(|| filename.strip_suffix(".setting.json"))
        .or_else(|| filename.strip_suffix(".json"))
        .map(std::string::ToString::to_string)
}

fn is_ccs_settings_filename(name: &str) -> bool {
    name.ends_with(".settings.json") || name.ends_with(".setting.json")
}

fn is_safe_profile_filename_stem(stem: &str) -> bool {
    !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn list_ccs_json_files(ccs_dir: &Path) -> Result<Vec<PathBuf>, io::Error> {
    let entries = fs::read_dir(ccs_dir)?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let ft = entry.file_type()?;
        if !ft.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".json") {
            files.push(entry.path());
        }
    }
    Ok(files)
}

fn ccs_home_dir() -> Option<PathBuf> {
    // Matches CCS behavior: respects CCS_HOME env var for test isolation.
    env::var_os("CCS_HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}

fn ccs_dir() -> Option<PathBuf> {
    ccs_home_dir().map(|home| home.join(".ccs"))
}

fn ccs_config_json_path() -> Option<PathBuf> {
    // Matches CCS behavior: CCS_CONFIG overrides config.json path.
    // Source (CCS): `dist/utils/config-manager.js` getConfigPath().
    env::var_os("CCS_CONFIG")
        .map(PathBuf::from)
        .or_else(|| ccs_dir().map(|d| d.join("config.json")))
}

fn ccs_config_yaml_path() -> Option<PathBuf> {
    ccs_dir().map(|d| d.join("config.yaml"))
}

fn load_ccs_profiles_from_config_json() -> Result<HashMap<String, String>, CcsEnvVarsError> {
    let Some(path) = ccs_config_json_path() else {
        return Err(CcsEnvVarsError::MissingHomeDir);
    };
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(&path).map_err(|source| CcsEnvVarsError::ReadConfig {
        path: path.clone(),
        source,
    })?;
    let parsed: CcsConfigJson =
        serde_json::from_str(&content).map_err(|source| CcsEnvVarsError::ParseConfigJson {
            path: path.clone(),
            source,
        })?;
    Ok(parsed.profiles)
}

fn parse_ccs_profiles_from_config_yaml(content: &str) -> HashMap<String, String> {
    // Minimal YAML extractor for CCS `config.yaml`.
    // Source (CCS): `dist/config/unified-config-types.d.ts` and
    // `dist/utils/config-manager.js` getSettingsPath() uses `profiles.<name>.settings`.
    //
    // CCS writes this file via js-yaml with quotingType='"', producing a predictable shape:
    // profiles:
    //   glm:
    //     type: api
    //     settings: "~/.ccs/glm.settings.json"
    let mut in_profiles = false;
    let mut profiles_indent = 0usize;
    let mut current_profile: Option<(String, usize)> = None;
    let mut out: HashMap<String, String> = HashMap::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }

        let indent = line.len().saturating_sub(trimmed.len());

        if !in_profiles {
            if trimmed == "profiles:" {
                in_profiles = true;
                profiles_indent = indent;
                continue;
            }
            continue;
        }

        // End of `profiles:` block when indentation drops back.
        if indent <= profiles_indent {
            break;
        }

        // Profile entry line: "<indent> name:" (or with inline mapping).
        //
        // CCS writes YAML with two-space indentation, but be tolerant of other indentation
        // styles to reduce surprising `ProfileNotFound` behavior.
        if indent > profiles_indent && current_profile.is_none() {
            if let Some((name, rest)) = trimmed.split_once(':') {
                let profile_name = name.trim().to_string();
                let rest = rest.trim();
                current_profile = Some((profile_name.clone(), indent));

                // Inline mapping form: name: { ..., settings: "..." }
                if rest.contains("settings:") {
                    if let Some(settings) = extract_yaml_inline_settings_value(rest) {
                        out.insert(profile_name, settings);
                    }
                }
                continue;
            }
        }

        // Nested under a profile. Look for "settings:".
        if let Some((profile_name, profile_indent)) = current_profile.as_ref() {
            if indent <= *profile_indent {
                // We've left the current profile's block.
                current_profile = None;
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("settings:") {
                let settings = unquote_yaml_scalar(value.trim());
                if !settings.is_empty() {
                    out.insert(profile_name.clone(), settings);
                }
            }
        }
    }

    out
}

fn extract_yaml_inline_settings_value(inline: &str) -> Option<String> {
    // Very small parser for "{ ..., settings: \"...\" }" emitted by yaml.dump().
    let needle = "settings:";
    let idx = inline.find(needle)?;
    let after = inline[idx + needle.len()..].trim_start();
    let token = after
        .split(',')
        .next()
        .unwrap_or(after)
        .trim()
        .trim_end_matches('}')
        .trim();
    let value = unquote_yaml_scalar(token);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn unquote_yaml_scalar(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        let inner = &v[1..v.len() - 1];
        // CCS uses js-yaml with double quotes; keep unescaping minimal for paths.
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        v.to_string()
    }
}

fn load_ccs_profiles_from_config_yaml() -> Result<HashMap<String, String>, CcsEnvVarsError> {
    let Some(path) = ccs_config_yaml_path() else {
        return Err(CcsEnvVarsError::MissingHomeDir);
    };
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(&path).map_err(|source| CcsEnvVarsError::ReadConfig {
        path: path.clone(),
        source,
    })?;
    Ok(parse_ccs_profiles_from_config_yaml(&content))
}

fn resolve_ccs_settings_path(profile: &str) -> Result<PathBuf, CcsEnvVarsError> {
    let Some(ccs_dir) = ccs_dir() else {
        return Err(CcsEnvVarsError::MissingHomeDir);
    };

    // 1) Unified YAML config (preferred by CCS when present).
    let yaml_profiles = load_ccs_profiles_from_config_yaml()?;
    if let Some(settings) = yaml_profiles.get(profile) {
        // Validate path doesn't use traversal or absolute paths
        if !is_path_safe_for_resolution(settings) {
            return Err(CcsEnvVarsError::UnsafeSettingsPath {
                path: ccs_dir.join("config.yaml"),
                settings_path: settings.clone(),
            });
        }
        return Ok(expand_user_path(settings));
    }

    // 2) Legacy config.json.
    let json_profiles = load_ccs_profiles_from_config_json()?;
    if let Some(settings) = json_profiles.get(profile) {
        // Validate path doesn't use traversal or absolute paths
        if !is_path_safe_for_resolution(settings) {
            return Err(CcsEnvVarsError::UnsafeSettingsPath {
                path: ccs_dir.join("config.json"),
                settings_path: settings.clone(),
            });
        }
        return Ok(expand_user_path(settings));
    }

    // 3) Fallback: direct profile settings file in ~/.ccs/ (common default path).
    // Source (CCS): unified config docs and type comments use "~/.ccs/<profile>.settings.json".
    if is_safe_profile_filename_stem(profile) {
        let candidates = [
            ccs_dir.join(format!("{profile}.settings.json")),
            ccs_dir.join(format!("{profile}.setting.json")),
        ];
        for candidate in candidates {
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(CcsEnvVarsError::ProfileNotFound {
        profile: profile.to_string(),
        ccs_dir,
    })
}

/// Check if a path string is absolute (starts with / or is a Windows drive/UNC path).
/// Returns true if the path is absolute.
fn is_absolute_path(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    if cfg!(windows) {
        let mut chars = path.chars();
        match (chars.next(), chars.next()) {
            // UNC paths: \\server\share or \\?\device, or Drive letter paths: C:\
            (Some('\\'), Some('\\')) | (Some(_), Some(':')) => return true,
            _ => {}
        }
    }
    false
}

/// Validate that a path doesn't escape the intended directory through traversal.
/// Returns true if the path is safe (no `..` components, no absolute paths).
fn is_path_safe_for_resolution(path: &str) -> bool {
    // Reject absolute paths - they could point anywhere on the filesystem
    if is_absolute_path(path) {
        return false;
    }
    // Reject paths containing parent directory references
    if path.contains("..") {
        return false;
    }
    // Reject paths with null bytes
    if path.contains('\0') {
        return false;
    }
    true
}

fn expand_user_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = ccs_home_dir() {
            return home.join(rest);
        }
    }
    // Relative paths are resolved relative to the CCS directory
    if let Some(ccs_dir) = ccs_dir() {
        // If path is not absolute and doesn't start with ~, it's a relative path
        if !is_absolute_path(path) {
            return ccs_dir.join(path);
        }
    }
    PathBuf::from(path)
}

fn find_env_object(json: &JsonValue) -> Option<&serde_json::Map<String, JsonValue>> {
    // Source (CCS): `dist/types/config.d.ts` defines Settings as:
    //   { env?: Record<string, string>, ... }
    // and `dist/types/config.js` validates env values are strings.
    json.as_object()?.get("env")?.as_object()
}

/// List all available CCS profile names from JSON files under `~/.ccs/`.
///
/// Returns a Vec of profile names (derived from file names like
/// `{profile}.settings.json`, `{profile}.setting.json`, or `{profile}.json`).
/// Returns an empty Vec if the .ccs directory doesn't exist or cannot be read.
pub fn list_available_ccs_profiles() -> Vec<String> {
    let Some(ccs_dir) = ccs_dir() else {
        return Vec::new();
    };

    let mut unique = std::collections::HashSet::new();

    if let Ok(yaml_profiles) = load_ccs_profiles_from_config_yaml() {
        unique.extend(yaml_profiles.keys().cloned());
    }
    if let Ok(json_profiles) = load_ccs_profiles_from_config_json() {
        unique.extend(json_profiles.keys().cloned());
    }

    // Also include any *.settings.json files in ~/.ccs (common default path).
    if let Ok(files) = list_ccs_json_files(&ccs_dir) {
        for path in files {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_ccs_settings_filename(name) {
                    if let Some(profile) = derive_ccs_profile_name_from_filename(name) {
                        unique.insert(profile);
                    }
                }
            }
        }
    }

    let mut profiles = unique.into_iter().collect::<Vec<_>>();
    profiles.sort();
    profiles
}

/// Find suggestions for a CCS profile name using case-insensitive matching.
///
/// Returns profile names from ~/.ccs that match the input case-insensitively,
/// or contain the input as a substring.
///
/// # Arguments
///
/// * `input` - The profile name the user tried to use
///
/// # Returns
///
/// A Vec of suggested profile names. Empty if no matches found.
pub fn find_ccs_profile_suggestions(input: &str) -> Vec<String> {
    let available = list_available_ccs_profiles();
    let input_lower = input.to_lowercase();

    available
        .into_iter()
        .filter(|profile| {
            let profile_lower = profile.to_lowercase();
            // Exact case-insensitive match
            profile_lower == input_lower
                || // Substring match (user typed part of the name)
                profile_lower.contains(&input_lower) ||
                input_lower.contains(&profile_lower)
        })
        .collect()
}

/// Load environment variables from a CCS settings file.
///
/// CCS stores profile -> settings file mapping in `~/.ccs/config.json` and/or
/// `~/.ccs/config.yaml`, and stores environment variables inside the settings file
/// under the `env` key (values must be strings).
///
/// Source (CCS): `dist/utils/config-manager.js` and `dist/types/config.d.ts`.
///
/// All key/value pairs found in the env map are imported as temporary process
/// environment variables for the agent invocation (they are not persisted).
///
/// # Arguments
///
/// * `profile` - The CCS profile name (e.g., "glm" for a matching `~/.ccs/glm.settings.json` file)
///
/// # Returns
///
/// Returns `HashMap<String, String>` with environment variables if successful.
/// Returns an error with context if the settings file cannot be read/parsed.
///
/// # Example
///
/// ```ignore
/// let env_vars = load_ccs_env_vars("glm").unwrap_or_default();
/// // env_vars contains: {
/// //   "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic",
/// //   "ANTHROPIC_AUTH_TOKEN": "...",
/// //   "ANTHROPIC_MODEL": "glm-4.7",
/// // }
/// ```
pub fn load_ccs_env_vars(profile: &str) -> Result<HashMap<String, String>, CcsEnvVarsError> {
    if profile.is_empty() {
        return Err(CcsEnvVarsError::InvalidProfile {
            profile: profile.to_string(),
        });
    }

    let settings_path = resolve_ccs_settings_path(profile)?;

    // Read and parse the settings file
    let content =
        fs::read_to_string(&settings_path).map_err(|source| CcsEnvVarsError::ReadFile {
            path: settings_path.clone(),
            source,
        })?;

    // Parse JSON
    let json: JsonValue =
        serde_json::from_str(&content).map_err(|source| CcsEnvVarsError::ParseJson {
            path: settings_path.clone(),
            source,
        })?;

    let env_obj = find_env_object(&json).ok_or_else(|| CcsEnvVarsError::MissingEnv {
        path: settings_path.clone(),
    })?;

    // Convert to HashMap<String, String>
    let mut env_vars = HashMap::new();
    for (key, value) in env_obj {
        if !is_valid_env_var_name_portable(key) {
            return Err(CcsEnvVarsError::InvalidEnvVarName {
                path: settings_path.clone(),
                key: key.clone(),
            });
        }
        // Reject dangerous environment variable names that could be used for injection
        if is_dangerous_env_var_name(key) {
            return Err(CcsEnvVarsError::DangerousEnvVar {
                path: settings_path.clone(),
                key: key.clone(),
            });
        }
        let str_value = value
            .as_str()
            .ok_or_else(|| CcsEnvVarsError::NonStringEnvVarValue {
                path: settings_path.clone(),
                key: key.clone(),
            })?;
        // Validate environment variable values for safety
        if !is_safe_env_var_value(str_value) {
            return Err(CcsEnvVarsError::UnsafeEnvVarValue {
                path: settings_path.clone(),
                key: key.clone(),
            });
        }
        env_vars.insert(key.clone(), str_value.to_string());
    }

    Ok(env_vars)
}

/// Find the claude CLI binary path.
///
/// Returns the path to the claude command if found in PATH.
/// Returns None if claude is not installed or not in PATH.
pub fn find_claude_binary() -> Option<PathBuf> {
    which::which("claude").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        prev: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let prev = env::var_os(key);
            env::set_var(key, value);
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(v) => env::set_var(self.key, v),
                None => env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn load_ccs_env_vars_uses_config_json_mapping_and_env_key() {
        let _lock = ENV_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        let settings_path = ccs_dir.join("glm.settings.json");
        fs::write(
            &settings_path,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://example","ANTHROPIC_AUTH_TOKEN":"token"}}"#,
        )
        .unwrap();

        // Use relative path from CCS directory (intended usage pattern)
        // This avoids absolute path rejection by is_path_safe_for_resolution
        fs::write(
            ccs_dir.join("config.json"),
            r#"{"profiles":{"glm":"glm.settings.json"}}"#,
        )
        .unwrap();

        let env_vars = load_ccs_env_vars("glm").unwrap();
        assert_eq!(
            env_vars.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://example"
        );
        assert_eq!(env_vars.get("ANTHROPIC_AUTH_TOKEN").unwrap(), "token");
    }

    #[test]
    fn load_ccs_env_vars_from_yaml_config() {
        let _lock = ENV_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        fs::write(
            ccs_dir.join("custom.settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://yaml-test","ANTHROPIC_MODEL":"test-model"}}"#,
        )
        .unwrap();

        // Use relative path from CCS directory (intended usage pattern)
        fs::write(
            ccs_dir.join("config.yaml"),
            r#"version: 7
profiles:
  custom:
    type: api
    settings: "custom.settings.json"
"#,
        )
        .unwrap();

        let env_vars = load_ccs_env_vars("custom").unwrap();
        assert_eq!(
            env_vars.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://yaml-test"
        );
        assert_eq!(env_vars.get("ANTHROPIC_MODEL").unwrap(), "test-model");
    }

    #[test]
    fn load_ccs_env_vars_from_yaml_config_with_nonstandard_indent() {
        let _lock = ENV_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        fs::write(
            ccs_dir.join("indent.settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://indent-test","ANTHROPIC_MODEL":"indent-model"}}"#,
        )
        .unwrap();

        // Use relative path from CCS directory (intended usage pattern)
        // Same structure as CCS config.yaml, but with 4-space indentation.
        fs::write(
            ccs_dir.join("config.yaml"),
            r#"version: 7
profiles:
    indent:
        type: api
        settings: "indent.settings.json"
"#,
        )
        .unwrap();

        let env_vars = load_ccs_env_vars("indent").unwrap();
        assert_eq!(
            env_vars.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://indent-test"
        );
        assert_eq!(env_vars.get("ANTHROPIC_MODEL").unwrap(), "indent-model");
    }

    #[test]
    fn load_ccs_env_vars_from_direct_settings_file() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        // Create settings file directly without config.yaml or config.json
        fs::write(
            ccs_dir.join("direct.settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://direct","ANTHROPIC_AUTH_TOKEN":"direct-token"}}"#,
        )
        .unwrap();

        let env_vars = load_ccs_env_vars("direct").unwrap();
        assert_eq!(
            env_vars.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://direct"
        );
        assert_eq!(
            env_vars.get("ANTHROPIC_AUTH_TOKEN").unwrap(),
            "direct-token"
        );
    }

    #[test]
    fn load_ccs_env_vars_profile_not_found() {
        let _lock = ENV_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        // Don't create any config files - profile should not be found
        let result = load_ccs_env_vars("nonexistent");
        assert!(result.is_err());

        match result.unwrap_err() {
            CcsEnvVarsError::ProfileNotFound { profile, .. } => {
                assert_eq!(profile, "nonexistent");
            }
            other => panic!("Expected ProfileNotFound error, got: {other:?}"),
        }
    }

    #[test]
    fn load_ccs_env_vars_alternate_spelling_setting_json() {
        let _lock = ENV_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        // Test alternate spelling .setting.json (without 's')
        fs::write(
            ccs_dir.join("alternate.setting.json"),
            r#"{"env":{"TEST_KEY":"test_value"}}"#,
        )
        .unwrap();

        let env_vars = load_ccs_env_vars("alternate").unwrap();
        assert_eq!(env_vars.get("TEST_KEY").unwrap(), "test_value");
    }

    #[test]
    fn load_ccs_env_vars_missing_env_object() {
        let _lock = ENV_MUTEX
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        // Create settings file without env object
        fs::write(
            ccs_dir.join("noenv.settings.json"),
            r#"{"other_key":"other_value"}"#,
        )
        .unwrap();

        let result = load_ccs_env_vars("noenv");
        assert!(result.is_err());

        match result.unwrap_err() {
            CcsEnvVarsError::MissingEnv { .. } => {}
            other => panic!("Expected MissingEnv error, got: {other:?}"),
        }
    }

    #[test]
    fn load_ccs_env_vars_empty_profile_name() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let result = load_ccs_env_vars("");
        assert!(result.is_err());

        match result.unwrap_err() {
            CcsEnvVarsError::InvalidProfile { profile } => {
                assert_eq!(profile, "");
            }
            other => panic!("Expected InvalidProfile error, got: {other:?}"),
        }
    }

    #[test]
    fn load_ccs_env_vars_expands_tilde_path() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        let settings_path = ccs_dir.join("expand.settings.json");
        fs::write(&settings_path, r#"{"env":{"FROM_EXPAND":"success"}}"#).unwrap();

        // Config with ~/ path that needs expansion
        fs::write(
            ccs_dir.join("config.yaml"),
            r#"version: 7
profiles:
  expand:
    type: api
    settings: "~/.ccs/expand.settings.json"
"#,
        )
        .unwrap();

        let env_vars = load_ccs_env_vars("expand").unwrap();
        assert_eq!(env_vars.get("FROM_EXPAND").unwrap(), "success");
    }

    #[test]
    fn load_ccs_env_vars_does_not_pollute_global_environment() {
        // Regression test for: https://github.com/...
        // Ensures that loading CCS env vars does NOT set them globally.
        // The env vars should only be returned in the HashMap, not set via std::env::set_var.
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        // Create a CCS settings file with GLM-like env vars
        let settings_path = ccs_dir.join("glm.settings.json");
        fs::write(
            &settings_path,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic","ANTHROPIC_AUTH_TOKEN":"test-token-glm","ANTHROPIC_MODEL":"glm-4.7"}}"#,
        )
        .unwrap();

        fs::write(
            ccs_dir.join("config.json"),
            r#"{"profiles":{"glm":"glm.settings.json"}}"#,
        )
        .unwrap();

        // Remember the original state of these env vars
        let original_base_url = env::var("ANTHROPIC_BASE_URL");
        let original_auth_token = env::var("ANTHROPIC_AUTH_TOKEN");
        let original_model = env::var("ANTHROPIC_MODEL");

        // Load CCS env vars - this should ONLY return them in a HashMap
        let env_vars = load_ccs_env_vars("glm").unwrap();

        // Verify the returned HashMap has the correct values
        assert_eq!(
            env_vars.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://api.z.ai/api/anthropic"
        );
        assert_eq!(
            env_vars.get("ANTHROPIC_AUTH_TOKEN").unwrap(),
            "test-token-glm"
        );
        assert_eq!(env_vars.get("ANTHROPIC_MODEL").unwrap(), "glm-4.7");

        // CRITICAL: Verify that the global environment is unchanged
        // This is the regression test - loading CCS env vars should NOT set them globally
        let after_base_url = env::var("ANTHROPIC_BASE_URL");
        let after_auth_token = env::var("ANTHROPIC_AUTH_TOKEN");
        let after_model = env::var("ANTHROPIC_MODEL");

        assert_eq!(
            original_base_url, after_base_url,
            "ANTHROPIC_BASE_URL global environment should be unchanged after load_ccs_env_vars"
        );
        assert_eq!(
            original_auth_token, after_auth_token,
            "ANTHROPIC_AUTH_TOKEN global environment should be unchanged after load_ccs_env_vars"
        );
        assert_eq!(
            original_model, after_model,
            "ANTHROPIC_MODEL global environment should be unchanged after load_ccs_env_vars"
        );
    }

    #[test]
    fn test_multiple_load_ccs_env_vars_calls_isolated() {
        // Regression test ensuring multiple load_ccs_env_vars calls with different
        // profiles don't cross-contaminate. Each call should return independent results.
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        let _guard = EnvGuard::set("CCS_HOME", home);

        let ccs_dir = home.join(".ccs");
        fs::create_dir_all(&ccs_dir).unwrap();

        // Create GLM profile settings
        fs::write(
            ccs_dir.join("glm.settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic","ANTHROPIC_AUTH_TOKEN":"glm-token","ANTHROPIC_MODEL":"glm-4.7"}}"#,
        )
        .unwrap();

        // Create another profile (e.g., "work") with different settings
        fs::write(
            ccs_dir.join("work.settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://work-api.example.com","ANTHROPIC_AUTH_TOKEN":"work-token","ANTHROPIC_MODEL":"claude-sonnet-4"}}"#,
        )
        .unwrap();

        fs::write(
            ccs_dir.join("config.json"),
            r#"{"profiles":{"glm":"glm.settings.json","work":"work.settings.json"}}"#,
        )
        .unwrap();

        // Load GLM profile env vars
        let glm_env = load_ccs_env_vars("glm").unwrap();
        assert_eq!(
            glm_env.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://api.z.ai/api/anthropic"
        );
        assert_eq!(glm_env.get("ANTHROPIC_AUTH_TOKEN").unwrap(), "glm-token");
        assert_eq!(glm_env.get("ANTHROPIC_MODEL").unwrap(), "glm-4.7");

        // Load work profile env vars
        let work_env = load_ccs_env_vars("work").unwrap();
        assert_eq!(
            work_env.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://work-api.example.com"
        );
        assert_eq!(work_env.get("ANTHROPIC_AUTH_TOKEN").unwrap(), "work-token");
        assert_eq!(work_env.get("ANTHROPIC_MODEL").unwrap(), "claude-sonnet-4");

        // Verify the two HashMaps are independent (modifying one doesn't affect the other)
        drop(glm_env);

        // Re-load work profile to verify we get a fresh HashMap
        let work_env2 = load_ccs_env_vars("work").unwrap();
        assert_eq!(
            work_env2.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://work-api.example.com"
        );
        assert!(!work_env2.contains_key("MODIFIED"));
    }
}
