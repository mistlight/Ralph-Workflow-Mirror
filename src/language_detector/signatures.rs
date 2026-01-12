//! Signature file detection for frameworks and package managers.
//!
//! Analyzes configuration files like Cargo.toml, package.json, etc.
//! to detect frameworks, test frameworks, and package managers.

use super::scanner::{should_skip_dir_name, MAX_FILES_TO_SCAN, MAX_SIGNATURE_SEARCH_DEPTH};
use std::collections::{HashMap, VecDeque};
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
fn combine_unique(items: Vec<String>) -> Option<String> {
    match items.len() {
        0 => None,
        1 => Some(items[0].clone()),
        _ => Some(items.join(" + ")),
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
    let mut targets: HashMap<String, ()> = HashMap::new();
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
        let _ = targets.insert(name.to_string(), ());
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
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
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

            if targets.contains_key(&name_lower) {
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

/// Detect signature files that indicate specific frameworks or package managers.
///
/// Returns a tuple of (frameworks, test_framework, package_manager).
pub(super) fn detect_signature_files(root: &Path) -> (Vec<String>, Option<String>, Option<String>) {
    let signatures = collect_signature_files(root);

    let mut frameworks: Vec<String> = Vec::new();
    let mut test_frameworks: Vec<String> = Vec::new();
    let mut package_managers: Vec<String> = Vec::new();

    // Rust
    if let Some(paths) = signatures.by_name_lower.get("cargo.toml") {
        push_unique(&mut package_managers, "Cargo");

        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains("[dev-dependencies]")
                    || content_lower.contains("[[test]]")
                {
                    push_unique(&mut test_frameworks, "cargo test");
                }
                // Common Rust frameworks
                if content_lower.contains("actix") {
                    push_unique(&mut frameworks, "Actix");
                }
                if content_lower.contains("axum") {
                    push_unique(&mut frameworks, "Axum");
                }
                if content_lower.contains("rocket") {
                    push_unique(&mut frameworks, "Rocket");
                }
                if content_lower.contains("tokio") {
                    push_unique(&mut frameworks, "Tokio");
                }
                if content_lower.contains("warp") {
                    push_unique(&mut frameworks, "Warp");
                }
                if content_lower.contains("tauri") {
                    push_unique(&mut frameworks, "Tauri");
                }
                if content_lower.contains("leptos") {
                    push_unique(&mut frameworks, "Leptos");
                }
                if content_lower.contains("yew") {
                    push_unique(&mut frameworks, "Yew");
                }
            }
        }
    }

    // Python
    if let Some(paths) = signatures.by_name_lower.get("pyproject.toml") {
        push_unique(&mut package_managers, "Poetry/pip");
        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains("pytest") {
                    push_unique(&mut test_frameworks, "pytest");
                }
                if content_lower.contains("django") {
                    push_unique(&mut frameworks, "Django");
                }
                if content_lower.contains("fastapi") {
                    push_unique(&mut frameworks, "FastAPI");
                }
                if content_lower.contains("flask") {
                    push_unique(&mut frameworks, "Flask");
                }
            }
        }
    } else if let Some(paths) = signatures.by_name_lower.get("requirements.txt") {
        push_unique(&mut package_managers, "pip");
        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains("pytest") {
                    push_unique(&mut test_frameworks, "pytest");
                }
                if content_lower.contains("django") {
                    push_unique(&mut frameworks, "Django");
                }
                if content_lower.contains("fastapi") {
                    push_unique(&mut frameworks, "FastAPI");
                }
                if content_lower.contains("flask") {
                    push_unique(&mut frameworks, "Flask");
                }
            }
        }
    } else if signatures.by_name_lower.contains_key("setup.py") {
        push_unique(&mut package_managers, "setuptools");
    } else if signatures.by_name_lower.contains_key("pipfile") {
        push_unique(&mut package_managers, "Pipenv");
    }

    // JavaScript/TypeScript
    if let Some(paths) = signatures.by_name_lower.get("package.json") {
        for path in paths {
            let pkg_dir = path.parent().unwrap_or(root);
            if pkg_dir.join("pnpm-lock.yaml").exists() {
                push_unique(&mut package_managers, "pnpm");
            } else if pkg_dir.join("yarn.lock").exists() {
                push_unique(&mut package_managers, "yarn");
            } else {
                // Default to npm when package.json exists without pnpm or yarn locks
                push_unique(&mut package_managers, "npm");
            }

            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                // Test frameworks
                if content_lower.contains("\"jest\"") {
                    push_unique(&mut test_frameworks, "Jest");
                } else if content_lower.contains("\"vitest\"") {
                    push_unique(&mut test_frameworks, "Vitest");
                } else if content_lower.contains("\"mocha\"") {
                    push_unique(&mut test_frameworks, "Mocha");
                }

                // Frameworks
                if content_lower.contains("\"react\"") {
                    push_unique(&mut frameworks, "React");
                }
                if content_lower.contains("\"vue\"") {
                    push_unique(&mut frameworks, "Vue");
                }
                if content_lower.contains("\"svelte\"") {
                    push_unique(&mut frameworks, "Svelte");
                }
                if content_lower.contains("\"angular\"") || content_lower.contains("\"@angular") {
                    push_unique(&mut frameworks, "Angular");
                }
                if content_lower.contains("\"next\"") {
                    push_unique(&mut frameworks, "Next.js");
                }
                if content_lower.contains("\"nuxt\"") {
                    push_unique(&mut frameworks, "Nuxt");
                }
                if content_lower.contains("\"express\"") {
                    push_unique(&mut frameworks, "Express");
                }
                if content_lower.contains("\"fastify\"") {
                    push_unique(&mut frameworks, "Fastify");
                }
                if content_lower.contains("\"nest\"") || content_lower.contains("\"@nestjs") {
                    push_unique(&mut frameworks, "NestJS");
                }
                if content_lower.contains("\"electron\"") {
                    push_unique(&mut frameworks, "Electron");
                }
            }
        }
    }

    // Go
    if let Some(paths) = signatures.by_name_lower.get("go.mod") {
        push_unique(&mut package_managers, "Go modules");
        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains("gin-gonic/gin") {
                    push_unique(&mut frameworks, "Gin");
                }
                if content_lower.contains("go-chi/chi") {
                    push_unique(&mut frameworks, "Chi");
                }
                if content_lower.contains("gofiber/fiber") {
                    push_unique(&mut frameworks, "Fiber");
                }
                if content_lower.contains("labstack/echo") {
                    push_unique(&mut frameworks, "Echo");
                }
            }
        }
        // Go uses built-in testing
        push_unique(&mut test_frameworks, "go test");
    }

    // Ruby
    if let Some(paths) = signatures.by_name_lower.get("gemfile") {
        push_unique(&mut package_managers, "Bundler");
        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains("rspec") {
                    push_unique(&mut test_frameworks, "RSpec");
                } else if content_lower.contains("minitest") {
                    push_unique(&mut test_frameworks, "Minitest");
                }
                if content_lower.contains("rails") {
                    push_unique(&mut frameworks, "Rails");
                }
                if content_lower.contains("sinatra") {
                    push_unique(&mut frameworks, "Sinatra");
                }
            }
        }
    }

    // Java
    if let Some(paths) = signatures.by_name_lower.get("pom.xml") {
        push_unique(&mut package_managers, "Maven");
        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains("junit") {
                    push_unique(&mut test_frameworks, "JUnit");
                }
                if content_lower.contains("spring") {
                    push_unique(&mut frameworks, "Spring");
                }
            }
        }
    } else if signatures.by_name_lower.contains_key("build.gradle")
        || signatures.by_name_lower.contains_key("build.gradle.kts")
    {
        push_unique(&mut package_managers, "Gradle");
        let paths = signatures
            .by_name_lower
            .get("build.gradle.kts")
            .or_else(|| signatures.by_name_lower.get("build.gradle"));
        if let Some(paths) = paths {
            for path in paths {
                if let Ok(content) = fs::read_to_string(path) {
                    let content_lower = content.to_lowercase();
                    if content_lower.contains("junit") {
                        push_unique(&mut test_frameworks, "JUnit");
                    }
                    if content_lower.contains("spring") {
                        push_unique(&mut frameworks, "Spring");
                    }
                }
            }
        }
    }

    // PHP
    if let Some(paths) = signatures.by_name_lower.get("composer.json") {
        push_unique(&mut package_managers, "Composer");
        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains("phpunit") {
                    push_unique(&mut test_frameworks, "PHPUnit");
                }
                if content_lower.contains("laravel") {
                    push_unique(&mut frameworks, "Laravel");
                }
                if content_lower.contains("symfony") {
                    push_unique(&mut frameworks, "Symfony");
                }
            }
        }
    }

    // .NET / C#
    if signatures.by_extension_lower.contains_key("csproj") {
        push_unique(&mut package_managers, "NuGet");
    }

    // Elixir
    if let Some(paths) = signatures.by_name_lower.get("mix.exs") {
        push_unique(&mut package_managers, "Mix");
        push_unique(&mut test_frameworks, "ExUnit");
        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains(":phoenix") {
                    push_unique(&mut frameworks, "Phoenix");
                }
            }
        }
    }

    // Dart/Flutter
    if let Some(paths) = signatures.by_name_lower.get("pubspec.yaml") {
        push_unique(&mut package_managers, "pub");
        for path in paths {
            if let Ok(content) = fs::read_to_string(path) {
                let content_lower = content.to_lowercase();
                if content_lower.contains("flutter") {
                    push_unique(&mut frameworks, "Flutter");
                }
                if content_lower.contains("test:") {
                    push_unique(&mut test_frameworks, "dart test");
                }
            }
        }
    }

    (
        frameworks,
        combine_unique(test_frameworks),
        combine_unique(package_managers),
    )
}
