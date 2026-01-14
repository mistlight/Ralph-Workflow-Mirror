//! Network configuration for containers

#[cfg(test)]
mod tests {
    /// Network mode for containers
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    enum NetworkMode {
        /// Enable network access (default, needed for API calls)
        #[default]
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
        const fn is_enabled(&self) -> bool {
            !matches!(self, Self::Disabled)
        }

        /// Get the Docker/Podman network flag value
        const fn as_flag(&self) -> Option<&'static str> {
            match self {
                Self::Enabled => None, // Default, no flag needed
                Self::Disabled => Some("none"),
                Self::Bridge => Some("bridge"),
                Self::Host => Some("host"),
            }
        }
    }

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
