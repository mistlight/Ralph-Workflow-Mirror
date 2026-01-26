//! Signature file detection for frameworks and package managers.
//!
//! Analyzes configuration files like Cargo.toml, package.json, etc.
//! to detect frameworks, test frameworks, and package managers.

use super::scanner::{should_skip_dir_name, MAX_SIGNATURE_SEARCH_DEPTH};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::workspace::Workspace;

/// Maximum number of files to scan
const MAX_FILES_TO_SCAN: usize = 2000;

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
}

/// Collect signature files using workspace.
fn collect_signature_files_with_workspace(
    workspace: &dyn Workspace,
    root: &Path,
) -> SignatureFiles {
    let targets: HashSet<&str> = [
        "cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "pipfile",
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
        "gemfile",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "composer.json",
        "mix.exs",
        "pubspec.yaml",
    ]
    .into_iter()
    .collect();

    let mut result = SignatureFiles::default();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    let mut scanned_entries: usize = 0;
    let mut collected: usize = 0;

    while let Some((dir, depth)) = queue.pop_front() {
        if scanned_entries >= MAX_FILES_TO_SCAN || collected >= MAX_SIGNATURE_FILES {
            break;
        }

        let Ok(entries) = workspace.read_dir(&dir) else {
            continue;
        };

        for entry in entries {
            if scanned_entries >= MAX_FILES_TO_SCAN || collected >= MAX_SIGNATURE_FILES {
                break;
            }
            scanned_entries += 1;

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
            if targets.contains(name_lower.as_str()) {
                result
                    .by_name_lower
                    .entry(name_lower)
                    .or_default()
                    .push(path);
                collected += 1;
            }
        }
    }

    result
}

/// Detection results accumulator.
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

