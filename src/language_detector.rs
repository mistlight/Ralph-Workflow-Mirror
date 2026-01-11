//! Language and Stack Detection Module
//!
//! Detects the primary technology stack of a repository by analyzing file extensions,
//! configuration files, and common patterns. This enables language-specific review
//! guidance without requiring an LLM.
//!
//! The detection is fast (< 100ms typically) and uses heuristics based on:
//! - File extension counts
//! - Signature files (Cargo.toml, package.json, etc.)
//! - Framework indicators in config files

#![deny(unsafe_code)]

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

/// Maximum number of files to scan (for performance on large repos)
const MAX_FILES_TO_SCAN: usize = 2000;

/// Minimum file count to consider a language as present
const MIN_FILES_FOR_DETECTION: usize = 1;

/// Represents the detected technology stack of a project
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectStack {
    /// Primary programming language (most prevalent)
    pub(crate) primary_language: String,
    /// Secondary languages used in the project
    pub(crate) secondary_languages: Vec<String>,
    /// Detected frameworks (React, Django, Rails, etc.)
    pub(crate) frameworks: Vec<String>,
    /// Whether the project appears to have tests
    pub(crate) has_tests: bool,
    /// Detected test framework (if any)
    pub(crate) test_framework: Option<String>,
    /// Package manager detected
    pub(crate) package_manager: Option<String>,
}

impl Default for ProjectStack {
    fn default() -> Self {
        Self {
            primary_language: "Unknown".to_string(),
            secondary_languages: Vec::new(),
            frameworks: Vec::new(),
            has_tests: false,
            test_framework: None,
            package_manager: None,
        }
    }
}

impl ProjectStack {
    /// Returns true if the project uses Rust
    pub(crate) fn is_rust(&self) -> bool {
        self.primary_language == "Rust" || self.secondary_languages.iter().any(|l| l == "Rust")
    }

    /// Returns true if the project uses Python
    pub(crate) fn is_python(&self) -> bool {
        self.primary_language == "Python" || self.secondary_languages.iter().any(|l| l == "Python")
    }

    /// Returns true if the project uses JavaScript or TypeScript
    pub(crate) fn is_javascript_or_typescript(&self) -> bool {
        matches!(self.primary_language.as_str(), "JavaScript" | "TypeScript")
            || self
                .secondary_languages
                .iter()
                .any(|l| l == "JavaScript" || l == "TypeScript")
    }

    /// Returns true if the project uses Go
    pub(crate) fn is_go(&self) -> bool {
        self.primary_language == "Go" || self.secondary_languages.iter().any(|l| l == "Go")
    }

    /// Format as a summary string for display
    pub(crate) fn summary(&self) -> String {
        let mut parts = vec![self.primary_language.clone()];

        if !self.secondary_languages.is_empty() {
            parts.push(format!("(+{})", self.secondary_languages.join(", ")));
        }

        if !self.frameworks.is_empty() {
            parts.push(format!("[{}]", self.frameworks.join(", ")));
        }

        if self.has_tests {
            if let Some(ref tf) = self.test_framework {
                parts.push(format!("tests:{}", tf));
            } else {
                parts.push("tests:yes".to_string());
            }
        }

        parts.join(" ")
    }
}

/// Mapping from file extensions to language names
fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        // Rust
        "rs" => Some("Rust"),
        // Python
        "py" | "pyw" | "pyi" => Some("Python"),
        // JavaScript/TypeScript
        "js" | "mjs" | "cjs" => Some("JavaScript"),
        "ts" | "mts" | "cts" => Some("TypeScript"),
        "jsx" => Some("JavaScript"),
        "tsx" => Some("TypeScript"),
        // Go
        "go" => Some("Go"),
        // Java
        "java" => Some("Java"),
        // Kotlin
        "kt" | "kts" => Some("Kotlin"),
        // C/C++
        "c" | "h" => Some("C"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some("C++"),
        // C#
        "cs" => Some("C#"),
        // Ruby
        "rb" | "erb" => Some("Ruby"),
        // PHP
        "php" => Some("PHP"),
        // Swift
        "swift" => Some("Swift"),
        // Scala
        "scala" | "sc" => Some("Scala"),
        // Shell
        "sh" | "bash" | "zsh" => Some("Shell"),
        // SQL
        "sql" => Some("SQL"),
        // Lua
        "lua" => Some("Lua"),
        // Perl
        "pl" | "pm" => Some("Perl"),
        // R
        "r" => Some("R"),
        // Dart
        "dart" => Some("Dart"),
        // Elixir
        "ex" | "exs" => Some("Elixir"),
        // Haskell
        "hs" | "lhs" => Some("Haskell"),
        // OCaml
        "ml" | "mli" => Some("OCaml"),
        // F#
        "fs" | "fsi" | "fsx" => Some("F#"),
        // Clojure
        "clj" | "cljs" | "cljc" | "edn" => Some("Clojure"),
        // Zig
        "zig" => Some("Zig"),
        // Nim
        "nim" => Some("Nim"),
        // V
        "v" => Some("V"),
        _ => None,
    }
}

