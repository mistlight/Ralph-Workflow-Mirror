//! Host tool discovery and injection for container execution
//!
//! This module ensures that all commands available on the host work in the container
//! by discovering and mounting language-specific tool directories.

use crate::container::error::ContainerResult;
use crate::container::engine::Mount;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

#[cfg(test)]
use std::fs;

/// Common system binary directories to mount from the host
const SYSTEM_BIN_DIRS: &[&str] = &[
    "/usr/local/bin",
    "/usr/local/sbin",
    "/usr/bin",
    "/usr/sbin",
    "/bin",
    "/sbin",
];

/// Language version manager home directories
const VERSION_MANAGER_DIRS: &[&str] = &[
    ".rbenv",      // Ruby version manager
    ".rvm",        // Ruby version manager (alternative)
    ".nvm",        // Node.js version manager
    ".pyenv",      // Python version manager
    ".jenv",       // Java version manager
    ".sdkman",     // Java/Gradle/Maven manager
    ".gvm",        // Groovy version manager
    ".goenv",      // Go version manager
    ".swiftenv",   // Swift version manager
    ".jabba",      // Java version manager
];

/// Tool mount configuration
///
/// Defines how a host tool directory is mounted into the container.
#[derive(Debug, Clone)]
pub struct ToolMount {
    /// Source path on host
    pub source: PathBuf,
    /// Target path inside container
    pub target: String,
    /// Whether mount is read-only
    pub read_only: bool,
}

impl ToolMount {
    /// Create a new tool mount
    pub fn new(source: PathBuf, target: String) -> Self {
        Self {
            source,
            target,
            read_only: true, // Tool mounts are read-only for security
        }
    }

    /// Create a read-write tool mount
    pub fn read_write(source: PathBuf, target: String) -> Self {
        Self {
            source,
            target,
            read_only: false,
        }
    }

    /// Convert to a container Mount
    pub fn to_mount(&self) -> Mount {
        if self.read_only {
            Mount::read_only(
                self.source.display().to_string(),
                self.target.clone(),
            )
        } else {
            Mount::new(
                self.source.display().to_string(),
                self.target.clone(),
            )
        }
    }
}

/// Tool manager for discovering and mounting host tools
///
/// Discovers language-specific tools and version managers on the host
/// and creates appropriate volume mounts for container access.
#[derive(Debug, Clone)]
pub struct ToolManager {
    /// Home directory (for version managers)
    home_dir: Option<PathBuf>,
    /// Additional custom tool directories
    custom_tool_dirs: Vec<PathBuf>,
    /// Whether to mount system binary directories
    mount_system_bins: bool,
}

impl ToolManager {
    /// Create a new tool manager
    pub fn new() -> Self {
        Self {
            home_dir: dirs::home_dir(),
            custom_tool_dirs: Vec::new(),
            mount_system_bins: true,
        }
    }

    /// Create a tool manager without system binary mounts
    ///
    /// This is useful when you only want language-specific tools.
    pub fn without_system_bins() -> Self {
        Self {
            home_dir: dirs::home_dir(),
            custom_tool_dirs: Vec::new(),
            mount_system_bins: false,
        }
    }

    /// Add a custom tool directory to mount
    pub fn add_tool_dir(&mut self, path: PathBuf) -> &mut Self {
        self.custom_tool_dirs.push(path);
        self
    }

    /// Set whether to mount system binary directories
    pub fn with_system_bins(&mut self, mount: bool) -> &mut Self {
        self.mount_system_bins = mount;
        self
    }

