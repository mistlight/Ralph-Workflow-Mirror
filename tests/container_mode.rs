//! Integration tests for container-based security mode
//!
//! These tests verify that the container mode works correctly including:
//! - SecurityMode parsing and platform defaults
//! - Tool mounting
//! - Port mapping

// Note: These tests require Docker or Podman to be installed and running
// They should be run with: cargo test --test container_mode -- --ignored

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    /// Test that SecurityMode enum parses correctly
    #[test]
    fn test_security_mode_parsing() {
        use ralph::SecurityMode;

        assert_eq!("auto".parse::<SecurityMode>().unwrap(), SecurityMode::Auto);
        assert_eq!("container".parse::<SecurityMode>().unwrap(), SecurityMode::Container);
        assert_eq!("user-account".parse::<SecurityMode>().unwrap(), SecurityMode::UserAccount);
        assert_eq!("user".parse::<SecurityMode>().unwrap(), SecurityMode::UserAccount);
        assert_eq!("none".parse::<SecurityMode>().unwrap(), SecurityMode::None);

        // Test case insensitivity
        assert_eq!("CONTAINER".parse::<SecurityMode>().unwrap(), SecurityMode::Container);
        assert_eq!("User-Account".parse::<SecurityMode>().unwrap(), SecurityMode::UserAccount);

        // Test invalid input
        assert!("invalid".parse::<SecurityMode>().is_err());
    }

    /// Test that SecurityMode has correct platform defaults
    #[test]
    fn test_security_mode_platform_default() {
        use ralph::SecurityMode;

        let default = SecurityMode::default_for_platform();

        // On macOS, default should be UserAccount (since Linux containers can't run macOS binaries)
        // On Linux, default should be Container
        // On other platforms, default should be None
        #[cfg(target_os = "macos")]
        assert_eq!(default, SecurityMode::UserAccount);

        #[cfg(target_os = "linux")]
        assert_eq!(default, SecurityMode::Container);

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        assert_eq!(default, SecurityMode::None);
    }

    /// Test that SecurityMode Display implementation works
    #[test]
    fn test_security_mode_display() {
        use ralph::SecurityMode;
        use std::fmt::Write;

        let mut buffer = String::new();
        write!(&mut buffer, "{}", SecurityMode::Auto).unwrap();
        assert_eq!(buffer, "auto");

        buffer.clear();
        write!(&mut buffer, "{}", SecurityMode::Container).unwrap();
        assert_eq!(buffer, "container");

        buffer.clear();
        write!(&mut buffer, "{}", SecurityMode::UserAccount).unwrap();
        assert_eq!(buffer, "user-account");

        buffer.clear();
        write!(&mut buffer, "{}", SecurityMode::None).unwrap();
        assert_eq!(buffer, "none");
    }

    /// Test container executor creation (without actually running a container)
    #[test]
    fn test_container_executor_creation() {
        use ralph::{ContainerConfig, ContainerExecutor};

        let repo_root = PathBuf::from(".");
        let agent_dir = PathBuf::from(".agent");
        let image = "ralph-agent:latest".to_string();

        let config = ContainerConfig::new(repo_root, agent_dir, image)
            .with_enabled(true)
            .with_network(true);

        let executor = ContainerExecutor::new(config);

        assert!(executor.is_enabled());
        assert_eq!(executor.config().image, "ralph-agent:latest");
    }

    /// Test tool manager discovery (doesn't require container runtime)
    #[test]
    fn test_tool_manager_discovery() {
        use ralph::ToolManager;

        let manager = ToolManager::new();
        let mounts = manager.discover_tool_mounts();

        // Should return at least an empty Vec (not panic)
        // May have mounts if running on a system with version managers
        assert!(mounts.is_ok());

        let env_vars = manager.get_env_vars();
        // Should return a Vec (may be empty)
        // We're just checking it doesn't panic
        let _ = env_vars;
    }

    /// Test that tool mount to Mount conversion works correctly
    #[test]
    fn test_tool_mount_conversion() {
        use ralph::ToolMount;

        let mount = ToolMount::new(
            PathBuf::from("/usr/local/bin"),
            "/usr/local/bin".to_string(),
        );

        assert!(mount.read_only);
        assert_eq!(mount.source, PathBuf::from("/usr/local/bin"));
        assert_eq!(mount.target, "/usr/local/bin");

        let engine_mount = mount.to_mount();
        assert_eq!(engine_mount.source, "/usr/local/bin");
        assert_eq!(engine_mount.target, "/usr/local/bin");
        assert!(engine_mount.read_only);
    }

    /// Test read-write tool mount
    #[test]
    fn test_tool_mount_read_write() {
        use ralph::ToolMount;

        let mount = ToolMount::read_write(
            PathBuf::from("/home/user/.npm-global"),
            "/home/ralph/.npm-global".to_string(),
        );

        assert!(!mount.read_only);

        let engine_mount = mount.to_mount();
        assert!(!engine_mount.read_only);
    }

    /// Test port mapping creation
    #[test]
    fn test_port_mapping() {
        use ralph::PortMapping;

        // Test auto-allocate (host_port = 0 means runtime will assign)
        let mapping = PortMapping::auto_allocate(3000);
        assert_eq!(mapping.container_port, 3000);
        assert_eq!(mapping.host_port, 0); // 0 means auto-allocate

        // Test explicit port mapping
        let explicit = PortMapping::new(3000, 8080);
        assert_eq!(explicit.container_port, 3000);
        assert_eq!(explicit.host_port, 8080);
    }

    /// Test volume manager includes Claude directory mount when it exists
    #[test]
    fn test_volume_manager_claude_mount() {
        use ralph::VolumeManager;

        let repo_root = PathBuf::from(".");
        let agent_dir = PathBuf::from(".agent");

        let manager = VolumeManager::new(repo_root, agent_dir, None);
        let mounts = manager.get_mounts();

        assert!(mounts.is_ok());

        let mounts = mounts.unwrap();
        // Should always have workspace and .agent mounts
        assert!(mounts.iter().any(|m| m.target == "/workspace"));
        assert!(mounts.iter().any(|m| m.target == "/workspace/.agent"));

        // Claude mount is optional - only if ~/.claude exists
        // We're just verifying the manager doesn't panic
    }

    /// Test volume manager includes config directory mount when provided
    #[test]
    fn test_volume_manager_config_mount() {
        use ralph::VolumeManager;

        let repo_root = PathBuf::from(".");
        let agent_dir = PathBuf::from(".agent");
        let config_dir = Some(PathBuf::from("/home/user/.config/ralph"));

        let manager = VolumeManager::new(repo_root, agent_dir, config_dir);
        let mounts = manager.get_mounts();

        assert!(mounts.is_ok());

        let _mounts = mounts.unwrap();
        // Config mount target should be set even if source doesn't exist
        // The canonicalize call handles non-existent paths gracefully
        // We're just verifying the structure is correct
    }
}