/// Detect signature files that indicate specific frameworks or package managers
fn detect_signature_files(root: &Path) -> (Vec<String>, Option<String>, Option<String>) {
    let mut frameworks = Vec::new();
    let mut test_framework = None;
    let mut package_manager = None;

    // Rust
    if root.join("Cargo.toml").exists() {
        package_manager = Some("Cargo".to_string());

        // Check Cargo.toml for test dependencies
        if let Ok(content) = fs::read_to_string(root.join("Cargo.toml")) {
            if content.contains("[dev-dependencies]") || content.contains("[[test]]") {
                test_framework = Some("cargo test".to_string());
            }
            // Check for common Rust frameworks
            if content.contains("actix") {
                frameworks.push("Actix".to_string());
            }
            if content.contains("axum") {
                frameworks.push("Axum".to_string());
            }
            if content.contains("rocket") {
                frameworks.push("Rocket".to_string());
            }
            if content.contains("tokio") {
                frameworks.push("Tokio".to_string());
            }
            if content.contains("warp") {
                frameworks.push("Warp".to_string());
            }
            if content.contains("tauri") {
                frameworks.push("Tauri".to_string());
            }
            if content.contains("leptos") {
                frameworks.push("Leptos".to_string());
            }
            if content.contains("yew") {
                frameworks.push("Yew".to_string());
            }
        }
    }

    // Python
    if root.join("pyproject.toml").exists() {
        package_manager = Some("Poetry/pip".to_string());
        if let Ok(content) = fs::read_to_string(root.join("pyproject.toml")) {
            if content.contains("pytest") {
                test_framework = Some("pytest".to_string());
            }
            if content.contains("django") {
                frameworks.push("Django".to_string());
            }
            if content.contains("fastapi") {
                frameworks.push("FastAPI".to_string());
            }
            if content.contains("flask") {
                frameworks.push("Flask".to_string());
            }
        }
    } else if root.join("requirements.txt").exists() {
        package_manager = Some("pip".to_string());
        if let Ok(content) = fs::read_to_string(root.join("requirements.txt")) {
            if content.contains("pytest") {
                test_framework = Some("pytest".to_string());
            }
            if content.contains("django") || content.contains("Django") {
                frameworks.push("Django".to_string());
            }
            if content.contains("fastapi") {
                frameworks.push("FastAPI".to_string());
            }
            if content.contains("flask") || content.contains("Flask") {
                frameworks.push("Flask".to_string());
            }
        }
    } else if root.join("setup.py").exists() {
        package_manager = Some("setuptools".to_string());
    } else if root.join("Pipfile").exists() {
        package_manager = Some("Pipenv".to_string());
    }

    // JavaScript/TypeScript
    if root.join("package.json").exists() {
        package_manager = Some("npm/yarn".to_string());
        if let Ok(content) = fs::read_to_string(root.join("package.json")) {
            // Test frameworks
            if content.contains("\"jest\"") {
                test_framework = Some("Jest".to_string());
            } else if content.contains("\"vitest\"") {
                test_framework = Some("Vitest".to_string());
            } else if content.contains("\"mocha\"") {
                test_framework = Some("Mocha".to_string());
            }

            // Frameworks
            if content.contains("\"react\"") {
                frameworks.push("React".to_string());
            }
            if content.contains("\"vue\"") {
                frameworks.push("Vue".to_string());
            }
            if content.contains("\"svelte\"") {
                frameworks.push("Svelte".to_string());
            }
            if content.contains("\"angular\"") || content.contains("\"@angular") {
                frameworks.push("Angular".to_string());
            }
            if content.contains("\"next\"") {
                frameworks.push("Next.js".to_string());
            }
            if content.contains("\"nuxt\"") {
                frameworks.push("Nuxt".to_string());
            }
            if content.contains("\"express\"") {
                frameworks.push("Express".to_string());
            }
            if content.contains("\"fastify\"") {
                frameworks.push("Fastify".to_string());
            }
            if content.contains("\"nest\"") || content.contains("\"@nestjs") {
                frameworks.push("NestJS".to_string());
            }
            if content.contains("\"electron\"") {
                frameworks.push("Electron".to_string());
            }
        }
    }

    // Go
    if root.join("go.mod").exists() {
        package_manager = Some("Go modules".to_string());
        if let Ok(content) = fs::read_to_string(root.join("go.mod")) {
            if content.contains("gin-gonic/gin") {
                frameworks.push("Gin".to_string());
            }
            if content.contains("go-chi/chi") {
                frameworks.push("Chi".to_string());
            }
            if content.contains("gofiber/fiber") {
                frameworks.push("Fiber".to_string());
            }
            if content.contains("labstack/echo") {
                frameworks.push("Echo".to_string());
            }
        }
        // Go uses built-in testing
        test_framework = Some("go test".to_string());
    }

    // Ruby
    if root.join("Gemfile").exists() {
        package_manager = Some("Bundler".to_string());
        if let Ok(content) = fs::read_to_string(root.join("Gemfile")) {
            if content.contains("rspec") {
                test_framework = Some("RSpec".to_string());
            } else if content.contains("minitest") {
                test_framework = Some("Minitest".to_string());
            }
            if content.contains("rails") {
                frameworks.push("Rails".to_string());
            }
            if content.contains("sinatra") {
                frameworks.push("Sinatra".to_string());
            }
        }
    }

    // Java
    if root.join("pom.xml").exists() {
        package_manager = Some("Maven".to_string());
        if let Ok(content) = fs::read_to_string(root.join("pom.xml")) {
            if content.contains("junit") {
                test_framework = Some("JUnit".to_string());
            }
            if content.contains("spring") {
                frameworks.push("Spring".to_string());
            }
        }
    } else if root.join("build.gradle").exists() || root.join("build.gradle.kts").exists() {
        package_manager = Some("Gradle".to_string());
        let gradle_file = if root.join("build.gradle.kts").exists() {
            root.join("build.gradle.kts")
        } else {
            root.join("build.gradle")
        };
        if let Ok(content) = fs::read_to_string(gradle_file) {
            if content.contains("junit") {
                test_framework = Some("JUnit".to_string());
            }
            if content.contains("spring") {
                frameworks.push("Spring".to_string());
            }
        }
    }

    // PHP
    if root.join("composer.json").exists() {
        package_manager = Some("Composer".to_string());
        if let Ok(content) = fs::read_to_string(root.join("composer.json")) {
            if content.contains("phpunit") {
                test_framework = Some("PHPUnit".to_string());
            }
            if content.contains("laravel") {
                frameworks.push("Laravel".to_string());
            }
            if content.contains("symfony") {
                frameworks.push("Symfony".to_string());
            }
        }
    }

    // .NET / C#
    if root.join("*.csproj").exists()
        || root
            .read_dir()
            .map(|d| {
                d.flatten()
                    .any(|e| e.path().extension() == Some("csproj".as_ref()))
            })
            .unwrap_or(false)
    {
        package_manager = Some("NuGet".to_string());
    }

    // Elixir
    if root.join("mix.exs").exists() {
        package_manager = Some("Mix".to_string());
        test_framework = Some("ExUnit".to_string());
        if let Ok(content) = fs::read_to_string(root.join("mix.exs")) {
            if content.contains(":phoenix") {
                frameworks.push("Phoenix".to_string());
            }
        }
    }

    // Dart/Flutter
    if root.join("pubspec.yaml").exists() {
        package_manager = Some("pub".to_string());
        if let Ok(content) = fs::read_to_string(root.join("pubspec.yaml")) {
            if content.contains("flutter") {
                frameworks.push("Flutter".to_string());
            }
            if content.contains("test:") {
                test_framework = Some("dart test".to_string());
            }
        }
    }

    (frameworks, test_framework, package_manager)
}

