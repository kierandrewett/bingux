//! D-Bus proxy instance management.
//!
//! Each sandboxed package gets its own proxy socket. The proxy
//! connects to the real bus and forwards filtered messages.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Configuration for a proxy instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Package this proxy serves.
    pub package: String,
    /// Path to the proxy socket the sandboxed app connects to.
    pub socket_path: PathBuf,
    /// Path to the real D-Bus bus socket.
    pub real_bus_path: PathBuf,
    /// Whether this is a session or system bus proxy.
    pub bus_type: crate::policy::BusType,
}

impl ProxyConfig {
    /// Create a session bus proxy config for a package.
    pub fn session(package: &str) -> Self {
        Self {
            package: package.to_string(),
            socket_path: PathBuf::from(format!("/run/bingux/dbus/{package}-session.sock")),
            real_bus_path: PathBuf::from("/run/dbus/system_bus_socket"),
            bus_type: crate::policy::BusType::Session,
        }
    }

    /// Create a system bus proxy config for a package.
    pub fn system(package: &str) -> Self {
        Self {
            package: package.to_string(),
            socket_path: PathBuf::from(format!("/run/bingux/dbus/{package}-system.sock")),
            real_bus_path: PathBuf::from("/run/dbus/system_bus_socket"),
            bus_type: crate::policy::BusType::System,
        }
    }
}

/// A running proxy instance.
#[derive(Debug)]
pub struct ProxyInstance {
    pub config: ProxyConfig,
    pub pid: Option<u32>,
}

impl ProxyInstance {
    pub fn new(config: ProxyConfig) -> Self {
        Self { config, pid: None }
    }

    /// The socket path that should be mounted into the sandbox
    /// as the package's D-Bus socket.
    pub fn sandbox_socket(&self) -> &PathBuf {
        &self.config.socket_path
    }

    /// Start the proxy (stub — real implementation would fork a process
    /// that listens on socket_path and forwards to real_bus_path with filtering).
    pub fn start(&mut self) -> Result<(), std::io::Error> {
        tracing::info!(
            "starting D-Bus proxy for {} ({:?} bus) at {}",
            self.config.package,
            self.config.bus_type,
            self.config.socket_path.display(),
        );
        // In a real implementation:
        // 1. Create unix socket at socket_path
        // 2. Connect to real_bus_path
        // 3. For each message: filter → allow/deny/prompt
        // 4. Forward allowed messages to real bus
        // 5. Return responses to sandbox
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), std::io::Error> {
        if let Some(pid) = self.pid.take() {
            tracing::info!("stopping D-Bus proxy for {} (pid {})", self.config.package, pid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_proxy_config() {
        let config = ProxyConfig::session("firefox");
        assert_eq!(config.package, "firefox");
        assert!(config.socket_path.to_str().unwrap().contains("firefox-session"));
        assert_eq!(config.bus_type, crate::policy::BusType::Session);
    }

    #[test]
    fn system_proxy_config() {
        let config = ProxyConfig::system("nginx");
        assert_eq!(config.package, "nginx");
        assert!(config.socket_path.to_str().unwrap().contains("nginx-system"));
        assert_eq!(config.bus_type, crate::policy::BusType::System);
    }

    #[test]
    fn proxy_instance_lifecycle() {
        let config = ProxyConfig::session("firefox");
        let mut proxy = ProxyInstance::new(config);
        assert!(proxy.pid.is_none());
        proxy.start().unwrap();
        proxy.stop().unwrap();
    }
}
