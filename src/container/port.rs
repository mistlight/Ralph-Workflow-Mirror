//! Port forwarding management for container services
//!
//! This module provides dynamic port forwarding capabilities so that services
//! started inside containers (e.g., `rails server on port 3000`) are accessible
//! on the host via localhost.

use crate::container::error::{ContainerError, ContainerResult};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

/// Default port range for dynamic allocation
const PORT_RANGE_START: u16 = 3000;
const PORT_RANGE_END: u16 = 9000;

/// Host address to bind published ports to
/// Using 127.0.0.1 ensures ports are only accessible on the local machine
const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

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
    pub fn new(container_port: u16, host_port: u16) -> Self {
        Self {
            container_port,
            host_port,
            protocol: PortProtocol::Tcp,
        }
    }

    /// Create a port mapping with auto-allocated host port
    ///
    /// The container runtime will assign an available port.
    pub fn auto_allocate(container_port: u16) -> Self {
        Self {
            container_port,
            host_port: 0, // 0 means auto-allocate
            protocol: PortProtocol::Tcp,
        }
    }

    /// Create a new port mapping with protocol
    pub fn with_protocol(container_port: u16, host_port: u16, protocol: PortProtocol) -> Self {
        Self {
            container_port,
            host_port,
            protocol,
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
    /// UDP protocol
    Udp,
}

impl PortProtocol {
    /// Get the protocol string for container run flags
    pub fn as_str(&self) -> &str {
        match self {
            PortProtocol::Tcp => "tcp",
            PortProtocol::Udp => "udp",
        }
    }
}

/// Port manager for tracking and allocating ports
///
/// Ensures ports are not reused across containers and provides
/// dynamic allocation when needed.
#[derive(Debug, Clone)]
pub struct PortManager {
    /// Currently allocated ports
    allocated_ports: Arc<Mutex<HashSet<u16>>>,
    /// Port mappings for the current execution
    mappings: Vec<PortMapping>,
}

impl PortManager {
    /// Create a new port manager
    pub fn new() -> Self {
        Self {
            allocated_ports: Arc::new(Mutex::new(HashSet::new())),
            mappings: Vec::new(),
        }
    }

    /// Add a port mapping
    ///
    /// If the host port is 0, an available port will be allocated.
    pub fn add_mapping(&mut self, mapping: PortMapping) -> ContainerResult<()> {
        let mut allocated = self.allocated_ports.lock().map_err(|e| {
            ContainerError::Other(format!("Failed to acquire port lock: {}", e))
        })?;

        if mapping.host_port != 0 {
            // Check if port is already allocated
            if allocated.contains(&mapping.host_port) {
                return Err(ContainerError::Other(format!(
                    "Host port {} is already in use",
                    mapping.host_port
                )));
            }
            allocated.insert(mapping.host_port);
        }

        self.mappings.push(mapping);
        Ok(())
    }

    /// Publish all commonly used development ports automatically
    ///
    /// This publishes ports for common development servers:
    /// - 3000-3010 (Rails, Next.js, Vite, etc.)
    /// - 4000-4010 (Phoenix, Django, etc.)
    /// - 5000-5010 (Flask, etc.)
    /// - 8000-8010 (Django, etc.)
    /// - 8080-8090 (Jetty, etc.)
    pub fn publish_common_ports(&mut self) -> ContainerResult<()> {
        let common_ranges = [(3000, 3010), (4000, 4010), (5000, 5010), (8000, 8010), (8080, 8090)];

        for (start, end) in common_ranges {
            for port in start..=end {
                self.add_mapping(PortMapping::auto_allocate(port))?;
            }
        }

        Ok(())
    }

    /// Publish a specific container port
    pub fn publish_port(&mut self, container_port: u16) -> ContainerResult<()> {
        self.add_mapping(PortMapping::auto_allocate(container_port))
    }

    /// Publish a specific container port with explicit host port
    pub fn publish_port_explicit(&mut self, container_port: u16, host_port: u16) -> ContainerResult<()> {
        self.add_mapping(PortMapping::new(container_port, host_port))
    }

    /// Get all port mappings
    pub fn mappings(&self) -> &[PortMapping] {
        &self.mappings
    }

    /// Get the Docker/Podman publish flags
    ///
    /// Returns a list of `-p` flags suitable for container run commands.
    pub fn publish_flags(&self) -> Vec<String> {
        self.mappings
            .iter()
            .map(|m| format!("-p={}", m.to_publish_flag()))
            .collect()
    }

    /// Check if a specific container port is published
    pub fn is_published(&self, container_port: u16) -> bool {
        self.mappings
            .iter()
            .any(|m| m.container_port == container_port)
    }

    /// Clear all port mappings
    pub fn clear(&mut self) {
        self.mappings.clear();
        if let Ok(mut allocated) = self.allocated_ports.lock() {
            allocated.clear();
        }
    }

    /// Get the number of port mappings
    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    /// Check if there are any port mappings
    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }
}

impl Default for PortManager {
    fn default() -> Self {
        Self::new()
    }
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

    ports.sort();
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
    fn test_port_protocol_as_str() {
        assert_eq!(PortProtocol::Tcp.as_str(), "tcp");
        assert_eq!(PortProtocol::Udp.as_str(), "udp");
    }

    #[test]
    fn test_port_manager_new() {
        let manager = PortManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_port_manager_add_mapping() {
        let mut manager = PortManager::new();
        manager.add_mapping(PortMapping::new(3000, 8080)).unwrap();
        assert_eq!(manager.len(), 1);
        assert!(manager.is_published(3000));
    }

    #[test]
    fn test_port_manager_duplicate_port() {
        let mut manager = PortManager::new();
        manager.add_mapping(PortMapping::new(3000, 8080)).unwrap();
        let result = manager.add_mapping(PortMapping::new(4000, 8080));
        assert!(result.is_err());
    }

    #[test]
    fn test_port_manager_publish_flags() {
        let mut manager = PortManager::new();
        manager.add_mapping(PortMapping::new(3000, 8080)).unwrap();
        manager.add_mapping(PortMapping::auto_allocate(4000)).unwrap();

        let flags = manager.publish_flags();
        assert!(flags.contains(&"-p=8080:3000".to_string()));
        assert!(flags.contains(&"-p=4000".to_string()));
    }

    #[test]
    fn test_port_manager_clear() {
        let mut manager = PortManager::new();
        manager.add_mapping(PortMapping::new(3000, 8080)).unwrap();
        assert_eq!(manager.len(), 1);

        manager.clear();
        assert!(manager.is_empty());
    }

    #[test]
    fn test_detect_ports_from_command_rails() {
        let cmd = vec!["bundle".to_string(), "exec".to_string(), "rails".to_string(), "server".to_string()];
        let ports = detect_ports_from_command(&cmd);
        assert_eq!(ports, vec![3000]);
    }

    #[test]
    fn test_detect_ports_from_command_django() {
        let cmd = vec!["python".to_string(), "manage.py".to_string(), "runserver".to_string()];
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
        let cmd = vec!["python".to_string(), "-m".to_string(), "http.server".to_string(), "8080".to_string()];
        let ports = detect_ports_from_command(&cmd);
        assert_eq!(ports, vec![8080]);
    }
}