/// Detect if tests exist in common test directories
fn detect_tests(root: &Path, primary_lang: &str) -> bool {
    let test_patterns = match primary_lang {
        "Rust" => vec!["tests/", "src/*/tests.rs"],
        "Python" => vec!["tests/", "test/", "test_*.py", "*_test.py"],
        "JavaScript" | "TypeScript" => {
            vec!["__tests__/", "test/", "tests/", "*.test.js", "*.spec.js"]
        }
        "Go" => vec!["*_test.go"],
        "Java" => vec!["src/test/", "test/"],
        "Ruby" => vec!["spec/", "test/"],
        _ => vec!["tests/", "test/", "spec/"],
    };

    for pattern in test_patterns {
        if pattern.ends_with('/') {
            // Directory check
            if root.join(pattern.trim_end_matches('/')).is_dir() {
                return true;
            }
        } else if pattern.contains('*') {
            // Glob pattern - simplified check
            let parts: Vec<&str> = pattern.split('*').collect();
            if let Ok(entries) = fs::read_dir(root) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if parts.len() == 2 {
                        let (prefix, suffix) = (parts[0], parts[1]);
                        if name_str.starts_with(prefix) && name_str.ends_with(suffix) {
                            return true;
                        }
                    }
                }
            }
        } else if root.join(pattern).exists() {
            return true;
        }
    }

    false
}

/// Scan directory recursively and count file extensions
fn count_extensions(root: &Path) -> io::Result<HashMap<String, usize>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut files_scanned = 0;

    // Use a simple recursive approach with early termination
    fn scan_dir(
        dir: &Path,
        counts: &mut HashMap<String, usize>,
        files_scanned: &mut usize,
    ) -> io::Result<()> {
        if *files_scanned >= MAX_FILES_TO_SCAN {
            return Ok(());
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()), // Skip unreadable directories
        };

        for entry in entries.flatten() {
            if *files_scanned >= MAX_FILES_TO_SCAN {
                return Ok(());
            }

            let path = entry.path();
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Skip hidden directories and common non-source directories
            if file_name_str.starts_with('.') {
                continue;
            }
            if matches!(
                file_name_str.as_ref(),
                "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | "vendor"
                    | "__pycache__"
                    | ".git"
                    | "venv"
                    | ".venv"
                    | "env"
            ) {
                continue;
            }

            if path.is_dir() {
                scan_dir(&path, counts, files_scanned)?;
            } else if path.is_file() {
                *files_scanned += 1;
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    *counts.entry(ext_str).or_insert(0) += 1;
                }
            }
        }

        Ok(())
    }

    scan_dir(root, &mut counts, &mut files_scanned)?;
    Ok(counts)
}

