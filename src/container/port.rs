//! Port forwarding management for container services
//!
//! This module provides dynamic port forwarding capabilities so that services
//! started inside containers (e.g., `rails server on port 3000`) are accessible
//! on the host via localhost.

/// Port mapping configuration
///
/// Defines how a container port is mapped to a host port.
/// Uses auto-allocation by default (`host_port=0`) to avoid conflicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMapping {
    /// Port inside the container
    pub container_port: u16,
    /// Port on the host (0 for auto-allocation)
    pub host_port: u16,
}

impl PortMapping {
    /// Create a port mapping with auto-allocated host port
    ///
    /// The container runtime will assign an available port.
    pub const fn auto_allocate(container_port: u16) -> Self {
        Self {
            container_port,
            host_port: 0, // 0 means auto-allocate
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

/// Detect ports that a command might use
///
/// Analyzes a command string to detect common development servers
/// and returns the ports they typically use.
pub fn detect_ports_from_command(command: &[String]) -> Vec<u16> {
    let cmd_str = command.join(" ").to_lowercase();
    let mut ports = Vec::new();

    // First, check for explicit port arguments
    // Common port argument patterns: -p, --port, -b, --bind, etc.
    for (i, arg) in command.iter().enumerate() {
        let arg_lower = arg.to_lowercase();
        // Handle --port 3000 or -p 3000
        if arg_lower == "-p"
            || arg_lower == "--port"
            || arg_lower == "-b"
            || arg_lower == "--bind-port"
        {
            if i + 1 < command.len() {
                if let Ok(port) = command[i + 1].parse::<u16>() {
                    ports.push(port);
                }
            }
        }
        // Handle --port=3000 or -p3000 style
        else if arg_lower.starts_with("--port=") || arg_lower.starts_with("-p") {
            let port_str = arg
                .strip_prefix("--port=")
                .or_else(|| arg.strip_prefix("-p"))
                .or_else(|| arg.strip_prefix("--bind-port="))
                .unwrap_or("");
            if let Ok(port) = port_str.parse::<u16>() {
                ports.push(port);
            }
        }
    }

    // Rails server
    if cmd_str.contains("rails server") || cmd_str.contains("rails s") {
        ports.push(3000);
    }

    // Next.js / Vite
    if cmd_str.contains("next dev") || cmd_str.contains("next dev") {
        ports.push(3000);
    }
    // Vite specifically (defaults to 5173)
    if cmd_str.contains("vite") || cmd_str.contains("npm run dev") || cmd_str.contains("yarn dev") {
        ports.push(5173);
        ports.push(3000); // Also common for Vite
    }

    // Django
    if cmd_str.contains("django") || cmd_str.contains("manage.py runserver") {
        ports.push(8000);
    }

    // Flask
    if cmd_str.contains("flask run") {
        ports.push(5000);
    }

    // Phoenix (Elixir)
    if cmd_str.contains("phx.server") || cmd_str.contains("mix phx.server") {
        ports.push(4000);
    }

    // Jetty / Java app servers
    if cmd_str.contains("jetty") || cmd_str.contains("mvn spring-boot:run") {
        ports.push(8080);
    }

    // Gradle bootRun
    if cmd_str.contains("gradle bootrun") || cmd_str.contains("gradlew bootrun") {
        ports.push(8080);
    }

    // Go live reload (air, realize, reflex)
    if cmd_str.contains("air") || cmd_str.contains("realize") || cmd_str.contains("reflex") {
        ports.push(3000);
    }

    // Parcel (JavaScript bundler)
    if cmd_str.contains("parcel") {
        ports.push(1234);
    }

    // Webpack dev server
    if cmd_str.contains("webpack dev server") || cmd_str.contains("webpack serve") {
        ports.push(8080);
    }

    // Angular CLI
    if cmd_str.contains("ng serve") {
        ports.push(4200);
    }

    // React Native
    if cmd_str.contains("react-native start") || cmd_str.contains("rnpm start") {
        ports.push(8081);
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

    // Rust act-web, warp, etc.
    if cmd_str.contains("cargo run") {
        // Common default ports for Rust web frameworks
        ports.push(8080);
        ports.push(3000);
    }

    ports.sort_unstable();
    ports.dedup();
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_mapping_auto_allocate() {
        let mapping = PortMapping::auto_allocate(3000);
        assert_eq!(mapping.container_port, 3000);
        assert_eq!(mapping.host_port, 0);
    }

    #[test]
    fn test_port_mapping_to_publish_flag() {
        let auto = PortMapping::auto_allocate(3000);
        assert_eq!(auto.to_publish_flag(), "3000");

        // Test explicit port mapping with host_port != 0
        let explicit = PortMapping {
            container_port: 3000,
            host_port: 8080,
        };
        assert_eq!(explicit.to_publish_flag(), "8080:3000");
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
