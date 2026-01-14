//! Network configuration for containers

/// Network mode for containers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    /// Enable network access (default, needed for API calls)
    Enabled,
    /// Disable network access (air-gapped mode)
    Disabled,
    /// Bridge mode (default Docker networking)
    Bridge,
    /// Host mode (use host network stack - less isolation)
    Host,
}

impl NetworkMode {
    /// Check if network is enabled in this mode
    pub fn is_enabled(&self) -> bool {
        !matches!(self, NetworkMode::Disabled)
    }

    /// Get the Docker/Podman network flag value
    pub fn as_flag(&self) -> Option<&'static str> {
        match self {
            NetworkMode::Enabled => None, // Default, no flag needed
            NetworkMode::Disabled => Some("none"),
            NetworkMode::Bridge => Some("bridge"),
            NetworkMode::Host => Some("host"),
        }
    }
}

impl Default for NetworkMode {
    fn default() -> Self {
        NetworkMode::Enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_mode_enabled() {
        assert!(NetworkMode::Enabled.is_enabled());
        assert!(NetworkMode::Bridge.is_enabled());
        assert!(NetworkMode::Host.is_enabled());
        assert!(!NetworkMode::Disabled.is_enabled());
    }

    #[test]
    fn test_network_mode_as_flag() {
        assert_eq!(NetworkMode::Enabled.as_flag(), None);
        assert_eq!(NetworkMode::Disabled.as_flag(), Some("none"));
        assert_eq!(NetworkMode::Bridge.as_flag(), Some("bridge"));
        assert_eq!(NetworkMode::Host.as_flag(), Some("host"));
    }

    #[test]
    fn test_network_mode_default() {
        assert_eq!(NetworkMode::default(), NetworkMode::Enabled);
    }
}
