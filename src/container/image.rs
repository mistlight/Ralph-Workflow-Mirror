//! Container image management and selection

use std::path::Path;

use crate::container::engine::EngineType;
use crate::container::error::{ContainerError, ContainerResult};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Result of building a container image
#[derive(Debug, Clone)]
pub struct BuildResult {
    /// The tag of the built image
    pub image_tag: String,
    /// Path to the Dockerfile that was used
    pub dockerfile_path: PathBuf,
    /// Base image that was used
    pub base_image: String,
}

/// Container image selection based on project stack
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ContainerImage {
    /// Auto-detect based on project stack
    #[default]
    Auto,
}

impl ContainerImage {
    /// Detect the appropriate container image based on project stack
    fn detect_image_for_stack(stack: Option<&str>) -> String {
        match stack {
            Some("rust" | "rust-lang") => "rust:latest".to_string(),
            Some("node" | "javascript" | "typescript") => "node:20".to_string(),
            Some("python") => "python:3.12".to_string(),
            Some("ruby" | "ruby-on-rails") => "ruby:latest".to_string(),
            Some("go" | "golang") => "golang:latest".to_string(),
            Some("java" | "kotlin") => "eclipse-temurin:21".to_string(),
            Some("php") => "php:latest".to_string(),
            _ => "ubuntu:24.04".to_string(), // Generic fallback
        }
    }

    /// Generate a Dockerfile for the Ralph agent container
    fn generate_dockerfile(base_image: &str, stack: Option<&str>) -> String {
        let mut content = format!(
            r#"# Ralph Agent Container
# Auto-generated Dockerfile for running AI agents in isolation

FROM {base_image}

# Set working directory
WORKDIR /workspace

# Install common development tools
RUN apt-get update && apt-get install -y \
    git \
    curl \
    wget \
    vim \
    sudo \
    build-essential \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user for running the agent
RUN useradd -m -s /bin/bash ralph && \
    echo "ralph ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Install language-specific tools based on detected stack
"#,
        );

        // Add language-specific installations
        match stack {
            Some("rust" | "rust-lang") => {
                content.push_str(
                    r"# Rust tools are already installed in the rust image
RUN cargo install cargo-edit 2>/dev/null || true
",
                );
            }
            Some("node" | "javascript" | "typescript") => {
                content.push_str(
                    r"# Node.js tools - npm and npx are already available
RUN npm install -g typescript @types/node 2>/dev/null || true
",
                );
            }
            Some("python") => {
                content.push_str(
                    r"# Python tools - pip is already available
RUN pip install --upgrade pip && \
    pip install black pylint mypy pytest 2>/dev/null || true
",
                );
            }
            Some("ruby" | "ruby-on-rails") => {
                content.push_str(
                    r"# Ruby tools - gem is already available
RUN gem install bundler rubocop rspec rubocop-rails 2>/dev/null || true
",
                );
            }
            Some("go" | "golang") => {
                content.push_str(
                    r"# Go tools are already installed in the golang image
RUN go install golang.org/x/tools/cmd/goimports@latest 2>/dev/null || true
",
                );
            }
            _ => {
                // Generic: add some common tools
                content.push_str(
                    r"# Generic tools available in most images
",
                );
            }
        }

        // Finish the Dockerfile
        content.push_str(
            r#"# Set the user
USER ralph

# Set the default command
CMD ["/bin/bash"]
"#,
        );

        content
    }

    /// Build a container image from a Dockerfile
    ///
    /// Creates a Dockerfile for the detected project stack and builds it.
    pub fn build_ralph_image(
        repo_path: &Path,
        tag: &str,
        engine_type: EngineType,
    ) -> ContainerResult<BuildResult> {
        let stack = detect_project_stack(repo_path);
        let base_image = Self::detect_image_for_stack(stack.as_deref());

        // Generate Dockerfile content
        let dockerfile_content = Self::generate_dockerfile(&base_image, stack.as_deref());

        // Write Dockerfile to .agent directory
        let agent_dir = repo_path.join(".agent");
        fs::create_dir_all(&agent_dir).map_err(|e| {
            ContainerError::Other(format!("Failed to create .agent directory: {e}"))
        })?;

        let dockerfile_path = agent_dir.join("Dockerfile.ralph-agent");
        fs::write(&dockerfile_path, dockerfile_content)
            .map_err(|e| ContainerError::Other(format!("Failed to write Dockerfile: {e}")))?;

        // Build the image
        let binary = engine_type.binary_name();
        let output = Command::new(binary)
            .args([
                "build",
                "-t",
                tag,
                "-f",
                dockerfile_path.to_str().unwrap(),
                ".",
            ])
            .current_dir(repo_path)
            .output()
            .map_err(|e| ContainerError::Other(format!("Failed to execute build command: {e}")))?;

        if !output.status.success() {
            return Err(ContainerError::Other(format!(
                "Failed to build image: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(BuildResult {
            image_tag: tag.to_string(),
            dockerfile_path,
            base_image,
        })
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

#[cfg(test)]
/// Ensure the container image exists locally
///
/// Pulls the image if it's not already present.
pub fn ensure_image_exists(image: &str, engine_type: EngineType) -> Result<(), String> {
    let binary = engine_type.binary_name();

    // Check if image exists locally
    let check_output = Command::new(binary).args(["images", "-q", image]).output();

    let needs_pull = match check_output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        Err(_) => true,
    };

    if needs_pull {
        // Pull the image
        let pull_output = Command::new(binary)
            .args(["pull", image])
            .output()
            .map_err(|e| format!("Failed to pull image: {e}"))?;

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
    fn test_container_image_default() {
        let image = ContainerImage::default();
        assert_eq!(image, ContainerImage::Auto);
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

    #[test]
    fn test_ensure_image_exists_with_valid_image() {
        // Test with a minimal image that's likely available
        let result = ensure_image_exists("alpine:latest", EngineType::Auto);
        // We don't assert success since docker might not be available
        // Just verify the function doesn't panic
        let _ = result;
    }

    #[test]
    fn test_detect_project_stack() {
        use std::fs;
        let temp = std::env::temp_dir();

        // Test Rust project detection
        let rust_repo = temp.join("test-rust-stack");
        fs::create_dir_all(&rust_repo).ok();
        fs::write(rust_repo.join("Cargo.toml"), "[package]\nname = \"test\"").ok();
        let stack = detect_project_stack(&rust_repo);
        assert_eq!(stack, Some("rust".to_string()));
        fs::remove_dir_all(&rust_repo).ok();

        // Test empty directory
        let empty_repo = temp.join("test-empty-stack");
        fs::create_dir_all(&empty_repo).ok();
        let stack = detect_project_stack(&empty_repo);
        assert_eq!(stack, None);
        fs::remove_dir_all(&empty_repo).ok();
    }
}