/// Detect the project stack for a given repository root
pub(crate) fn detect_stack(root: &Path) -> io::Result<ProjectStack> {
    // Count file extensions
    let extension_counts = count_extensions(root)?;

    // Convert extensions to languages and aggregate
    let mut language_counts: HashMap<&str, usize> = HashMap::new();
    for (ext, count) in &extension_counts {
        if let Some(lang) = extension_to_language(ext) {
            *language_counts.entry(lang).or_insert(0) += count;
        }
    }

    // Sort languages by count (descending)
    let mut language_vec: Vec<_> = language_counts
        .into_iter()
        .filter(|(_, count)| *count >= MIN_FILES_FOR_DETECTION)
        .collect();
    language_vec.sort_by(|a, b| b.1.cmp(&a.1));

    // Determine primary and secondary languages
    let primary_language = language_vec
        .first()
        .map(|(lang, _)| (*lang).to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let secondary_languages: Vec<String> = language_vec
        .iter()
        .skip(1)
        .take(3) // Limit to top 3 secondary languages
        .map(|(lang, _)| (*lang).to_string())
        .collect();

    // Detect signature files for frameworks and test frameworks
    let (frameworks, test_framework, package_manager) = detect_signature_files(root);

    // Detect if tests exist
    let has_tests = test_framework.is_some() || detect_tests(root, &primary_language);

    Ok(ProjectStack {
        primary_language,
        secondary_languages,
        frameworks,
        has_tests,
        test_framework,
        package_manager,
    })
}

/// Detect stack and return a summary string (for display in banner)
pub(crate) fn detect_stack_summary(root: &Path) -> String {
    match detect_stack(root) {
        Ok(stack) => stack.summary(),
        Err(_) => "Unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(path).unwrap();
    }

    #[test]
    fn test_extension_to_language() {
        assert_eq!(extension_to_language("rs"), Some("Rust"));
        assert_eq!(extension_to_language("py"), Some("Python"));
        assert_eq!(extension_to_language("js"), Some("JavaScript"));
        assert_eq!(extension_to_language("ts"), Some("TypeScript"));
        assert_eq!(extension_to_language("go"), Some("Go"));
        assert_eq!(extension_to_language("java"), Some("Java"));
        assert_eq!(extension_to_language("rb"), Some("Ruby"));
        assert_eq!(extension_to_language("unknown"), None);
    }

    #[test]
    fn test_rust_project_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create Rust project structure
        fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
tokio = "1.0"

[dev-dependencies]
tempfile = "3.0"
"#,
        )
        .unwrap();

        create_test_file(root, "src/main.rs");
        create_test_file(root, "src/lib.rs");
        create_test_file(root, "tests/integration.rs");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Rust");
        assert!(stack.is_rust());
        assert!(stack.frameworks.contains(&"Tokio".to_string()));
        assert_eq!(stack.package_manager, Some("Cargo".to_string()));
        assert!(stack.has_tests);
        assert_eq!(stack.test_framework, Some("cargo test".to_string()));
    }

    #[test]
    fn test_python_project_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create Python project structure
        fs::write(
            root.join("pyproject.toml"),
            r#"
[project]
name = "test"

[project.dependencies]
django = "^4.0"

[tool.pytest.ini_options]
testpaths = ["tests"]
"#,
        )
        .unwrap();

        create_test_file(root, "src/main.py");
        create_test_file(root, "src/utils.py");
        create_test_file(root, "tests/test_main.py");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Python");
        assert!(stack.is_python());
        assert_eq!(stack.package_manager, Some("Poetry/pip".to_string()));
    }

    #[test]
    fn test_javascript_react_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create React project structure
        fs::write(
            root.join("package.json"),
            r#"{
  "name": "test",
  "dependencies": {
    "react": "^18.0.0",
    "react-dom": "^18.0.0"
  },
  "devDependencies": {
    "jest": "^29.0.0"
  }
}"#,
        )
        .unwrap();

        create_test_file(root, "src/App.jsx");
        create_test_file(root, "src/index.js");
        create_test_file(root, "__tests__/App.test.js");

        let stack = detect_stack(root).unwrap();

        assert!(stack.is_javascript_or_typescript());
        assert!(stack.frameworks.contains(&"React".to_string()));
        assert_eq!(stack.test_framework, Some("Jest".to_string()));
        assert!(stack.has_tests);
    }

    #[test]
    fn test_go_project_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create Go project structure
        fs::write(
            root.join("go.mod"),
            r#"
module example.com/test

go 1.21

require github.com/gin-gonic/gin v1.9.0
"#,
        )
        .unwrap();

        create_test_file(root, "main.go");
        create_test_file(root, "handlers/api.go");
        create_test_file(root, "handlers/api_test.go");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Go");
        assert!(stack.is_go());
        assert!(stack.frameworks.contains(&"Gin".to_string()));
        assert_eq!(stack.test_framework, Some("go test".to_string()));
    }

    #[test]
    fn test_typescript_nextjs_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{
  "name": "test",
  "dependencies": {
    "next": "^14.0.0",
    "react": "^18.0.0"
  },
  "devDependencies": {
    "vitest": "^1.0.0",
    "typescript": "^5.0.0"
  }
}"#,
        )
        .unwrap();

        create_test_file(root, "src/app/page.tsx");
        create_test_file(root, "src/components/Header.tsx");
        create_test_file(root, "tsconfig.json");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "TypeScript");
        assert!(stack.frameworks.contains(&"Next.js".to_string()));
        assert!(stack.frameworks.contains(&"React".to_string()));
        assert_eq!(stack.test_framework, Some("Vitest".to_string()));
    }

    #[test]
    fn test_mixed_language_project() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create a project with multiple languages
        create_test_file(root, "src/main.rs");
        create_test_file(root, "src/lib.rs");
        create_test_file(root, "scripts/setup.py");
        create_test_file(root, "scripts/deploy.sh");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Rust");
        assert!(!stack.secondary_languages.is_empty());
    }

    #[test]
    fn test_empty_directory() {
        let dir = TempDir::new().unwrap();
        let stack = detect_stack(dir.path()).unwrap();

        assert_eq!(stack.primary_language, "Unknown");
        assert!(stack.secondary_languages.is_empty());
        assert!(stack.frameworks.is_empty());
    }

    #[test]
    fn test_skips_node_modules() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create a JS file in the project
        create_test_file(root, "src/index.js");

        // Create many files in node_modules (should be skipped)
        let node_modules = root.join("node_modules");
        fs::create_dir_all(&node_modules).unwrap();
        for i in 0..100 {
            create_test_file(&node_modules, &format!("dep{}/index.js", i));
        }

        let stack = detect_stack(root).unwrap();

        // Should still detect JavaScript, not be overwhelmed by node_modules
        assert_eq!(stack.primary_language, "JavaScript");
    }

    #[test]
    fn test_project_stack_summary() {
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec!["Python".to_string()],
            frameworks: vec!["Actix".to_string()],
            has_tests: true,
            test_framework: Some("cargo test".to_string()),
            package_manager: Some("Cargo".to_string()),
        };

        let summary = stack.summary();
        assert!(summary.contains("Rust"));
        assert!(summary.contains("Python"));
        assert!(summary.contains("Actix"));
        assert!(summary.contains("cargo test"));
    }

    #[test]
    fn test_detect_stack_summary() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        create_test_file(root, "src/main.rs");

        let summary = detect_stack_summary(root);
        assert!(summary.contains("Rust"));
    }

    #[test]
    fn test_ruby_rails_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("Gemfile"),
            r#"