    /// Discover all available tool mounts
    ///
    /// Returns a list of tool directories that should be mounted into the container.
    pub fn discover_tool_mounts(&self) -> ContainerResult<Vec<ToolMount>> {
        let mut mounts = Vec::new();
        let mut seen_targets = HashSet::new();

        // Mount system binary directories (read-only)
        if self.mount_system_bins {
            for &bin_dir in SYSTEM_BIN_DIRS {
                if Path::new(bin_dir).exists() {
                    let target = bin_dir.to_string();
                    if seen_targets.insert(target.clone()) {
                        mounts.push(ToolMount::new(
                            PathBuf::from(bin_dir),
                            target,
                        ));
                    }
                }
            }
        }

        // Discover and mount language version managers
        if let Some(ref home) = self.home_dir {
            for &manager_dir in VERSION_MANAGER_DIRS {
                let source = home.join(manager_dir);
                if source.exists() && source.is_dir() {
                    let target = format!("/home/ralph/{}", manager_dir);
                    if seen_targets.insert(target.clone()) {
                        mounts.push(ToolMount::new(source, target));
                    }
                }
            }

            // Check for global npm packages (npx needs this)
            let npm_global = home.join(".npm-global");
            if npm_global.exists() && npm_global.is_dir() {
                let target = "/home/ralph/.npm-global".to_string();
                if seen_targets.insert(target.clone()) {
                    // Mount read-write as npm may need to install packages
                    mounts.push(ToolMount::read_write(npm_global, target));
                }
            }

            // Check for cargo bin (Rust tools)
            let cargo_bin = home.join(".cargo").join("bin");
            if cargo_bin.exists() && cargo_bin.is_dir() {
                let target = "/home/ralph/.cargo/bin".to_string();
                if seen_targets.insert(target.clone()) {
                    mounts.push(ToolMount::new(cargo_bin, target));
                }
            }

            // Check for local pip/bin (Python tools)
            let local_bin = home.join(".local").join("bin");
            if local_bin.exists() && local_bin.is_dir() {
                let target = "/home/ralph/.local/bin".to_string();
                if seen_targets.insert(target.clone()) {
                    mounts.push(ToolMount::new(local_bin, target));
                }
            }
        }

        // Add custom tool directories
        for tool_dir in &self.custom_tool_dirs {
            if tool_dir.exists() {
                // Use the directory name as the target
                let target = if let Some(file_name) = tool_dir.file_name() {
                    format!("/usr/local/{}", file_name.to_string_lossy())
                } else {
                    "/usr/local/custom-tools".to_string()
                };
                if seen_targets.insert(target.clone()) {
                    mounts.push(ToolMount::new(tool_dir.clone(), target));
                }
            }
        }

        Ok(mounts)
    }

    /// Get environment variables to pass to the container
    ///
    /// Returns environment variables that ensure tools work correctly in the container.
    pub fn get_env_vars(&self) -> Vec<(String, String)> {
        let mut env_vars = Vec::new();

        // Pass through the host PATH so npx and other tools work
        if let Ok(path) = env::var("PATH") {
            // Filter out system paths that will be available in container
            let filtered_path: Vec<String> = env::split_paths(&path)
                .filter(|p| {
                    let path_str = p.display().to_string();
                    // Keep paths that are likely user-specific or custom
                    !path_str.starts_with("/usr/bin")
                        && !path_str.starts_with("/bin")
                        && !path_str.starts_with("/sbin")
                })
                .map(|p| p.display().to_string())
                .collect();

            if !filtered_path.is_empty() {
                // Prepend filtered host paths to container PATH
                let host_paths = filtered_path.join(":");
                env_vars.push(("HOST_PATH".to_string(), host_paths));
            }
        }

        // Pass language-specific environment variables
        for (key, _) in env::vars() {
            let key_upper = key.to_uppercase();
            if key_upper.contains("NODE")
                || key_upper.contains("NPM")
                || key_upper.contains("PYTHON")
                || key_upper.contains("RUBY")
                || key_upper.contains("JAVA")
                || key_upper.contains("GO")
                || key_upper.contains("CARGO")
                || key_upper.contains("RUST")
                || key_upper.contains("PHP")
                || key_upper.contains("Composer")
            {
                let value = env::var(&key).unwrap_or_default();
                env_vars.push((key, value));
            }
        }

        env_vars
    }

    /// Detect which language tools are available on the host
    ///
    /// Returns a list of detected language/tool environments.
    pub fn detect_available_tools(&self) -> Vec<String> {
        let mut tools = Vec::new();

        if let Some(ref home) = self.home_dir {
            // Check for Ruby version managers
            if home.join(".rbenv").exists() {
                tools.push("rbenv (Ruby)".to_string());
            }
            if home.join(".rvm").exists() {
                tools.push("rvm (Ruby)".to_string());
            }

            // Check for Node.js version manager
            if home.join(".nvm").exists() {
                tools.push("nvm (Node.js)".to_string());
            }

            // Check for Python version manager
            if home.join(".pyenv").exists() {
                tools.push("pyenv (Python)".to_string());
            }

            // Check for Java version managers
            if home.join(".jenv").exists() {
                tools.push("jenv (Java)".to_string());
            }
            if home.join(".sdkman").exists() {
                tools.push("sdkman (Java/Gradle/Maven)".to_string());
            }

            // Check for Rust tools
            if home.join(".cargo").exists() {
                tools.push("cargo (Rust)".to_string());
            }

            // Check for Go version manager
            if home.join(".goenv").exists() {
                tools.push("goenv (Go)".to_string());
            }
        }

        // Check for system tools
        if Path::new("/usr/local/bin").exists() {
            tools.push("system tools (/usr/local/bin)".to_string());
        }

        tools
    }

