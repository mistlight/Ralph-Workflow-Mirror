//! Security-related CLI commands
//!
//! This module handles commands for setting up and checking security modes.

use crate::config::Config;
use crate::container::{ContainerEngine, EngineType, SecurityMode};
use crate::git_helpers::get_repo_root;
use crate::logger::{Colors, Logger};

/// Handle the --setup-security command
///
/// Sets up the user account for user-account security mode.
pub fn handle_setup_security(colors: Colors) -> anyhow::Result<()> {
    println!("{}", colors.bold());
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║                        Ralph Security Setup                              ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!("{}", colors.reset());

    println!("\nThis will set up a dedicated user account for running Ralph agents.");
    println!("This provides isolation while maintaining access to development tools.\n");

    // Check if we're running on macOS
    #[cfg(target_os = "macos")]
    {
        println!("{}macOS detected{}", colors.cyan(), colors.reset());
        println!("On macOS, user-account mode provides:");
        println!("  • Isolated user profile (~/ralph-agent)");
        println!("  • Separate environment variables");
        println!("  • Controlled access to system resources");
        println!("  • Compatible with Homebrew and language version managers");
    }

    #[cfg(target_os = "linux")]
    {
        println!("{}Linux detected{}", colors.cyan(), colors.reset());
        println!("On Linux, user-account mode provides:");
        println!("  • User namespace isolation");
        println!("  • Separate filesystem permissions");
        println!("  • Controlled sudo access");
        println!("  • Compatible with development toolchains");
    }

    println!("\n{}Actions that will be taken:{}",
           colors.yellow(), colors.reset());
    println!("  1. Create user 'ralph-agent' with home directory");
    println!("  2. Add user to sudoers (NOPASSWD for package management)");
    println!("  3. Set up basic shell configuration");

    // Check if user already exists
    let check_result = std::process::Command::new("id")
        .arg("ralph-agent")
        .output();

    let user_exists = check_result.map(|o| o.status.success()).unwrap_or(false);

    if user_exists {
        println!("\n{}Note: User 'ralph-agent' already exists.{}", colors.dim(), colors.reset());
        println!("Setup will skip user creation and focus on configuration.");
    }

    println!("\n{}{}Continue?{} [y/N] ", colors.bold(), colors.yellow(), colors.reset());
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    if !input.trim().to_lowercase().starts_with('y') {
        println!("Setup cancelled.");
        return Ok(());
    }

    // Run setup with sudo
    println!("\n{}Running setup (sudo required)...{}", colors.bold(), colors.reset());

    let status = std::process::Command::new("sudo")
        .args(["/bin/bash", "-c"])
        .arg(format!(
            r#"# Create user if doesn't exist
if ! id ralph-agent &>/dev/null; then
    useradd -m -s /bin/bash ralph-agent
    echo "Created user 'ralph-agent'"
else
    echo "User 'ralph-agent' already exists"
fi

# Add to sudoers with NOPASSWD for specific commands
SUDOERS_LINE="ralph-agent ALL=(ALL) NOPASSWD: ALL"
if ! sudo grep -q "$SUDOERS_LINE" /etc/sudoers.d/ralph-agent 2>/dev/null; then
    echo "$SUDOERS_LINE" | sudo tee /etc/sudoers.d/ralph-agent >/dev/null
    sudo chmod 440 /etc/sudoers.d/ralph-agent
    echo "Added sudoers configuration"
else
    echo "Sudoers configuration already exists"
fi

# Create .profile for environment
sudo -u ralph-agent bash -c 'cat > ~/.profile << "EOF"
# Ralph Agent Environment
export PATH="$HOME/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

# Preserve language manager paths
if [ -d "$HOME/.rbenv/bin" ]; then
    export PATH="$HOME/.rbenv/bin:$PATH"
    eval "$(rbenv init - bash 2>/dev/null)" || true
fi

if [ -f "$HOME/.nvm/nvm.sh" ]; then
    source "$HOME/.nvm/nvm.sh"
fi

# asdf version manager
if [ -d "$HOME/.asdf" ]; then
    export ASDF_DATA_DIR="$HOME/.asdf"
    export ASDF_DIR="$HOME/.asdf"
    . "$ASDF_DIR/asdf.sh"
fi

# mise version manager (formerly rtx)
if command -v mise &> /dev/null; then
    export MISE_DATA_DIR="$HOME/.mise"
    export MISE_SHELL=bash
    eval "$(mise activate bash 2>/dev/null)" || true
fi

# pyenv version manager
if [ -d "$HOME/.pyenv" ]; then
    export PYENV_ROOT="$HOME/.pyenv"
    export PATH="$PYENV_ROOT/bin:$PATH"
    eval "$(pyenv init - bash 2>/dev/null)" || true
fi

# fnm (Fast Node Manager)
if [ -d "$HOME/.fnm" ]; then
    export FNM_DIR="$HOME/.fnm"
    eval "$(fnm env --use-on-cd 2>/dev/null)" || true
fi

# RVM (Ruby Version Manager)
if [ -f "$HOME/.rvm/scripts/rvm" ]; then
    export RVM_HOME="$HOME/.rvm"
    source "$RVM_HOME/scripts/rvm"
fi
EOF
'
echo "Created shell configuration"

# Create symbolic links to version manager directories from the main user
# This allows the ralph-agent user to access the same versions as the main user
MAIN_USER_HOME=$(getent passwd "$(logname)" | cut -d: -f6)
echo ""
echo "Setting up version manager links..."

# Link rbenv
if [ -d "$MAIN_USER_HOME/.rbenv" ] && [ ! -d "/home/ralph-agent/.rbenv" ]; then
    sudo ln -sf "$MAIN_USER_HOME/.rbenv" /home/ralph-agent/.rbenv
    echo "  Linked .rbenv"
fi

# Link RVM
if [ -d "$MAIN_USER_HOME/.rvm" ] && [ ! -d "/home/ralph-agent/.rvm" ]; then
    sudo ln -sf "$MAIN_USER_HOME/.rvm" /home/ralph-agent/.rvm
    echo "  Linked .rvm"
fi

# Link nvm
if [ -d "$MAIN_USER_HOME/.nvm" ] && [ ! -d "/home/ralph-agent/.nvm" ]; then
    sudo ln -sf "$MAIN_USER_HOME/.nvm" /home/ralph-agent/.nvm
    echo "  Linked .nvm"
fi

# Link pyenv
if [ -d "$MAIN_USER_HOME/.pyenv" ] && [ ! -d "/home/ralph-agent/.pyenv" ]; then
    sudo ln -sf "$MAIN_USER_HOME/.pyenv" /home/ralph-agent/.pyenv
    echo "  Linked .pyenv"
fi

# Link asdf
if [ -d "$MAIN_USER_HOME/.asdf" ] && [ ! -d "/home/ralph-agent/.asdf" ]; then
    sudo ln -sf "$MAIN_USER_HOME/.asdf" /home/ralph-agent/.asdf
    echo "  Linked .asdf"
fi

# Link mise
if [ -d "$MAIN_USER_HOME/.mise" ] && [ ! -d "/home/ralph-agent/.mise" ]; then
    sudo ln -sf "$MAIN_USER_HOME/.mise" /home/ralph-agent/.mise
    echo "  Linked .mise"
fi

# Link fnm
if [ -d "$MAIN_USER_HOME/.fnm" ] && [ ! -d "/home/ralph-agent/.fnm" ]; then
    sudo ln -sf "$MAIN_USER_HOME/.fnm" /home/ralph-agent/.fnm
    echo "  Linked .fnm"
fi

# Link cargo (Rust)
if [ -d "$MAIN_USER_HOME/.cargo" ] && [ ! -d "/home/ralph-agent/.cargo" ]; then
    sudo ln -sf "$MAIN_USER_HOME/.cargo" /home/ralph-agent/.cargo
    echo "  Linked .cargo"
fi

echo "Created shell configuration"

echo ""
echo "✓ Setup complete!"
"#
        ))
        .status()?;

    if status.success() {
        println!("{}✓ User account setup complete!{}", colors.green(), colors.reset());
        println!("\n{}Next steps:{}",
               colors.bold(), colors.reset());
        println!("  1. Set security mode: {}--security-mode user-account{}",
                 colors.cyan(), colors.reset());
        println!("  2. Or set environment: {}RALPH_SECURITY_MODE=user-account{}",
                 colors.cyan(), colors.reset());
        println!("\n{}Usage:{}",
               colors.bold(), colors.reset());
        println!("  Agents will now run as the 'ralph-agent' user,");
        println!("  providing isolation from your main user account.");
    } else {
        println!("{}✗ Setup failed. Please check the output above.{}",
                 colors.red(), colors.reset());
        anyhow::bail!("Setup command failed");
    }

    Ok(())
}