source 'https://rubygems.org'
gem 'rails', '~> 7.0'
gem 'rspec-rails', group: :test
"#,
        )
        .unwrap();

        create_test_file(root, "app/controllers/application_controller.rb");
        create_test_file(root, "spec/models/user_spec.rb");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Ruby");
        assert!(stack.frameworks.contains(&"Rails".to_string()));
        assert_eq!(stack.test_framework, Some("RSpec".to_string()));
        assert!(stack.has_tests);
    }

    #[test]
    fn test_java_spring_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("pom.xml"),
            r#"
<project>
  <dependencies>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter</artifactId>
    </dependency>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>
"#,
        )
        .unwrap();

        fs::create_dir_all(root.join("src/main/java")).unwrap();
        create_test_file(root, "src/main/java/App.java");
        fs::create_dir_all(root.join("src/test/java")).unwrap();
        create_test_file(root, "src/test/java/AppTest.java");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Java");
        assert!(stack.frameworks.contains(&"Spring".to_string()));
        assert_eq!(stack.test_framework, Some("JUnit".to_string()));
        assert!(stack.has_tests);
    }

    // ============================================================================
    // Additional Language Detection Tests
    // ============================================================================

    #[test]
    fn test_extension_to_language_comprehensive() {
        // Rust
        assert_eq!(extension_to_language("rs"), Some("Rust"));

        // Python variants
        assert_eq!(extension_to_language("py"), Some("Python"));
        assert_eq!(extension_to_language("pyw"), Some("Python"));
        assert_eq!(extension_to_language("pyi"), Some("Python"));

        // JavaScript variants
        assert_eq!(extension_to_language("js"), Some("JavaScript"));
        assert_eq!(extension_to_language("mjs"), Some("JavaScript"));
        assert_eq!(extension_to_language("cjs"), Some("JavaScript"));
        assert_eq!(extension_to_language("jsx"), Some("JavaScript"));

        // TypeScript variants
        assert_eq!(extension_to_language("ts"), Some("TypeScript"));
        assert_eq!(extension_to_language("mts"), Some("TypeScript"));
        assert_eq!(extension_to_language("cts"), Some("TypeScript"));
        assert_eq!(extension_to_language("tsx"), Some("TypeScript"));

        // Go
        assert_eq!(extension_to_language("go"), Some("Go"));

        // Java
        assert_eq!(extension_to_language("java"), Some("Java"));

        // Kotlin
        assert_eq!(extension_to_language("kt"), Some("Kotlin"));
        assert_eq!(extension_to_language("kts"), Some("Kotlin"));

        // C/C++
        assert_eq!(extension_to_language("c"), Some("C"));
        assert_eq!(extension_to_language("h"), Some("C"));
        assert_eq!(extension_to_language("cpp"), Some("C++"));
        assert_eq!(extension_to_language("cc"), Some("C++"));
        assert_eq!(extension_to_language("cxx"), Some("C++"));
        assert_eq!(extension_to_language("hpp"), Some("C++"));
        assert_eq!(extension_to_language("hxx"), Some("C++"));
        assert_eq!(extension_to_language("hh"), Some("C++"));

        // C#
        assert_eq!(extension_to_language("cs"), Some("C#"));

        // Ruby
        assert_eq!(extension_to_language("rb"), Some("Ruby"));
        assert_eq!(extension_to_language("erb"), Some("Ruby"));

        // PHP
        assert_eq!(extension_to_language("php"), Some("PHP"));

        // Swift
        assert_eq!(extension_to_language("swift"), Some("Swift"));

        // Scala
        assert_eq!(extension_to_language("scala"), Some("Scala"));
        assert_eq!(extension_to_language("sc"), Some("Scala"));

        // Shell
        assert_eq!(extension_to_language("sh"), Some("Shell"));
        assert_eq!(extension_to_language("bash"), Some("Shell"));
        assert_eq!(extension_to_language("zsh"), Some("Shell"));

        // SQL
        assert_eq!(extension_to_language("sql"), Some("SQL"));

        // Lua
        assert_eq!(extension_to_language("lua"), Some("Lua"));

        // Perl
        assert_eq!(extension_to_language("pl"), Some("Perl"));
        assert_eq!(extension_to_language("pm"), Some("Perl"));

        // R
        assert_eq!(extension_to_language("r"), Some("R"));

        // Dart
        assert_eq!(extension_to_language("dart"), Some("Dart"));

        // Elixir
        assert_eq!(extension_to_language("ex"), Some("Elixir"));
        assert_eq!(extension_to_language("exs"), Some("Elixir"));

        // Haskell
        assert_eq!(extension_to_language("hs"), Some("Haskell"));
        assert_eq!(extension_to_language("lhs"), Some("Haskell"));

        // OCaml
        assert_eq!(extension_to_language("ml"), Some("OCaml"));
        assert_eq!(extension_to_language("mli"), Some("OCaml"));

        // F#
        assert_eq!(extension_to_language("fs"), Some("F#"));
        assert_eq!(extension_to_language("fsi"), Some("F#"));
        assert_eq!(extension_to_language("fsx"), Some("F#"));

        // Clojure
        assert_eq!(extension_to_language("clj"), Some("Clojure"));
        assert_eq!(extension_to_language("cljs"), Some("Clojure"));
        assert_eq!(extension_to_language("cljc"), Some("Clojure"));
        assert_eq!(extension_to_language("edn"), Some("Clojure"));

        // Zig
        assert_eq!(extension_to_language("zig"), Some("Zig"));

        // Nim
        assert_eq!(extension_to_language("nim"), Some("Nim"));

        // V
        assert_eq!(extension_to_language("v"), Some("V"));

        // Unknown
        assert_eq!(extension_to_language("unknown"), None);
        assert_eq!(extension_to_language("txt"), None);
        assert_eq!(extension_to_language("md"), None);
    }

    #[test]
    fn test_is_rust_method() {
        // Primary language is Rust
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            ..Default::default()
        };
        assert!(stack.is_rust());

        // Secondary language includes Rust
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec!["Rust".to_string()],
            ..Default::default()
        };
        assert!(stack.is_rust());

        // Neither primary nor secondary
        let stack = ProjectStack {
            primary_language: "JavaScript".to_string(),
            secondary_languages: vec!["Python".to_string()],
            ..Default::default()
        };
        assert!(!stack.is_rust());
    }

    #[test]
    fn test_is_python_method() {
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            ..Default::default()
        };
        assert!(stack.is_python());

        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec!["Python".to_string()],
            ..Default::default()
        };
        assert!(stack.is_python());

        let stack = ProjectStack {
            primary_language: "JavaScript".to_string(),
            ..Default::default()
        };
        assert!(!stack.is_python());
    }

    #[test]
    fn test_is_javascript_or_typescript_method() {
        // JavaScript primary
        let stack = ProjectStack {
            primary_language: "JavaScript".to_string(),
            ..Default::default()
        };
        assert!(stack.is_javascript_or_typescript());

        // TypeScript primary
        let stack = ProjectStack {
            primary_language: "TypeScript".to_string(),
            ..Default::default()
        };
        assert!(stack.is_javascript_or_typescript());

        // JavaScript secondary
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec!["JavaScript".to_string()],
            ..Default::default()
        };
        assert!(stack.is_javascript_or_typescript());

        // TypeScript secondary
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec!["TypeScript".to_string()],
            ..Default::default()
        };
        assert!(stack.is_javascript_or_typescript());

        // Neither
        let stack = ProjectStack {
            primary_language: "Go".to_string(),
            secondary_languages: vec!["Python".to_string()],
            ..Default::default()
        };
        assert!(!stack.is_javascript_or_typescript());
    }

    #[test]
    fn test_is_go_method() {
        let stack = ProjectStack {
            primary_language: "Go".to_string(),
            ..Default::default()
        };
        assert!(stack.is_go());

        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec!["Go".to_string()],
            ..Default::default()
        };
        assert!(stack.is_go());

        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            ..Default::default()
        };
        assert!(!stack.is_go());
    }

    #[test]
    fn test_summary_with_no_tests() {
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: None,
        };

        let summary = stack.summary();
        assert_eq!(summary, "Rust");
        assert!(!summary.contains("tests"));
    }

    #[test]
    fn test_summary_with_tests_no_framework() {
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: true,
            test_framework: None,
            package_manager: None,
        };

        let summary = stack.summary();
        assert!(summary.contains("tests:yes"));
    }

    #[test]
    fn test_summary_comprehensive() {
        let stack = ProjectStack {
            primary_language: "TypeScript".to_string(),
            secondary_languages: vec!["JavaScript".to_string(), "Python".to_string()],
            frameworks: vec!["React".to_string(), "Next.js".to_string()],
            has_tests: true,
            test_framework: Some("Jest".to_string()),
            package_manager: Some("npm".to_string()),
        };

        let summary = stack.summary();
        assert!(summary.contains("TypeScript"));
        assert!(summary.contains("JavaScript"));
        assert!(summary.contains("Python"));
        assert!(summary.contains("React"));
        assert!(summary.contains("Next.js"));
        assert!(summary.contains("Jest"));
    }

    // ============================================================================
    // PHP Detection Tests
    // ============================================================================

    #[test]
    fn test_php_laravel_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("composer.json"),
            r#"{
    "name": "test/app",
    "require": {
        "laravel/framework": "^10.0"
    },
    "require-dev": {
        "phpunit/phpunit": "^10.0"
    }
}"#,
        )
        .unwrap();

        create_test_file(root, "app/Http/Controllers/HomeController.php");
        create_test_file(root, "tests/Feature/ExampleTest.php");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "PHP");
        assert!(stack.frameworks.contains(&"Laravel".to_string()));
        assert_eq!(stack.test_framework, Some("PHPUnit".to_string()));
        assert_eq!(stack.package_manager, Some("Composer".to_string()));
    }

    // ============================================================================
    // Elixir Detection Tests
    // ============================================================================

    #[test]
    fn test_elixir_phoenix_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("mix.exs"),
            r#"