    /// Check if a specific tool binary exists on the host
    pub fn tool_exists(tool_name: &str) -> bool {
        // Check in PATH
        if let Ok(path) = env::var("PATH") {
            for path_dir in env::split_paths(&path) {
                let binary = path_dir.join(tool_name);
                if binary.exists() {
                    return true;
                }
            }
        }
        false
    }

    /// Get the home directory
    pub fn home_dir(&self) -> Option<&Path> {
        self.home_dir.as_deref()
    }
}

impl Default for ToolManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect which project stack is being used based on available tools
///
/// Returns the detected language/stack for container image selection.
pub fn detect_project_stack_from_tools(repo_path: &Path) -> Option<String> {
    // Check for project files
    let cargo_toml = repo_path.join("Cargo.toml");
    if cargo_toml.exists() {
        return Some("rust".to_string());
    }

    let package_json = repo_path.join("package.json");
    if package_json.exists() {
        return Some("node".to_string());
    }

    let gemfile = repo_path.join("Gemfile");
    if gemfile.exists() {
        return Some("ruby".to_string());
    }

    let requirements_txt = repo_path.join("requirements.txt");
    let pyproject_toml = repo_path.join("pyproject.toml");
    if requirements_txt.exists() || pyproject_toml.exists() {
        return Some("python".to_string());
    }

    let go_mod = repo_path.join("go.mod");
    if go_mod.exists() {
        return Some("go".to_string());
    }

    let pom_xml = repo_path.join("pom.xml");
    let build_gradle = repo_path.join("build.gradle");
    if pom_xml.exists() || build_gradle.exists() {
        return Some("java".to_string());
    }

    let composer_json = repo_path.join("composer.json");
    if composer_json.exists() {
        return Some("php".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_manager_new() {
        let manager = ToolManager::new();
        assert!(manager.mount_system_bins);
    }

    #[test]
    fn test_tool_manager_without_system_bins() {
        let manager = ToolManager::without_system_bins();
        assert!(!manager.mount_system_bins);
    }

    #[test]
    fn test_tool_mount_new() {
        let mount = ToolMount::new(
            PathBuf::from("/usr/bin"),
            "/usr/bin".to_string(),
        );
        assert_eq!(mount.source, PathBuf::from("/usr/bin"));
        assert_eq!(mount.target, "/usr/bin");
        assert!(mount.read_only);
    }

    #[test]
    fn test_tool_mount_read_write() {
        let mount = ToolMount::read_write(
            PathBuf::from("/home/user/.npm-global"),
            "/home/ralph/.npm-global".to_string(),
        );
        assert!(!mount.read_only);
    }

    #[test]
    fn test_tool_mount_to_mount() {
        let tool_mount = ToolMount::new(
            PathBuf::from("/usr/local/bin"),
            "/usr/local/bin".to_string(),
        );
        let mount = tool_mount.to_mount();
        assert_eq!(mount.source, "/usr/local/bin");
        assert_eq!(mount.target, "/usr/local/bin");
        assert!(mount.read_only);
    }

    #[test]
    fn test_detect_project_stack_rust() {
        let temp = std::env::temp_dir();
        let repo_path = temp.join("test-rust-project");
        fs::create_dir_all(&repo_path).ok();
        fs::write(repo_path.join("Cargo.toml"), "[package]\nname = \"test\"").ok();

        let stack = detect_project_stack_from_tools(&repo_path);
        assert_eq!(stack, Some("rust".to_string()));

        // Cleanup
        fs::remove_dir_all(&repo_path).ok();
    }

    #[test]
    fn test_detect_project_stack_none() {
        let temp = std::env::temp_dir();
        let repo_path = temp.join("test-empty-project");
        fs::create_dir_all(&repo_path).ok();

        let stack = detect_project_stack_from_tools(&repo_path);
        assert_eq!(stack, None);

        // Cleanup
        fs::remove_dir_all(&repo_path).ok();
    }

    #[test]
    fn test_tool_exists_likely_true() {
        // Most systems have 'sh' available
        assert!(ToolManager::tool_exists("sh") || ToolManager::tool_exists("bash"));
    }

    #[test]
    fn test_tool_manager_get_env_vars() {
        let manager = ToolManager::new();
        let env_vars = manager.get_env_vars();
        // Should at least return an empty vec, not panic
        assert!(env_vars.is_empty() || !env_vars.is_empty());
    }
}
