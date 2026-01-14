//! Tests for language detection module.

use super::*;
use std::fs::{self, File};
use std::path::Path;
use tempfile::TempDir;

fn create_test_file(dir: &Path, name: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    File::create(path).unwrap();
}

// Extension mapping tests
#[test]
fn extension_to_language_covers_common_languages() {
    assert_eq!(extension_to_language("rs"), Some("Rust"));
    assert_eq!(extension_to_language("py"), Some("Python"));
    assert_eq!(extension_to_language("js"), Some("JavaScript"));
    assert_eq!(extension_to_language("ts"), Some("TypeScript"));
    assert_eq!(extension_to_language("go"), Some("Go"));
    assert_eq!(extension_to_language("java"), Some("Java"));
    assert_eq!(extension_to_language("rb"), Some("Ruby"));
    assert_eq!(extension_to_language("php"), Some("PHP"));
    assert_eq!(extension_to_language("yml"), Some("YAML"));
    assert_eq!(extension_to_language("yaml"), Some("YAML"));
    assert_eq!(extension_to_language("json"), Some("JSON"));
    assert_eq!(extension_to_language("md"), None);
}

#[test]
fn extension_matching_is_case_insensitive() {
    assert_eq!(extension_to_language("RS"), Some("Rust"));
    assert_eq!(extension_to_language("Py"), Some("Python"));
    assert_eq!(extension_to_language("JS"), Some("JavaScript"));
}

// Stack detection tests
#[test]
fn primary_language_prefers_code_over_config() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Many config/markup files and a single Rust file.
    for i in 0..10 {
        create_test_file(root, &format!("config/{i}.yml"));
    }
    create_test_file(root, "src/main.rs");

    let stack = detect_stack(root).unwrap();
    assert_eq!(stack.primary_language, "Rust");
    assert!(stack.secondary_languages.contains(&"YAML".to_string()));
}

#[test]
fn rust_project_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
tokio = "1.0"
"#,
    )
    .unwrap();

    create_test_file(root, "src/main.rs");
    create_test_file(root, "tests/integration.rs");

    let stack = detect_stack(root).unwrap();
    assert_eq!(stack.primary_language, "Rust");
    assert!(stack.frameworks.contains(&"Tokio".to_string()));
    assert_eq!(stack.package_manager, Some("Cargo".to_string()));
    assert!(stack.has_tests);
}

#[test]
fn python_project_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(
        root.join("pyproject.toml"),
        r#"
[project]
name = "test"
[project.dependencies]
django = "*"
"#,
    )
    .unwrap();
    create_test_file(root, "app/main.py");
    create_test_file(root, "tests/test_app.py");

    let stack = detect_stack(root).unwrap();
    assert_eq!(stack.primary_language, "Python");
    assert!(stack.frameworks.contains(&"Django".to_string()));
    assert!(stack.has_tests);
}

#[test]
fn react_project_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(
        root.join("package.json"),
        r#"{"name":"test","dependencies":{"react":"^18.0.0"},"devDependencies":{"jest":"^29.0.0"}}"#,
    )
    .unwrap();
    create_test_file(root, "src/App.tsx");
    create_test_file(root, "src/index.ts");

    let stack = detect_stack(root).unwrap();
    assert!(stack.is_javascript_or_typescript());
    assert!(stack.frameworks.contains(&"React".to_string()));
    assert_eq!(stack.package_manager, Some("npm".to_string()));
    assert_eq!(stack.test_framework, Some("Jest".to_string()));
}

#[test]
fn go_project_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(
        root.join("go.mod"),
        "module example.com/test\n\ngo 1.21\n\nrequire github.com/gin-gonic/gin v1.9.0\n",
    )
    .unwrap();
    create_test_file(root, "main.go");
    create_test_file(root, "handlers/api.go");

    let stack = detect_stack(root).unwrap();
    assert_eq!(stack.primary_language, "Go");
    assert!(stack.frameworks.contains(&"Gin".to_string()));
}

#[test]
fn monorepo_multiple_packages_detects_primary_language_by_prevalence() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Backend in Go.
    fs::create_dir_all(root.join("backend")).unwrap();
    fs::write(
        root.join("backend/go.mod"),
        "module example.com/backend\n\ngo 1.21",
    )
    .unwrap();
    create_test_file(root, "backend/main.go");

    // Frontend in TypeScript (more files => primary).
    fs::create_dir_all(root.join("frontend")).unwrap();
    fs::write(root.join("frontend/package.json"), r#"{"name":"frontend"}"#).unwrap();
    create_test_file(root, "frontend/src/App.tsx");
    create_test_file(root, "frontend/src/index.ts");
    create_test_file(root, "frontend/src/utils.ts");

    // Shared scripts in Python.
    create_test_file(root, "scripts/deploy.py");

    let stack = detect_stack(root).unwrap();
    assert_eq!(stack.primary_language, "TypeScript");
    assert!(stack.secondary_languages.contains(&"Go".to_string()));
    assert!(stack.secondary_languages.contains(&"Python".to_string()));
}

#[test]
fn ignores_node_modules_and_target_like_directories() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // A real JS file.
    create_test_file(root, "src/index.js");

    // Many files in node_modules should be ignored.
    for i in 0..50 {
        create_test_file(root, &format!("node_modules/pkg{i}/index.js"));
    }

    // Many files in target should be ignored.
    for i in 0..50 {
        create_test_file(root, &format!("target/build{i}/main.rs"));
    }

    let stack = detect_stack(root).unwrap();
    assert_eq!(stack.primary_language, "JavaScript");
}