/// Handle the --security-check command
///
/// Checks and reports the status of security mode configuration.
pub fn handle_security_check(
    colors: Colors,
    config: &Config,
    _logger: &mut Logger,
) -> anyhow::Result<()> {
    println!("{}", colors.bold());
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║                      Ralph Security Check                                ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!("{}", colors.reset());

    // Determine effective security mode
    let security_mode_str = config.security_mode.as_deref().unwrap_or("auto");
    let security_mode: SecurityMode = match security_mode_str.parse() {
        Ok(mode) => mode,
        Err(_) => {
            println!("{}Invalid security mode: '{}'{}",
                     colors.red(), security_mode_str, colors.reset());
            println!("Valid options: auto, container, user-account, none");
            return Ok(());
        }
    };

    let resolved_mode = match security_mode {
        SecurityMode::Auto => SecurityMode::default_for_platform(),
        other => other,
    };

    println!("\n{}Configuration:{}",
           colors.bold(), colors.reset());
    println!("  Security mode: {}{}{}",
             colors.cyan(),
             security_mode_str,
             colors.reset());
    println!("  Resolved to: {}{}{}",
             colors.cyan(),
             match resolved_mode {
                 SecurityMode::Auto => "auto",
                 SecurityMode::Container => "container",
                 SecurityMode::UserAccount => "user-account",
                 SecurityMode::None => "none",
             },
             colors.reset());
    println!("  Container mode: {}{}{}",
             if config.container_mode { colors.green() } else { colors.dim() },
             if config.container_mode { "enabled" } else { "disabled" },
             colors.reset());
    println!("  Container engine: {}{}{}",
             colors.cyan(),
             config.container_engine.as_deref().unwrap_or("auto"),
             colors.reset());

    // Check container mode
    println!("\n{}Container Mode Status:{}",
           colors.bold(), colors.reset());

    if matches!(resolved_mode, SecurityMode::Container) || config.container_mode {
        let engine_type = match config.container_engine.as_deref() {
            Some("docker") => EngineType::Docker,
            Some("podman") => EngineType::Podman,
            _ => EngineType::Auto,
        };

        match ContainerEngine::detect(engine_type) {
            Ok(engine) => {
                println!("  {}✓ Container engine detected:{} {}",
                         colors.green(),
                         colors.reset(),
                         engine.binary());

                // Check if image exists
                if let Some(ref image) = config.container_image {
                    let check = std::process::Command::new(engine.binary())
                        .args(["images", "-q", image])
                        .output();

                    let exists = check.map(|o| !o.stdout.is_empty()).unwrap_or(false);
                    if exists {
                        println!("  {}✓ Container image available:{} {}",
                                 colors.green(),
                                 colors.reset(),
                                 image);
                    } else {
                        println!("  {}⚠ Container image not found:{} {} (will be pulled on first use)",
                                 colors.yellow(),
                                 colors.reset(),
                                 image);
                    }
                }
            }
            Err(e) => {
                println!("  {}✗ No container engine available{}",
                         colors.red(),
                         colors.reset());
                println!("    Error: {}", e);
                println!("    Install Docker or Podman to use container mode");
            }
        }
    } else {
        println!("  {}Not configured{}", colors.dim(), colors.reset());
    }

    // Check user account mode
    println!("\n{}User Account Mode Status:{}",
           colors.bold(), colors.reset());

    let check_result = std::process::Command::new("id")
        .arg("ralph-agent")
        .output();

    let user_exists = check_result.map(|o| o.status.success()).unwrap_or(false);

    if user_exists {
        println!("  {}✓ User 'ralph-agent' exists{}",
                 colors.green(),
                 colors.reset());

        // Check if user can access the repository
        if let Ok(repo_root) = get_repo_root() {
            let has_access = std::process::Command::new("sudo")
                .args(["-u", "ralph-agent", "test", "-r", &repo_root.to_string_lossy()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if has_access {
                println!("  {}✓ User can access repository{}",
                         colors.green(),
                         colors.reset());
            } else {
                println!("  {}⚠ User cannot access repository{}",
                         colors.yellow(),
                         colors.reset());
                println!("    Run: {}sudo chmod +rx {}{}",
                         colors.cyan(),
                         repo_root.display(),
                         colors.reset());
            }
        }
    } else {
        println!("  {}⚠ User 'ralph-agent' does not exist{}",
                 colors.yellow(),
                 colors.reset());
        println!("    Run: {}ralph --setup-security{}",
                 colors.cyan(),
                 colors.reset());
    }

    // Check environment variables
    println!("\n{}Environment Variables:{}",
           colors.bold(), colors.reset());

    let env_vars = vec![
        ("RALPH_SECURITY_MODE", "Security mode override"),
        ("RALPH_CONTAINER_MODE", "Enable container mode"),
        ("RALPH_CONTAINER_ENGINE", "Container engine (docker/podman)"),
        ("RALPH_CONTAINER_IMAGE", "Container image"),
    ];

    for (var, _description) in env_vars {
        if let Ok(value) = std::env::var(var) {
            println!("  {}{}={}\"{}\"{}",
                     colors.green(),
                     var,
                     colors.reset(),
                     value,
                     colors.reset());
        } else {
            println!("  {}{}=(not set){}",
                     colors.dim(),
                     var,
                     colors.reset());
        }
    }

    // Summary
    println!("\n{}Summary:{}",
           colors.bold(), colors.reset());

    let is_ready = match resolved_mode {
        SecurityMode::Container => {
            config.container_mode &&
                ContainerEngine::detect(
                    config.container_engine.as_deref()
                        .map(|s| if s == "docker" { EngineType::Docker } else { EngineType::Podman })
                        .unwrap_or(EngineType::Auto)
                ).is_ok()
        }
        SecurityMode::UserAccount => user_exists,
        SecurityMode::Auto | SecurityMode::None => true,
    };

    if is_ready {
        println!("  {}✓ Security mode is ready to use{}",
                 colors.green(),
                 colors.reset());
    } else {
        println!("  {}⚠ Security mode needs setup{}",
                 colors.yellow(),
                 colors.reset());
        println!("\n{}Recommended actions:{}",
                 colors.bold(), colors.reset());

        match resolved_mode {
            SecurityMode::Container => {
                println!("  • Install Docker or Podman");
                println!("  • Or set: {}RALPH_SECURITY_MODE=user-account{}",
                         colors.cyan(),
                         colors.reset());
            }
            SecurityMode::UserAccount => {
                println!("  • Run: {}ralph --setup-security{}",
                         colors.cyan(),
                         colors.reset());
            }
            _ => {}
        }
    }

    Ok(())
}

/// Handle the --build-image command
///
/// Builds a container image for container mode.
#[cfg(feature = "build-image")]
pub fn handle_build_image(tag: Option<String>, colors: Colors) -> anyhow::Result<()> {
    use crate::container::image::ContainerImage;

    println!("{}", colors.bold());
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║                     Ralph Container Image Builder                          ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!("{}", colors.reset());

    // Get repository root
    let repo_root = get_repo_root()?;

    // Detect container engine
    let engine = ContainerEngine::detect(EngineType::Auto)?;

    println!("\n{}Configuration:{}",
           colors.bold(), colors.reset());
    println!("  Engine: {}{}{}",
             colors.cyan(),
             engine.binary(),
             colors.reset());
    println!("  Repository: {}{}{}",
             colors.cyan(),
             repo_root.display(),
             colors.reset());

    // Determine tag
    let tag = tag.unwrap_or_else(|| "ralph-agent:latest".to_string());
    println!("  Tag: {}{}{}",
             colors.cyan(),
             tag,
             colors.reset());

    // Detect project stack
    use crate::container::image::detect_project_stack;
    let stack = detect_project_stack(&repo_root);

    if let Some(ref s) = stack {
        println!("  Detected stack: {}{}{}",
                 colors.green(),
                 s,
                 colors.reset());
    } else {
        println!("  Detected stack: {}generic{}",
                 colors.dim(),
                 colors.reset());
    }

    println!("\n{}Building container image...{}",
             colors.bold(),
             colors.reset());

    match ContainerImage::build_ralph_image(&repo_root, &tag, engine.engine_type()) {
        Ok(result) => {
            println!("{}✓ Build successful!{}",
                     colors.green(),
                     colors.reset());

            println!("\n{}Build Details:{}",
                     colors.bold(), colors.reset());
            println!("  Image tag: {}{}{}",
                     colors.cyan(),
                     result.image_tag,
                     colors.reset());
            println!("  Base image: {}{}{}",
                     colors.cyan(),
                     result.base_image,
                     colors.reset());
            println!("  Dockerfile: {}{}{}",
                     colors.cyan(),
                     result.dockerfile_path.display(),
                     colors.reset());

            println!("\n{}Next steps:{}",
                     colors.bold(), colors.reset());
            println!("  1. Use the image: {}--container-image {}{}",
                     colors.cyan(),
                     result.image_tag,
                     colors.reset());
            println!("  2. Or set env var: {}RALPH_CONTAINER_IMAGE={}{}",
                     colors.cyan(),
                     result.image_tag,
                     colors.reset());

            Ok(())
        }
        Err(e) => {
            println!("{}✗ Build failed: {}{}",
                     colors.red(),
                     e,
                     colors.reset());
            anyhow::bail!("Container image build failed: {}", e);
        }
    }
}
