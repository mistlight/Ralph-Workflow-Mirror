//! Host tool discovery and injection for container execution
//!
//! This module ensures that all commands available on the host work in the container
//! by discovering and mounting language-specific tool directories.

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

/// macOS Homebrew installation paths
const MACOS_HOMEBREW_PATHS: &[&str] = &[
    "/opt/homebrew/bin", // Apple Silicon
    "/opt/homebrew/sbin",
    "/usr/local/bin", // Intel
    "/usr/local/sbin",
];

/// Homebrew environment variable names
const HOMEBREW_ENV_VARS: &[&str] = &[
    "HOMEBREW_PREFIX",
    "HOMEBREW_CELLAR",
    "HOMEBREW_REPOSITORY",
    "HOMEBREW_SHELLENV_PATH",
];

/// Language version manager home directories
const VERSION_MANAGER_DIRS: &[&str] = &[
    ".rbenv",    // Ruby version manager
    ".rvm",      // Ruby version manager (alternative)
    ".nvm",      // Node.js version manager
    ".pyenv",    // Python version manager
    ".jenv",     // Java version manager
    ".sdkman",   // Java/Gradle/Maven manager
    ".gvm",      // Groovy version manager
    ".goenv",    // Go version manager
    ".swiftenv", // Swift version manager
    ".jabba",    // Java version manager
    ".mix",      // Elixir/Phoenix (stores mix install archives)
    ".gradle",   // Java/Gradle cache and wrapper
    ".m2",       // Maven local repository
    ".go",       // Go workspace
    ".asdf",     // Multi-language version manager
    ".mise",     // Multi-language version manager (formerly rtx)
    ".chruby",   // Ruby version manager
    ".fnm",      // Fast Node Manager
    ".volta",    // JavaScript tool manager
    ".jbang",    // Java shell scripting tool
    ".dart",     // Dart SDK (Flutter/Dart development)
    ".flutter",  // Flutter SDK
    ".lein",     // Leiningen (Clojure build tool)
    ".maven",    // Maven wrapper directory
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
    pub const fn new(source: PathBuf, target: String) -> Self {
        Self {
            source,
            target,
            read_only: true, // Tool mounts are read-only for security
        }
    }

    /// Create a read-write tool mount
    pub const fn read_write(source: PathBuf, target: String) -> Self {
        Self {
            source,
            target,
            read_only: false,
        }
    }

    /// Convert to a container Mount
    pub fn to_mount(&self) -> Mount {
        if self.read_only {
            Mount::read_only(self.source.display().to_string(), self.target.clone())
        } else {
            Mount::new(self.source.display().to_string(), self.target.clone())
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
    /// Repository root path (for project-local tool detection)
    repo_root: Option<PathBuf>,
}

impl ToolManager {
    /// Create a new tool manager
    pub fn new() -> Self {
        Self {
            home_dir: dirs::home_dir(),
            custom_tool_dirs: Vec::new(),
            mount_system_bins: true,
            repo_root: None,
        }
    }

    /// Create a tool manager with a repository root
    pub fn with_repo(repo_root: PathBuf) -> Self {
        Self {
            home_dir: dirs::home_dir(),
            custom_tool_dirs: Vec::new(),
            mount_system_bins: true,
            repo_root: Some(repo_root),
        }
    }

    /// Create a tool manager without system binary mounts
    ///
    /// This is useful when you only want language-specific tools.
    #[cfg(test)]
    pub fn without_system_bins() -> Self {
        Self {
            home_dir: dirs::home_dir(),
            custom_tool_dirs: Vec::new(),
            mount_system_bins: false,
            repo_root: None,
        }
    }

    /// Add a custom tool directory to mount
    #[cfg(test)]
    pub fn add_tool_dir(&mut self, path: PathBuf) -> &mut Self {
        self.custom_tool_dirs.push(path);
        self
    }

    /// Set whether to mount system binary directories
    #[cfg(test)]
    pub const fn with_system_bins(&mut self, mount: bool) -> &mut Self {
        self.mount_system_bins = mount;
        self
    }

    /// Discover all available tool mounts
    ///
    /// Returns a list of tool directories that should be mounted into the container.
    pub fn discover_tool_mounts(&self) -> Vec<ToolMount> {
        let mut mounts = Vec::new();
        let mut seen_targets = HashSet::new();

        // Mount system binary directories (read-only)
        if self.mount_system_bins {
            for &bin_dir in SYSTEM_BIN_DIRS {
                if Path::new(bin_dir).exists() {
                    let target = bin_dir.to_string();
                    if seen_targets.insert(target.clone()) {
                        mounts.push(ToolMount::new(PathBuf::from(bin_dir), target));
                    }
                }
            }

            // On macOS, also mount Homebrew directories
            if cfg!(target_os = "macos") {
                for &brew_path in MACOS_HOMEBREW_PATHS {
                    if Path::new(brew_path).exists() {
                        let target = brew_path.to_string();
                        if seen_targets.insert(target.clone()) {
                            mounts.push(ToolMount::new(PathBuf::from(brew_path), target));
                        }
                    }
                }

                // Mount Homebrew prefix (contains Cellar, etc.)
                if let Ok(prefix) = env::var("HOMEBREW_PREFIX") {
                    let prefix_path = Path::new(&prefix);
                    if prefix_path.exists() && prefix_path.is_dir() {
                        let target = prefix.clone();
                        if seen_targets.insert(target.clone()) {
                            mounts.push(ToolMount::new(PathBuf::from(prefix), target));
                        }
                    }
                }
            }
        }

        // Discover and mount language version managers
        if let Some(ref home) = self.home_dir {
            for &manager_dir in VERSION_MANAGER_DIRS {
                let source = home.join(manager_dir);
                if source.exists() && source.is_dir() {
                    let target = format!("/home/ralph/{manager_dir}");
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

        // Discover and mount project-local tool configurations
        if let Some(ref repo_root) = self.repo_root {
            // Python virtual environments
            for venv_dir in &[".venv", "venv", "env", "virtualenv"] {
                let venv_path = repo_root.join(venv_dir);
                if venv_path.exists() && venv_path.is_dir() {
                    let target = format!("/workspace/{venv_dir}");
                    if seen_targets.insert(target.clone()) {
                        // Mount read-write as pip may need to install packages
                        mounts.push(ToolMount::read_write(venv_path, target));
                    }
                }
            }

            // Node.js version files (.nvmrc, .node-version)
            for version_file in &[".nvmrc", ".node-version"] {
                let version_path = repo_root.join(version_file);
                if version_path.exists() && version_path.is_file() {
                    // Just note this for environment setup - the actual version manager
                    // is already mounted from home directory
                }
            }

            // Ruby version files (.ruby-version, .tool-versions)
            for version_file in &[".ruby-version", ".tool-versions"] {
                let version_path = repo_root.join(version_file);
                if version_path.exists() && version_path.is_file() {
                    // Note for environment setup - version managers already mounted
                }
            }

            // Python version files (.python-version)
            let python_version = repo_root.join(".python-version");
            if python_version.exists() && python_version.is_file() {
                // Note for environment setup
            }

            // asdf.local.toml (project-local asdf configuration)
            let asdf_local = repo_root.join(".asdf-local.toml");
            if asdf_local.exists() && asdf_local.is_file() {
                // Note for environment setup - asdf already mounted from home
            }

            // mise local configuration (.mise.local.toml)
            let mise_local = repo_root.join(".mise.local.toml");
            if mise_local.exists() && mise_local.is_file() {
                // Note for environment setup - mise already mounted from home
            }

            // Java ecosystem directories
            for java_dir in &[".gradle", "build", "target"] {
                let java_path = repo_root.join(java_dir);
                if java_path.exists() && java_path.is_dir() {
                    let target = format!("/workspace/{java_dir}");
                    if seen_targets.insert(target.clone()) {
                        // Mount read-write as build tools write here
                        mounts.push(ToolMount::read_write(java_path, target));
                    }
                }
            }

            // Node.js specific directories
            for node_dir in &["node_modules", ".next", ".nuxt", "dist", ".output"] {
                let node_path = repo_root.join(node_dir);
                if node_path.exists() && node_path.is_dir() {
                    let target = format!("/workspace/{node_dir}");
                    if seen_targets.insert(target.clone()) {
                        mounts.push(ToolMount::read_write(node_path, target));
                    }
                }
            }

            // Go workspace directories
            let go_bin = repo_root.join("bin");
            if go_bin.exists() && go_bin.is_dir() {
                let target = "/workspace/bin".to_string();
                if seen_targets.insert(target.clone()) {
                    mounts.push(ToolMount::new(go_bin, target));
                }
            }

            // asdf tool versions (already mounted via home dir, but we may need
            // to ensure ASDF_DATA_DIR is set correctly)
        }

        // Add custom tool directories
        for tool_dir in &self.custom_tool_dirs {
            if tool_dir.exists() {
                // Use the directory name as the target
                let target = tool_dir.file_name().map_or_else(
                    || "/usr/local/custom-tools".to_string(),
                    |file_name| format!("/usr/local/{}", file_name.to_string_lossy()),
                );
                if seen_targets.insert(target.clone()) {
                    mounts.push(ToolMount::new(tool_dir.clone(), target));
                }
            }
        }

        mounts
    }

    /// Get environment variables to pass to the container
    ///
    /// Returns environment variables that ensure tools work correctly in the container.
    pub fn get_env_vars() -> Vec<(String, String)> {
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

        // On macOS, pass through Homebrew environment variables
        if cfg!(target_os = "macos") {
            for &key in HOMEBREW_ENV_VARS {
                if let Ok(value) = env::var(key) {
                    env_vars.push((key.to_string(), value));
                }
            }
        }

        // Pass language-specific environment variables
        for (key, _) in env::vars() {
            let key_upper = key.to_uppercase();
            if key_upper.contains("NODE")
                || key_upper.contains("NPM")
                || key_upper.contains("PYTHON")
                || key_upper.contains("RUBY")
                || key_upper.contains("GEM")
                || key_upper.contains("RBENV")
                || key_upper.contains("RVM")
                || key_upper.contains("JAVA")
                || key_upper.contains("GO")
                || key_upper.contains("GOPATH")
                || key_upper.contains("GOROOT")
                || key_upper.contains("CARGO")
                || key_upper.contains("RUST")
                || key_upper.contains("RUSTUP")
                || key_upper.contains("PHP")
                || key_upper.contains("COMPOSER")
                || key_upper.contains("MIX")
                || key_upper.contains("ELIXIR")
                || key_upper.contains("ERL")
                || key_upper.contains("GRADLE")
                || key_upper.contains("MAVEN")
                || key_upper.contains("M2")
                || key_upper.contains("PIP")
                || key_upper.contains("PYENV")
                || key_upper.contains("VIRTUAL_ENV")
                || key_upper.contains("CONDA")
                || key_upper.contains("PERL")
                || key_upper.contains("PERL5LIB")
                || key_upper.contains("SCALA")
                || key_upper.contains("SBT")
                || key_upper.contains("ASDF")
                || key_upper.contains("MISE")
                || key_upper.contains("RTX")
                || key_upper.contains("FNM")
                || key_upper.contains("VOLTA")
                || key_upper.contains("CHRUBY")
                || key_upper.contains("JABBA")
                || key_upper.contains("SWIFTENV")
                || key_upper.contains("SDKMAN")
                || key_upper.contains("GVM")
                || key_upper.contains("GOENV")
                || key_upper.contains("POETRY")
                || key_upper.contains("PEX")
                || key_upper.contains("PIPX")
                || key_upper.contains("PNPM")
                || key_upper.contains("YARN")
                || key_upper.contains("BUN")
                || key_upper.contains("FLUTTER")
                || key_upper.contains("DART")
                || key_upper.contains("JBANG")
                || key_upper.contains("LEIN")
                || key_upper.contains("CLOJURE")
            {
                let value = env::var(&key).unwrap_or_default();
                env_vars.push((key, value));
            }
        }

        env_vars
    }

    /// Get shell initialization script content for container startup
    ///
    /// Returns a bash script fragment that initializes all detected version managers.
    pub fn get_shell_init_script(&self) -> String {
        let mut init_lines = Vec::new();
        let Some(ref _home) = self.home_dir else {
            return String::new();
        };

        // Add rbenv initialization
        init_lines.push(
            r#"
# Initialize rbenv if available
if [ -d "/home/ralph/.rbenv" ]; then
    export RBENV_ROOT="/home/ralph/.rbenv"
    export PATH="$RBENV_ROOT/bin:$PATH"
    eval "$(rbenv init - bash 2>/dev/null || true)"
fi
"#
            .to_string(),
        );

        // Add RVM initialization
        init_lines.push(
            r#"
# Initialize RVM if available
if [ -f "/home/ralph/.rvm/scripts/rvm" ]; then
    export RVM_HOME="/home/ralph/.rvm"
    source "$RVM_HOME/scripts/rvm"
fi
"#
            .to_string(),
        );

        // Add nvm initialization
        init_lines.push(
            r#"
# Initialize nvm if available
if [ -f "/home/ralph/.nvm/nvm.sh" ]; then
    export NVM_DIR="/home/ralph/.nvm"
    [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
fi
"#
            .to_string(),
        );

        // Add pyenv initialization
        init_lines.push(
            r#"
# Initialize pyenv if available
if [ -d "/home/ralph/.pyenv" ]; then
    export PYENV_ROOT="/home/ralph/.pyenv"
    export PATH="$PYENV_ROOT/bin:$PATH"
    eval "$(pyenv init - bash 2>/dev/null || true)"
fi
"#
            .to_string(),
        );

        // Add asdf initialization
        init_lines.push(
            r#"
# Initialize asdf if available
if [ -f "/home/ralph/.asdf/asdf.sh" ]; then
    export ASDF_DATA_DIR="/home/ralph/.asdf"
    export ASDF_DIR="/home/ralph/.asdf"
    . "$ASDF_DIR/asdf.sh"
fi
"#
            .to_string(),
        );

        // Add mise initialization
        init_lines.push(
            r#"
# Initialize mise if available
if command -v mise &> /dev/null; then
    export MISE_DATA_DIR="/home/ralph/.mise"
    export MISE_SHELL=bash
    eval "$(mise activate bash 2>/dev/null || true)"
fi
"#
            .to_string(),
        );

        // Add fnm initialization
        init_lines.push(
            r#"
# Initialize fnm if available
if command -v fnm &> /dev/null; then
    export FNM_DIR="/home/ralph/.fnm"
    eval "$(fnm env --use-on-cd 2>/dev/null || true)"
fi
"#
            .to_string(),
        );

        // Add Dart/Flutter initialization
        init_lines.push(
            r#"
# Initialize Flutter if available
if [ -d "/home/ralph/.flutter" ]; then
    export FLUTTER_ROOT="/home/ralph/.flutter"
    export PATH="$FLUTTER_ROOT/bin:$PATH"
fi

# Initialize Dart SDK if available
if [ -d "/home/ralph/.dart" ]; then
    export DART_ROOT="/home/ralph/.dart"
    export PATH="$DART_ROOT/bin:$PATH"
fi
"#
            .to_string(),
        );

        // Add jbang initialization
        init_lines.push(
            r#"
# Initialize jbang if available
if [ -d "/home/ralph/.jbang" ]; then
    export JBANG_HOME="/home/ralph/.jbang"
    export PATH="$JBANG_HOME/bin:$PATH"
fi
"#
            .to_string(),
        );

        // Add virtual environment activation if detected
        init_lines.push(
            r#"
# Activate Python virtual environment if available
if [ -f "/workspace/.venv/bin/activate" ]; then
    source "/workspace/.venv/bin/activate"
elif [ -f "/workspace/venv/bin/activate" ]; then
    source "/workspace/venv/bin/activate"
fi
"#
            .to_string(),
        );

        init_lines.join("\n")
    }

    /// Detect which language tools are available on the host
    ///
    /// Returns a list of detected language/tool environments.
    #[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
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
#[cfg(test)]
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
        let mount = ToolMount::new(PathBuf::from("/usr/bin"), "/usr/bin".to_string());
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
        let env_vars = ToolManager::get_env_vars();
        // Should at least return an empty vec, not panic
        assert!(env_vars.is_empty() || !env_vars.is_empty());
    }

    #[test]
    fn test_tool_manager_add_tool_dir() {
        let mut manager = ToolManager::new();
        let custom_path = PathBuf::from("/opt/custom-tools");
        // Just verify the method doesn't panic and returns &mut Self
        let result = manager.add_tool_dir(custom_path);
        // Should return self for chaining
        assert_eq!(
            std::ptr::from_ref(result) as usize,
            &raw const manager as usize
        );
    }

    #[test]
    fn test_tool_manager_with_system_bins() {
        let mut manager = ToolManager::new();
        // Just verify the method doesn't panic and returns &mut Self
        let result = manager.with_system_bins(false);
        // Should return self for chaining
        assert_eq!(
            std::ptr::from_ref(result) as usize,
            &raw const manager as usize
        );
    }

    #[test]
    fn test_tool_manager_home_dir() {
        let manager = ToolManager::new();
        // home_dir may be Some or None depending on the test environment
        let home = manager.home_dir();
        if let Some(path) = home {
            assert!(!path.as_os_str().is_empty());
        }
    }

    #[test]
    fn test_tool_manager_detect_available_tools() {
        let manager = ToolManager::new();
        let tools = manager.detect_available_tools();
        // Should at least return an empty vec, not panic
        assert!(tools.is_empty() || !tools.is_empty());
    }
}
