//! Signature file detection for frameworks and package managers.
//!
//! Analyzes configuration files like Cargo.toml, package.json, etc.
//! to detect frameworks, test frameworks, and package managers.

use super::scanner::{should_skip_dir_name, MAX_FILES_TO_SCAN, MAX_SIGNATURE_SEARCH_DEPTH};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum number of signature files to collect (across all types).
const MAX_SIGNATURE_FILES: usize = 50;

/// Helper to push unique values to a vector.
fn push_unique(vec: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !vec.iter().any(|v| v == &value) {
        vec.push(value);
    }
}

/// Combine multiple items into a single string.
fn combine_unique(items: &[String]) -> Option<String> {
    match items.len() {
        0 => None,
        1 => Some(items[0].clone()),
        _ => Some(
            items
                .iter()
                .map(std::string::String::as_str)
                .collect::<Vec<_>>()
                .join(" + "),
        ),
    }
}

/// Container for signature files found during scanning.
#[derive(Default)]
struct SignatureFiles {
    by_name_lower: HashMap<String, Vec<PathBuf>>,
    by_extension_lower: HashMap<String, Vec<PathBuf>>,
}

/// Collect signature files from the repository.
fn collect_signature_files(root: &Path) -> SignatureFiles {
    let mut targets: HashSet<String> = HashSet::new();
    for name in [
        "cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "pipfile",
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "gemfile",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "composer.json",
        "mix.exs",
        "pubspec.yaml",
    ] {
        let _ = targets.insert(name.to_string());
    }

    let mut result = SignatureFiles::default();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    let mut scanned_entries: usize = 0;
    let mut collected: usize = 0;

    while let Some((dir, depth)) = queue.pop_front() {
        if scanned_entries >= MAX_FILES_TO_SCAN || collected >= MAX_SIGNATURE_FILES {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            if scanned_entries >= MAX_FILES_TO_SCAN || collected >= MAX_SIGNATURE_FILES {
                break;
            }
            scanned_entries += 1;

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let name_lower = name.to_lowercase();

            if path.is_dir() {
                if should_skip_dir_name(&name_lower) {
                    continue;
                }
                if depth < MAX_SIGNATURE_SEARCH_DEPTH {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }

            if !path.is_file() {
                continue;
            }

            if targets.contains(&name_lower) {
                result
                    .by_name_lower
                    .entry(name_lower.clone())
                    .or_default()
                    .push(path.clone());
                collected += 1;
            }

            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if ext_lower == "csproj" {
                    result
                        .by_extension_lower
                        .entry(ext_lower)
                        .or_default()
                        .push(path.clone());
                    collected += 1;
                }
            }
        }
    }

    result
}

/// Detection results accumulator
struct DetectionResults {
    frameworks: Vec<String>,
    test_frameworks: Vec<String>,
    package_managers: Vec<String>,
}

impl DetectionResults {
    const fn new() -> Self {
        Self {
            frameworks: Vec::new(),
            test_frameworks: Vec::new(),
            package_managers: Vec::new(),
        }
    }

    fn push_framework(&mut self, framework: impl Into<String>) {
        push_unique(&mut self.frameworks, framework);
    }

    fn push_test_framework(&mut self, framework: impl Into<String>) {
        push_unique(&mut self.test_frameworks, framework);
    }

    fn push_package_manager(&mut self, manager: impl Into<String>) {
        push_unique(&mut self.package_managers, manager);
    }

    fn finish(self) -> (Vec<String>, Option<String>, Option<String>) {
        (
            self.frameworks,
            combine_unique(&self.test_frameworks),
            combine_unique(&self.package_managers),
        )
    }
}

/// Detect Rust-specific frameworks and tools
fn detect_rust(signatures: &SignatureFiles, results: &mut DetectionResults) {
    let Some(paths) = signatures.by_name_lower.get("cargo.toml") else {
        return;
    };

    results.push_package_manager("Cargo");

    for path in paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("[dev-dependencies]") || content_lower.contains("[[test]]") {
            results.push_test_framework("cargo test");
        }

        for (name, framework) in [
            ("actix", "Actix"),
            ("axum", "Axum"),
            ("rocket", "Rocket"),
            ("tokio", "Tokio"),
            ("warp", "Warp"),
            ("tauri", "Tauri"),
            ("leptos", "Leptos"),
            ("yew", "Yew"),
        ] {
            if content_lower.contains(name) {
                results.push_framework(framework);
            }
        }
    }
}