defmodule MyApp.MixProject do
  use Mix.Project

  def project do
    [
      app: :my_app,
      deps: deps()
    ]
  end

  defp deps do
    [
      {:phoenix, "~> 1.7"}
    ]
  end
end
"#,
        )
        .unwrap();

        create_test_file(root, "lib/my_app.ex");
        create_test_file(root, "test/my_app_test.exs");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Elixir");
        assert!(stack.frameworks.contains(&"Phoenix".to_string()));
        assert_eq!(stack.test_framework, Some("ExUnit".to_string()));
        assert_eq!(stack.package_manager, Some("Mix".to_string()));
    }

    // ============================================================================
    // Dart/Flutter Detection Tests
    // ============================================================================

    #[test]
    fn test_dart_flutter_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("pubspec.yaml"),
            r#"
name: my_app
dependencies:
  flutter:
    sdk: flutter
dev_dependencies:
  test: ^1.0.0
"#,
        )
        .unwrap();

        create_test_file(root, "lib/main.dart");
        create_test_file(root, "test/widget_test.dart");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Dart");
        assert!(stack.frameworks.contains(&"Flutter".to_string()));
        assert_eq!(stack.package_manager, Some("pub".to_string()));
    }

    // ============================================================================
    // Multiple Framework Detection Tests
    // ============================================================================

    #[test]
    fn test_rust_multiple_frameworks() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