/// Detect Rust frameworks.
fn detect_rust(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(cargo_files) = signatures.by_name_lower.get("cargo.toml") else {
        return;
    };

    results.push_package_manager("Cargo");

    for path in cargo_files {
        let Ok(content) = workspace.read(path) else {
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

/// Detect Python frameworks.
fn detect_python(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let paths = if let Some(p) = signatures.by_name_lower.get("pyproject.toml") {
        results.push_package_manager("Poetry/pip");
        Some(p)
    } else if let Some(p) = signatures.by_name_lower.get("requirements.txt") {
        results.push_package_manager("pip");
        Some(p)
    } else if signatures.by_name_lower.contains_key("setup.py") {
        results.push_package_manager("setuptools");
        None
    } else if signatures.by_name_lower.contains_key("pipfile") {
        results.push_package_manager("Pipenv");
        None
    } else {
        None
    };

    if let Some(paths) = paths {
        for path in paths {
            let Ok(content) = workspace.read(path) else {
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
}

/// Detect JavaScript/TypeScript frameworks.
fn detect_javascript(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(paths) = signatures.by_name_lower.get("package.json") else {
        return;
    };

    // Check for package manager lock files
    if signatures.by_name_lower.contains_key("pnpm-lock.yaml") {
        results.push_package_manager("pnpm");
    } else if signatures.by_name_lower.contains_key("yarn.lock") {
        results.push_package_manager("Yarn");
    } else if signatures.by_name_lower.contains_key("bun.lockb") {
        results.push_package_manager("Bun");
    } else {
        results.push_package_manager("npm");
    }

    for path in paths {
        let Ok(content) = workspace.read(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        // Test frameworks
        for (pattern, name) in [
            ("\"jest\"", "Jest"),
            ("\"vitest\"", "Vitest"),
            ("\"mocha\"", "Mocha"),
            ("\"cypress\"", "Cypress"),
            ("\"playwright\"", "Playwright"),
        ] {
            if content_lower.contains(pattern) {
                results.push_test_framework(name);
            }
        }

        // Frameworks
        for (pattern, name) in [
            ("\"react\"", "React"),
            ("\"vue\"", "Vue"),
            ("\"angular\"", "Angular"),
            ("\"svelte\"", "Svelte"),
            ("\"next\"", "Next.js"),
            ("\"nuxt\"", "Nuxt"),
            ("\"express\"", "Express"),
            ("\"fastify\"", "Fastify"),
            ("\"nestjs\"", "NestJS"),
            ("\"gatsby\"", "Gatsby"),
        ] {
            if content_lower.contains(pattern) {
                results.push_framework(name);
            }
        }
    }
}

/// Detect Go frameworks.
fn detect_go(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(paths) = signatures.by_name_lower.get("go.mod") else {
        return;
    };

    results.push_package_manager("Go Modules");
    results.push_test_framework("go test");

    for path in paths {
        let Ok(content) = workspace.read(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        for (pattern, name) in [
            ("gin-gonic/gin", "Gin"),
            ("labstack/echo", "Echo"),
            ("gofiber/fiber", "Fiber"),
            ("gorilla/mux", "Gorilla"),
            ("go-chi/chi", "Chi"),
        ] {
            if content_lower.contains(pattern) {
                results.push_framework(name);
            }
        }
    }
}

/// Detect Ruby frameworks.
fn detect_ruby(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(paths) = signatures.by_name_lower.get("gemfile") else {
        return;
    };

    results.push_package_manager("Bundler");

    for path in paths {
        let Ok(content) = workspace.read(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("rspec") {
            results.push_test_framework("RSpec");
        } else if content_lower.contains("minitest") {
            results.push_test_framework("Minitest");
        }

        if content_lower.contains("rails") {
            results.push_framework("Rails");
        } else if content_lower.contains("sinatra") {
            results.push_framework("Sinatra");
        }
    }
}

/// Detect Java frameworks.
fn detect_java(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    // Maven
    if let Some(paths) = signatures.by_name_lower.get("pom.xml") {
        results.push_package_manager("Maven");
        detect_java_frameworks(workspace, paths, results);
    }

    // Gradle
    let gradle_paths: Vec<_> = signatures
        .by_name_lower
        .get("build.gradle")
        .into_iter()
        .chain(signatures.by_name_lower.get("build.gradle.kts"))
        .flatten()
        .collect();

    if !gradle_paths.is_empty() {
        results.push_package_manager("Gradle");
        detect_java_frameworks(workspace, &gradle_paths, results);
    }
}

fn detect_java_frameworks(
    workspace: &dyn Workspace,
    paths: &[impl AsRef<Path>],
    results: &mut DetectionResults,
) {
    for path in paths {
        let Ok(content) = workspace.read(path.as_ref()) else {
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

/// Detect PHP frameworks.
fn detect_php(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(paths) = signatures.by_name_lower.get("composer.json") else {
        return;
    };

    results.push_package_manager("Composer");

    for path in paths {
        let Ok(content) = workspace.read(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("phpunit") {
            results.push_test_framework("PHPUnit");
        }

        for (pattern, name) in [("laravel", "Laravel"), ("symfony", "Symfony")] {
            if content_lower.contains(pattern) {
                results.push_framework(name);
            }
        }
    }
}

/// Detect .NET frameworks.
fn detect_dotnet(signatures: &SignatureFiles, results: &mut DetectionResults) {
    if signatures
        .by_name_lower
        .keys()
        .any(|k| k.ends_with(".csproj") || k.ends_with(".fsproj"))
    {
        results.push_package_manager("NuGet");
    }
}

/// Detect Elixir frameworks.
fn detect_elixir(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(paths) = signatures.by_name_lower.get("mix.exs") else {
        return;
    };

    results.push_package_manager("Mix");
    results.push_test_framework("ExUnit");

    for path in paths {
        let Ok(content) = workspace.read(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("phoenix") {
            results.push_framework("Phoenix");
        }
    }
}

/// Detect Dart/Flutter frameworks.
fn detect_dart(
    workspace: &dyn Workspace,
    signatures: &SignatureFiles,
    results: &mut DetectionResults,
) {
    let Some(paths) = signatures.by_name_lower.get("pubspec.yaml") else {
        return;
    };

    results.push_package_manager("Pub");

    for path in paths {
        let Ok(content) = workspace.read(path) else {
            continue;
        };
        let content_lower = content.to_lowercase();

        if content_lower.contains("flutter:") || content_lower.contains("flutter_test") {
            results.push_framework("Flutter");
            results.push_test_framework("Flutter Test");
        }
    }
}

/// Detect signature files and return frameworks, test framework, package manager.
pub(super) fn detect_signature_files_with_workspace(
    workspace: &dyn Workspace,
    root: &Path,
) -> (Vec<String>, Option<String>, Option<String>) {
    let signatures = collect_signature_files_with_workspace(workspace, root);
    let mut results = DetectionResults::new();

    detect_rust(workspace, &signatures, &mut results);
    detect_python(workspace, &signatures, &mut results);
    detect_javascript(workspace, &signatures, &mut results);
    detect_go(workspace, &signatures, &mut results);
    detect_ruby(workspace, &signatures, &mut results);
    detect_java(workspace, &signatures, &mut results);
    detect_php(workspace, &signatures, &mut results);
    detect_dotnet(&signatures, &mut results);
    detect_elixir(workspace, &signatures, &mut results);
    detect_dart(workspace, &signatures, &mut results);

    results.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;

    #[test]
    fn test_detect_rust() {
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
    fn test_detect_javascript() {
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

    #[test]
    fn test_detect_python() {
        let workspace = MemoryWorkspace::new_test().with_file(
            "pyproject.toml",
            r#"
[project]
name = "test"
dependencies = ["django", "pytest"]
"#,
        );

        let (frameworks, test_fw, pkg_mgr) =
            detect_signature_files_with_workspace(&workspace, Path::new(""));

        assert!(frameworks.contains(&"Django".to_string()));
        assert_eq!(test_fw, Some("pytest".to_string()));
        assert_eq!(pkg_mgr, Some("Poetry/pip".to_string()));
    }

    #[test]
    fn test_detect_go() {
        let workspace = MemoryWorkspace::new_test().with_file(
            "go.mod",
            "module example.com/test\n\ngo 1.21\n\nrequire github.com/gin-gonic/gin v1.9.0\n",
        );

        let (frameworks, test_fw, pkg_mgr) =
            detect_signature_files_with_workspace(&workspace, Path::new(""));

        assert!(frameworks.contains(&"Gin".to_string()));
        assert_eq!(test_fw, Some("go test".to_string()));
        assert_eq!(pkg_mgr, Some("Go Modules".to_string()));
    }

    #[test]
    fn test_detect_ruby() {
        let workspace = MemoryWorkspace::new_test().with_file(
            "Gemfile",
            r#"
source 'https://rubygems.org'
gem 'rails', '~> 7.0'
gem 'rspec-rails', group: :test
"#,
        );

        let (frameworks, test_fw, pkg_mgr) =
            detect_signature_files_with_workspace(&workspace, Path::new(""));

        assert!(frameworks.contains(&"Rails".to_string()));
        assert_eq!(test_fw, Some("RSpec".to_string()));
        assert_eq!(pkg_mgr, Some("Bundler".to_string()));
    }
}
