//! Container image management and selection

use crate::container::engine::EngineType;
use std::path::Path;

/// Container image selection based on project stack
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerImage {
    /// Use a specific image
    Specific(String),
    /// Auto-detect based on project stack
    Auto,
}

impl ContainerImage {
    /// Get the image name to use
    pub fn resolve(&self, project_stack: Option<&str>) -> String {
        match self {
            ContainerImage::Specific(image) => image.clone(),
            ContainerImage::Auto => {
                // Detect appropriate image based on project stack
                Self::detect_image_for_stack(project_stack)
            }
        }
    }

    /// Detect the appropriate container image based on project stack
    fn detect_image_for_stack(stack: Option<&str>) -> String {
        match stack {
            Some("rust") | Some("rust-lang") => "rust:latest".to_string(),
            Some("node") | Some("javascript") | Some("typescript") => {
                "node:20".to_string()
            }
            Some("python") => "python:3.12".to_string(),
            Some("ruby") | Some("ruby-on-rails") => "ruby:latest".to_string(),
            Some("go") | Some("golang") => "golang:latest".to_string(),
            Some("java") | Some("kotlin") => "eclipse-temurin:21".to_string(),
            Some("php") => "php:latest".to_string(),
            _ => "ubuntu:24.04".to_string(), // Generic fallback
        }
    }

    /// Create a specific image
    pub fn specific(image: String) -> Self {
        Self::Specific(image)
    }

    /// Create an auto-detecting image selector
    pub fn auto() -> Self {
        Self::Auto
    }
}

impl Default for ContainerImage {
    fn default() -> Self {
        Self::Auto
    }
}

/// Detect project stack from the repository
///
/// This is a simple heuristic-based detection that looks for common files.
pub fn detect_project_stack(repo_path: &Path) -> Option<String> {
    // Check for Rust
    if repo_path.join("Cargo.toml").exists() {
        return Some("rust".to_string());
    }

    // Check for Node.js
    if repo_path.join("package.json").exists() {
        return Some("node".to_string());
    }

    // Check for Python
    if repo_path.join("pyproject.toml").exists()
        || repo_path.join("requirements.txt").exists()
        || repo_path.join("setup.py").exists()
    {
        return Some("python".to_string());
    }

    // Check for Ruby
    if repo_path.join("Gemfile").exists() {
        return Some("ruby".to_string());
    }

    // Check for Go
    if repo_path.join("go.mod").exists() {
        return Some("go".to_string());
    }

    // Check for Java/Kotlin (Maven or Gradle)
    if repo_path.join("pom.xml").exists()
        || repo_path.join("build.gradle").exists()
        || repo_path.join("build.gradle.kts").exists()
    {
        return Some("java".to_string());
    }

    // Check for PHP
    if repo_path.join("composer.json").exists() {
        return Some("php".to_string());
    }

    None
}

/// Ensure the container image exists locally
///
/// Pulls the image if it's not already present.
pub fn ensure_image_exists(
    image: &str,
    engine_type: EngineType,
) -> Result<(), String> {
    let binary = engine_type.binary_name();

    // Check if image exists locally
    let check_output = std::process::Command::new(binary)
        .args(["images", "-q", image])
        .output();

    let needs_pull = match check_output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        Err(_) => true,
    };

    if needs_pull {
        // Pull the image
        let pull_output = std::process::Command::new(binary)
            .args(["pull", image])
            .output()
            .map_err(|e| format!("Failed to pull image: {}", e))?;

        if !pull_output.status.success() {
            return Err(format!(
                "Failed to pull image {}: {}",
                image,
                String::from_utf8_lossy(&pull_output.stderr)
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_image_specific() {
        let image = ContainerImage::specific("ubuntu:24.04".to_string());
        assert_eq!(image.resolve(None), "ubuntu:24.04");
    }

    #[test]
    fn test_container_image_auto() {
        let image = ContainerImage::auto();
        assert_eq!(image.resolve(None), "ubuntu:24.04"); // Default
        assert_eq!(image.resolve(Some("rust")), "rust:latest");
        assert_eq!(image.resolve(Some("node")), "node:20");
        assert_eq!(image.resolve(Some("python")), "python:3.12");
    }

    #[test]
    fn test_detect_image_for_stack() {
        assert_eq!(
            ContainerImage::detect_image_for_stack(Some("rust")),
            "rust:latest"
        );
        assert_eq!(
            ContainerImage::detect_image_for_stack(Some("javascript")),
            "node:20"
        );
        assert_eq!(
            ContainerImage::detect_image_for_stack(Some("python")),
            "python:3.12"
        );
    }
}