/// Detect Python-specific frameworks and tools
fn detect_python(signatures: &SignatureFiles, results: &mut DetectionResults) {
    if let Some(paths) = signatures.by_name_lower.get("pyproject.toml") {
        results.push_package_manager("Poetry/pip");
        detect_python_frameworks(paths, results);
    } else if let Some(paths) = signatures.by_name_lower.get("requirements.txt") {
        results.push_package_manager("pip");
        detect_python_frameworks(paths, results);
    } else if signatures.by_name_lower.contains_key("setup.py") {
        results.push_package_manager("setuptools");
    } else if signatures.by_name_lower.contains_key("pipfile") {
        results.push_package_manager("Pipenv");
    }
}

/// Helper to detect Python frameworks from file content
fn detect_python_frameworks(paths: &[PathBuf], results: &mut DetectionResults) {
    for path in paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("pytest") {
            results.push_test_framework("pytest");
        }
        for (name, framework) in [
            ("django", "Django"),
            ("fastapi", "FastAPI"),
            ("flask", "Flask"),
        ] {
            if content_lower.contains(name) {
                results.push_framework(framework);
            }
        }
    }
}

/// Detect JavaScript/TypeScript frameworks and tools
fn detect_javascript(signatures: &SignatureFiles, root: &Path, results: &mut DetectionResults) {
    let Some(paths) = signatures.by_name_lower.get("package.json") else {
        return;
    };

    for path in paths {
        let pkg_dir = path.parent().unwrap_or(root);
        if pkg_dir.join("pnpm-lock.yaml").exists() {
            results.push_package_manager("pnpm");
        } else if pkg_dir.join("yarn.lock").exists() {
            results.push_package_manager("yarn");
        } else {
            results.push_package_manager("npm");
        }

        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        // Test frameworks
        for (pattern, name) in [
            ("\"jest\"", "Jest"),
            ("\"vitest\"", "Vitest"),
            ("\"mocha\"", "Mocha"),
        ] {
            if content_lower.contains(pattern) {
                results.push_test_framework(name);
                break;
            }
        }

        // Frameworks
        for (pattern, name) in [
            ("\"react\"", "React"),
            ("\"vue\"", "Vue"),
            ("\"svelte\"", "Svelte"),
            ("\"angular\"", "Angular"),
            ("\"@angular\"", "Angular"),
            ("\"next\"", "Next.js"),
            ("\"nuxt\"", "Nuxt"),
            ("\"express\"", "Express"),
            ("\"fastify\"", "Fastify"),
            ("\"nest\"", "NestJS"),
            ("\"@nestjs\"", "NestJS"),
            ("\"electron\"", "Electron"),
        ] {
            if content_lower.contains(pattern) {
                results.push_framework(name);
            }
        }
    }
}

/// Detect Go-specific frameworks and tools
fn detect_go(signatures: &SignatureFiles, results: &mut DetectionResults) {
    let Some(paths) = signatures.by_name_lower.get("go.mod") else {
        return;
    };

    results.push_package_manager("Go modules");
    results.push_test_framework("go test");

    for path in paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        for (pattern, name) in [
            ("gin-gonic/gin", "Gin"),
            ("go-chi/chi", "Chi"),
            ("gofiber/fiber", "Fiber"),
            ("labstack/echo", "Echo"),
        ] {
            if content_lower.contains(pattern) {
                results.push_framework(name);
            }
        }
    }
}

/// Detect Ruby-specific frameworks and tools
fn detect_ruby(signatures: &SignatureFiles, results: &mut DetectionResults) {
    let Some(paths) = signatures.by_name_lower.get("gemfile") else {
        return;
    };

    results.push_package_manager("Bundler");

    for path in paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("rspec") {
            results.push_test_framework("RSpec");
        } else if content_lower.contains("minitest") {
            results.push_test_framework("Minitest");
        }

        for (name, framework) in [("rails", "Rails"), ("sinatra", "Sinatra")] {
            if content_lower.contains(name) {
                results.push_framework(framework);
            }
        }
    }
}

/// Detect Java-specific frameworks and tools
fn detect_java(signatures: &SignatureFiles, results: &mut DetectionResults) {
    if let Some(paths) = signatures.by_name_lower.get("pom.xml") {
        results.push_package_manager("Maven");
        detect_java_frameworks(paths, results);
    } else if signatures.by_name_lower.contains_key("build.gradle")
        || signatures.by_name_lower.contains_key("build.gradle.kts")
    {
        results.push_package_manager("Gradle");
        let paths = signatures
            .by_name_lower
            .get("build.gradle.kts")
            .or_else(|| signatures.by_name_lower.get("build.gradle"));
        if let Some(paths) = paths {
            detect_java_frameworks(paths, results);
        }
    }
}

/// Helper to detect Java frameworks from file content
fn detect_java_frameworks(paths: &[PathBuf], results: &mut DetectionResults) {
    for path in paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("junit") {
            results.push_test_framework("JUnit");
        }
        if content_lower.contains("spring") {
            results.push_framework("Spring");
        }
    }
}

