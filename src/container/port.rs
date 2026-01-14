//! Port forwarding management for container services
//!
//! This module provides dynamic port forwarding capabilities so that services
//! started inside containers (e.g., `rails server on port 3000`) are accessible
//! on the host via localhost.

/// Port mapping configuration
///
/// Defines how a container port is mapped to a host port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMapping {
    /// Port inside the container
    pub container_port: u16,
    /// Port on the host (can be 0 for auto-allocation)
    pub host_port: u16,
    /// Protocol (tcp or udp)
    pub protocol: PortProtocol,
}

impl PortMapping {
    /// Create a new port mapping with explicit host port
    pub const fn new(container_port: u16, host_port: u16) -> Self {
        Self {
            container_port,
            host_port,
            protocol: PortProtocol::Tcp,
        }
    }

    /// Create a port mapping with auto-allocated host port
    ///
    /// The container runtime will assign an available port.
    pub const fn auto_allocate(container_port: u16) -> Self {
        Self {
            container_port,
            host_port: 0, // 0 means auto-allocate
            protocol: PortProtocol::Tcp,
        }
    }

    /// Get the Docker/Podman publish flag string
    pub fn to_publish_flag(&self) -> String {
        if self.host_port == 0 {
            // Auto-allocate: -p 3000 (container port only)
            format!("{}", self.container_port)
        } else {
            // Explicit mapping: -p 3000:3000
            format!("{}:{}", self.host_port, self.container_port)
        }
    }
}

/// Port protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortProtocol {
    /// TCP protocol
    Tcp,
}

/// Detect ports that a command might use
///
/// Analyzes a command string to detect common development servers
/// and returns the ports they typically use.
pub fn detect_ports_from_command(command: &[String]) -> Vec<u16> {
    let cmd_str = command.join(" ").to_lowercase();
    let mut ports = Vec::new();

    // Rails server
    if cmd_str.contains("rails server") || cmd_str.contains("rails s") {
        ports.push(3000);
    }

    // Next.js / Vite
    if cmd_str.contains("next dev") || cmd_str.contains("vite") {
        ports.push(3000);
    }

    // Django
    if cmd_str.contains("django") || cmd_str.contains("manage.py runserver") {
        ports.push(8000);
    }

    // Flask
    if cmd_str.contains("flask run") {
        ports.push(5000);
    }

    // Phoenix
    if cmd_str.contains("phx.server") || cmd_str.contains("mix phx.server") {
        ports.push(4000);
    }

    // Jetty / Java app servers
    if cmd_str.contains("jetty") || cmd_str.contains("mvn spring-boot:run") {
        ports.push(8080);
    }

    // Go live reload
    if cmd_str.contains("air") || cmd_str.contains("realize") {
        ports.push(3000);
    }

    // General HTTP server patterns
    if cmd_str.contains("python -m http.server") || cmd_str.contains("python3 -m http.server") {
        // Extract port from command like "python -m http.server 8080"
        if let Some(pos) = cmd_str.find("http.server") {
            let after = &cmd_str[pos + "http.server".len()..];
            if let Some(port_str) = after.split_whitespace().next() {
                if let Ok(port) = port_str.parse::<u16>() {
                    ports.push(port);
                }
            }
        } else {
            ports.push(8000); // Default
        }
    }

    // Netcat listeners
    if cmd_str.contains("nc -l") || cmd_str.contains("netcat -l") {
        // Try to extract port
        for (i, arg) in command.iter().enumerate() {
            if arg == "-l" && i + 1 < command.len() {
                if let Ok(port) = command[i + 1].parse::<u16>() {
                    ports.push(port);
                }
            }
        }
    }

    ports.sort_unstable();
    ports.dedup();
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_mapping_new() {
        let mapping = PortMapping::new(3000, 3000);
        assert_eq!(mapping.container_port, 3000);
        assert_eq!(mapping.host_port, 3000);
        assert_eq!(mapping.protocol, PortProtocol::Tcp);
    }

    #[test]
    fn test_port_mapping_auto_allocate() {
        let mapping = PortMapping::auto_allocate(3000);
        assert_eq!(mapping.container_port, 3000);
        assert_eq!(mapping.host_port, 0);
    }

    #[test]
    fn test_port_mapping_to_publish_flag() {
        let explicit = PortMapping::new(3000, 8080);
        assert_eq!(explicit.to_publish_flag(), "8080:3000");

        let auto = PortMapping::auto_allocate(3000);
        assert_eq!(auto.to_publish_flag(), "3000");
    }

    #[test]
    fn test_detect_ports_from_command_rails() {
        let cmd = vec![
            "bundle".to_string(),
            "exec".to_string(),
            "rails".to_string(),
            "server".to_string(),
        ];
        let ports = detect_ports_from_command(&cmd);
        assert_eq!(ports, vec![3000]);
    }

    #[test]
    fn test_detect_ports_from_command_django() {
        let cmd = vec![
            "python".to_string(),
            "manage.py".to_string(),
            "runserver".to_string(),
        ];
        let ports = detect_ports_from_command(&cmd);
        assert_eq!(ports, vec![8000]);
    }

    #[test]
    fn test_detect_ports_from_command_flask() {
        let cmd = vec!["flask".to_string(), "run".to_string()];
        let ports = detect_ports_from_command(&cmd);
        assert_eq!(ports, vec![5000]);
    }

    #[test]
    fn test_detect_ports_from_command_http_server() {
        let cmd = vec![
            "python".to_string(),
            "-m".to_string(),
            "http.server".to_string(),
            "8080".to_string(),
        ];
        let ports = detect_ports_from_command(&cmd);
        assert_eq!(ports, vec![8080]);
    }
}
