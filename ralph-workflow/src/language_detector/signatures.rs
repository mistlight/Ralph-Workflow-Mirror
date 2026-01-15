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