/// Detect PHP-specific frameworks and tools
fn detect_php(signatures: &SignatureFiles, results: &mut DetectionResults) {
    let Some(paths) = signatures.by_name_lower.get("composer.json") else {
        return;
    };

    results.push_package_manager("Composer");

    for path in paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("phpunit") {
            results.push_test_framework("PHPUnit");
        }
        for (name, framework) in [("laravel", "Laravel"), ("symfony", "Symfony")] {
            if content_lower.contains(name) {
                results.push_framework(framework);
            }
        }
    }
}

/// Detect .NET/C# tools
fn detect_dotnet(signatures: &SignatureFiles, results: &mut DetectionResults) {
    if signatures.by_extension_lower.contains_key("csproj") {
        results.push_package_manager("NuGet");
    }
}

/// Detect Elixir-specific frameworks and tools
fn detect_elixir(signatures: &SignatureFiles, results: &mut DetectionResults) {
    let Some(paths) = signatures.by_name_lower.get("mix.exs") else {
        return;
    };

    results.push_package_manager("Mix");
    results.push_test_framework("ExUnit");

    for path in paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        if content.to_lowercase().contains(":phoenix") {
            results.push_framework("Phoenix");
        }
    }
}

/// Detect Dart/Flutter-specific frameworks and tools
fn detect_dart(signatures: &SignatureFiles, results: &mut DetectionResults) {
    let Some(paths) = signatures.by_name_lower.get("pubspec.yaml") else {
        return;
    };

    results.push_package_manager("pub");

    for path in paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("flutter") {
            results.push_framework("Flutter");
        }
        if content_lower.contains("test:") {
            results.push_test_framework("dart test");
        }
    }
}

/// Detect signature files that indicate specific frameworks or package managers.
///
/// Returns a tuple of (frameworks, `test_framework`, `package_manager`).
pub(super) fn detect_signature_files(root: &Path) -> (Vec<String>, Option<String>, Option<String>) {
    let signatures = collect_signature_files(root);
    let mut results = DetectionResults::new();

    detect_rust(&signatures, &mut results);
    detect_python(&signatures, &mut results);
    detect_javascript(&signatures, root, &mut results);
    detect_go(&signatures, &mut results);
    detect_ruby(&signatures, &mut results);
    detect_java(&signatures, &mut results);
    detect_php(&signatures, &mut results);
    detect_dotnet(&signatures, &mut results);
    detect_elixir(&signatures, &mut results);
    detect_dart(&signatures, &mut results);

    results.finish()
}

// =============================================================================
// Workspace-based variants
// =============================================================================

#[cfg(any(test, feature = "test-utils"))]
use crate::workspace::Workspace;

/// Collect signature files using workspace.
#[cfg(any(test, feature = "test-utils"))]
fn collect_signature_files_with_workspace(
    workspace: &dyn Workspace,
    root: &Path,
) -> SignatureFiles {
    let mut targets: HashSet<String> = HashSet::new();
    for name in [
        "cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "package.json",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "gemfile",
        "composer.json",
        "mix.exs",
        "project.clj",
        "deno.json",
        "bun.lockb",
        ".eslintrc",
        ".prettierrc",
        "jest.config",
        "vite.config",
        "webpack.config",
        "tsconfig.json",
        "angular.json",
        "next.config",
        "nuxt.config",
        "svelte.config",
        "astro.config",
        "tailwind.config",
        "postcss.config",
        "pnpm-lock.yaml",
        "yarn.lock",
    ] {
        targets.insert(name.to_string());
    }

    let extensions: HashSet<&str> = ["toml", "json", "yaml", "yml", "js", "ts", "mjs"]
        .into_iter()
        .collect();

    let mut result = SignatureFiles::default();
    let mut total_collected = 0usize;
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    while let Some((dir, depth)) = queue.pop_front() {
        if total_collected >= MAX_SIGNATURE_FILES {
            break;
        }
        let Ok(entries) = workspace.read_dir(&dir) else {
            continue;
        };

        for entry in entries {
            if total_collected >= MAX_SIGNATURE_FILES {
                break;
            }
            let path = entry.path().to_path_buf();
            let Some(name_os) = entry.file_name() else {
                continue;
            };
            let name = name_os.to_string_lossy().to_string();
            let name_lower = name.to_lowercase();

            if entry.is_dir() {
                if should_skip_dir_name(&name_lower) {
                    continue;
                }
                if depth < MAX_SIGNATURE_SEARCH_DEPTH {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }

            if !entry.is_file() {
                continue;
            }

            // Check if this is a target signature file
            if targets.contains(&name_lower) {
                result
                    .by_name_lower
                    .entry(name_lower.clone())
                    .or_default()
                    .push(path.clone());
                total_collected += 1;
            }

            // Check extension
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if extensions.contains(ext_lower.as_str()) {
                    result
                        .by_extension_lower
                        .entry(ext_lower)
                        .or_default()
                        .push(path);
                    total_collected += 1;
                }
            }
        }
    }

    result
}