axum = "0.7"
tokio = { version = "1.0", features = ["full"] }
leptos = "0.5"

[dev-dependencies]
"#,
        )
        .unwrap();

        create_test_file(root, "src/main.rs");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Rust");
        assert!(stack.frameworks.contains(&"Axum".to_string()));
        assert!(stack.frameworks.contains(&"Tokio".to_string()));
        assert!(stack.frameworks.contains(&"Leptos".to_string()));
    }

    #[test]
    fn test_vue_nuxt_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{
    "name": "test",
    "dependencies": {
        "vue": "^3.0.0",
        "nuxt": "^3.0.0"
    },
    "devDependencies": {
        "vitest": "^1.0.0"
    }
}"#,
        )
        .unwrap();

        // Note: .vue files aren't detected as JS/TS by extension
        // In real Vue projects, there are also .js/.ts files
        create_test_file(root, "nuxt.config.ts");
        create_test_file(root, "composables/useAuth.ts");

        let stack = detect_stack(root).unwrap();

        assert!(stack.is_javascript_or_typescript());
        assert!(stack.frameworks.contains(&"Vue".to_string()));
        assert!(stack.frameworks.contains(&"Nuxt".to_string()));
        assert_eq!(stack.test_framework, Some("Vitest".to_string()));
    }

    #[test]
    fn test_angular_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{
    "name": "test",
    "dependencies": {
        "@angular/core": "^17.0.0",
        "@angular/common": "^17.0.0"
    }
}"#,
        )
        .unwrap();

        create_test_file(root, "src/app/app.component.ts");

        let stack = detect_stack(root).unwrap();

        assert!(stack.frameworks.contains(&"Angular".to_string()));
    }

    // ============================================================================
    // Go Framework Detection Tests
    // ============================================================================

    #[test]
    fn test_go_chi_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("go.mod"),
            r#"
module example.com/test

go 1.21

require github.com/go-chi/chi/v5 v5.0.0
"#,
        )
        .unwrap();

        create_test_file(root, "main.go");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Go");
        assert!(stack.frameworks.contains(&"Chi".to_string()));
    }

    #[test]
    fn test_go_fiber_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("go.mod"),
            r#"
module example.com/test

go 1.21

require github.com/gofiber/fiber/v2 v2.52.0
"#,
        )
        .unwrap();

        create_test_file(root, "main.go");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Go");
        assert!(stack.frameworks.contains(&"Fiber".to_string()));
    }

    #[test]
    fn test_go_echo_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("go.mod"),
            r#"
module example.com/test

go 1.21

require github.com/labstack/echo/v4 v4.11.0
"#,
        )
        .unwrap();

        create_test_file(root, "main.go");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Go");
        assert!(stack.frameworks.contains(&"Echo".to_string()));
    }

    // ============================================================================
    // Monorepo and Edge Case Tests
    // ============================================================================

    #[test]
    fn test_monorepo_with_multiple_packages() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Backend in Go - create directory first
        fs::create_dir_all(root.join("backend")).unwrap();
        fs::write(
            root.join("backend/go.mod"),
            "module example.com/backend\n\ngo 1.21",
        )
        .unwrap();
        create_test_file(root, "backend/main.go");
        create_test_file(root, "backend/handlers/api.go");

        // Frontend in TypeScript - create directory first
        fs::create_dir_all(root.join("frontend")).unwrap();
        fs::write(
            root.join("frontend/package.json"),
            r#"{"name": "frontend", "dependencies": {"react": "^18.0.0"}}"#,
        )
        .unwrap();
        create_test_file(root, "frontend/src/App.tsx");
        create_test_file(root, "frontend/src/index.ts");
        create_test_file(root, "frontend/src/utils.ts");

        // Shared scripts in Python
        create_test_file(root, "scripts/deploy.py");
        create_test_file(root, "scripts/build.py");

        let stack = detect_stack(root).unwrap();

        // TypeScript should be primary (more files)
        assert_eq!(stack.primary_language, "TypeScript");
        // Go and Python should be secondary
        assert!(stack.secondary_languages.contains(&"Go".to_string()));
    }

    #[test]
    fn test_skips_target_directory() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create a small Rust project
        create_test_file(root, "src/main.rs");

        // Create many files in target/ (should be skipped)
        let target_dir = root.join("target/debug/deps");
        fs::create_dir_all(&target_dir).unwrap();
        for i in 0..100 {
            create_test_file(&target_dir, &format!("libdep{}.rs", i));
        }

        let stack = detect_stack(root).unwrap();

        // Should detect Rust, not be overwhelmed by target/
        assert_eq!(stack.primary_language, "Rust");
    }

    #[test]
    fn test_skips_hidden_directories() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create a Python file
        create_test_file(root, "main.py");

        // Create many files in .git (should be skipped)
        let git_dir = root.join(".git/objects");
        fs::create_dir_all(&git_dir).unwrap();
        for i in 0..50 {
            create_test_file(&git_dir, &format!("{}.py", i));
        }

        // Create files in .venv (should be skipped)
        let venv_dir = root.join(".venv/lib/python3/site-packages");
        fs::create_dir_all(&venv_dir).unwrap();
        for i in 0..50 {
            create_test_file(&venv_dir, &format!("pkg{}.py", i));
        }

        let stack = detect_stack(root).unwrap();

        // Should detect Python from main.py, not from hidden dirs
        assert_eq!(stack.primary_language, "Python");
    }

    #[test]
    fn test_project_stack_default() {
        let stack = ProjectStack::default();

        assert_eq!(stack.primary_language, "Unknown");
        assert!(stack.secondary_languages.is_empty());
        assert!(stack.frameworks.is_empty());
        assert!(!stack.has_tests);
        assert!(stack.test_framework.is_none());
        assert!(stack.package_manager.is_none());
    }

    #[test]
    fn test_python_requirements_txt_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("requirements.txt"),
            r#"