/// Detect Rust frameworks using workspace.
#[cfg(any(test, feature = "test-utils"))]
fn detect_rust_with_workspace(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(cargo_files) = signatures.by_name_lower.get("cargo.toml") else {
        return;
    };

    for path in cargo_files {
        let Ok(content) = workspace.read(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        // Frameworks
        if content_lower.contains("actix-web") {
            push_unique(&mut results.frameworks, "Actix");
        }
        if content_lower.contains("axum") {
            push_unique(&mut results.frameworks, "Axum");
        }
        if content_lower.contains("rocket") {
            push_unique(&mut results.frameworks, "Rocket");
        }
        if content_lower.contains("tokio") {
            push_unique(&mut results.frameworks, "Tokio");
        }

        // Test framework
        if results.test_frameworks.is_empty() && content_lower.contains("[dev-dependencies]") {
            results.push_test_framework("cargo test");
        }

        // Package manager
        if results.package_managers.is_empty() {
            results.push_package_manager("Cargo");
        }
    }
}

/// Detect JavaScript/TypeScript frameworks using workspace.
#[cfg(any(test, feature = "test-utils"))]
fn detect_javascript_with_workspace(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(package_files) = signatures.by_name_lower.get("package.json") else {
        return;
    };

    for path in package_files {
        let Ok(content) = workspace.read(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        // Frameworks
        if content_lower.contains("\"react\"") {
            push_unique(&mut results.frameworks, "React");
        }
        if content_lower.contains("\"vue\"") {
            push_unique(&mut results.frameworks, "Vue");
        }
        if content_lower.contains("\"next\"") {
            push_unique(&mut results.frameworks, "Next.js");
        }
        if content_lower.contains("\"express\"") {
            push_unique(&mut results.frameworks, "Express");
        }

        // Test framework
        if results.test_frameworks.is_empty() {
            if content_lower.contains("\"jest\"") {
                results.push_test_framework("Jest");
            } else if content_lower.contains("\"vitest\"") {
                results.push_test_framework("Vitest");
            }
        }

        // Package manager
        if results.package_managers.is_empty() {
            results.push_package_manager("npm");
        }
    }

    // Check for pnpm/yarn lock files
    if signatures.by_name_lower.contains_key("pnpm-lock.yaml") {
        results.package_managers.clear();
        results.push_package_manager("pnpm");
    } else if signatures.by_name_lower.contains_key("yarn.lock") {
        results.package_managers.clear();
        results.push_package_manager("Yarn");
    } else if signatures.by_name_lower.contains_key("bun.lockb") {
        results.package_managers.clear();
        results.push_package_manager("Bun");
    }
}

/// Detect signature files and return frameworks, test framework, package manager.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn detect_signature_files_with_workspace(
    workspace: &dyn Workspace,
    root: &Path,
) -> (Vec<String>, Option<String>, Option<String>) {
    let signatures = collect_signature_files_with_workspace(workspace, root);
    let mut results = DetectionResults::new();

    detect_rust_with_workspace(workspace, &signatures, &mut results);
    detect_javascript_with_workspace(workspace, &signatures, &mut results);

    results.finish()
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;

    #[test]
    fn test_detect_rust_with_workspace() {
        let workspace = MemoryWorkspace::new_test().with_file(
            "Cargo.toml",
            r#"
[package]
name = "test"
[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
[dev-dependencies]
"#,
        );

        let (frameworks, test_fw, pkg_mgr) =
            detect_signature_files_with_workspace(&workspace, Path::new(""));

        assert!(frameworks.contains(&"Axum".to_string()));
        assert!(frameworks.contains(&"Tokio".to_string()));
        assert_eq!(test_fw, Some("cargo test".to_string()));
        assert_eq!(pkg_mgr, Some("Cargo".to_string()));
    }

    #[test]
    fn test_detect_javascript_with_workspace() {
        let workspace = MemoryWorkspace::new_test().with_file(
            "package.json",
            r#"
{
  "dependencies": { "react": "^18.0.0", "next": "^14.0.0" },
  "devDependencies": { "jest": "^29.0.0" }
}
"#,
        );

        let (frameworks, test_fw, pkg_mgr) =
            detect_signature_files_with_workspace(&workspace, Path::new(""));

        assert!(frameworks.contains(&"React".to_string()));
        assert!(frameworks.contains(&"Next.js".to_string()));
        assert_eq!(test_fw, Some("Jest".to_string()));
        assert_eq!(pkg_mgr, Some("npm".to_string()));
    }
}