Django==4.2.0
pytest==7.4.0
Flask==2.3.0
"#,
        )
        .unwrap();

        create_test_file(root, "app.py");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Python");
        assert!(stack.frameworks.contains(&"Django".to_string()));
        assert!(stack.frameworks.contains(&"Flask".to_string()));
        assert_eq!(stack.test_framework, Some("pytest".to_string()));
        assert_eq!(stack.package_manager, Some("pip".to_string()));
    }

    #[test]
    fn test_python_pipfile_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(root.join("Pipfile"), "[packages]\n").unwrap();
        create_test_file(root, "main.py");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Python");
        assert_eq!(stack.package_manager, Some("Pipenv".to_string()));
    }

    #[test]
    fn test_python_setup_py_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(root.join("setup.py"), "from setuptools import setup").unwrap();
        create_test_file(root, "src/mypackage/__init__.py");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Python");
        assert_eq!(stack.package_manager, Some("setuptools".to_string()));
    }

    #[test]
    fn test_gradle_kotlin_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("build.gradle.kts"),
            r#"
plugins {
    kotlin("jvm") version "1.9.0"
}

dependencies {
    implementation("org.springframework.boot:spring-boot-starter")
    testImplementation("junit:junit:4.13.2")
}
"#,
        )
        .unwrap();

        fs::create_dir_all(root.join("src/main/kotlin")).unwrap();
        create_test_file(root, "src/main/kotlin/App.kt");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Kotlin");
        assert!(stack.frameworks.contains(&"Spring".to_string()));
        assert_eq!(stack.test_framework, Some("JUnit".to_string()));
        assert_eq!(stack.package_manager, Some("Gradle".to_string()));
    }

    #[test]
    fn test_node_backend_frameworks() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{
    "name": "test",
    "dependencies": {
        "express": "^4.18.0",
        "fastify": "^4.0.0"
    },
    "devDependencies": {
        "mocha": "^10.0.0"
    }
}"#,
        )
        .unwrap();

        create_test_file(root, "src/server.js");

        let stack = detect_stack(root).unwrap();

        assert!(stack.frameworks.contains(&"Express".to_string()));
        assert!(stack.frameworks.contains(&"Fastify".to_string()));
        assert_eq!(stack.test_framework, Some("Mocha".to_string()));
    }

    #[test]
    fn test_nestjs_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{
    "name": "test",
    "dependencies": {
        "@nestjs/core": "^10.0.0",
        "@nestjs/common": "^10.0.0"
    }
}"#,
        )
        .unwrap();

        create_test_file(root, "src/app.module.ts");

        let stack = detect_stack(root).unwrap();

        assert!(stack.frameworks.contains(&"NestJS".to_string()));
    }

    #[test]
    fn test_svelte_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{
    "name": "test",
    "dependencies": {
        "svelte": "^4.0.0"
    }
}"#,
        )
        .unwrap();

        create_test_file(root, "src/App.svelte");
        create_test_file(root, "src/main.js");

        let stack = detect_stack(root).unwrap();

        assert!(stack.frameworks.contains(&"Svelte".to_string()));
    }

    #[test]
    fn test_electron_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{
    "name": "test",
    "dependencies": {
        "electron": "^27.0.0"
    }
}"#,
        )
        .unwrap();

        create_test_file(root, "main.js");

        let stack = detect_stack(root).unwrap();

        assert!(stack.frameworks.contains(&"Electron".to_string()));
    }

    #[test]
    fn test_ruby_minitest_detection() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(
            root.join("Gemfile"),
            r#"
source 'https://rubygems.org'
gem 'sinatra'
gem 'minitest', group: :test
"#,
        )
        .unwrap();

        create_test_file(root, "app.rb");
        create_test_file(root, "test/test_app.rb");

        let stack = detect_stack(root).unwrap();

        assert_eq!(stack.primary_language, "Ruby");
        assert!(stack.frameworks.contains(&"Sinatra".to_string()));
        assert_eq!(stack.test_framework, Some("Minitest".to_string()));
    }

    #[test]
    fn test_case_insensitive_extension() {
        // Test that extensions are case-insensitive
        assert_eq!(extension_to_language("RS"), Some("Rust"));
        assert_eq!(extension_to_language("Py"), Some("Python"));
        assert_eq!(extension_to_language("JS"), Some("JavaScript"));
    }
}
