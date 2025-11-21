use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// Information about a registered local service
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub port: u16,
    pub tags: Vec<String>,
}

/// Manages local service discovery and port allocation
pub struct ServiceRegistry {
    services: Arc<Mutex<HashMap<String, ServiceInfo>>>,
    local_domain: String,
}

impl ServiceRegistry {
    pub fn new(local_domain: Option<String>) -> Self {
        Self {
            services: Arc::new(Mutex::new(HashMap::new())),
            local_domain: local_domain.unwrap_or_else(|| "local".to_string()),
        }
    }

    /// Find a free port on localhost
    ///
    /// This binds to port 0 to let the OS assign a free port, then drops the listener.
    /// Note: There is a tiny race condition window where another process could grab the port,
    /// but it's generally safe enough for this use case.
    pub fn allocate_port(&self) -> Option<u16> {
        match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => match listener.local_addr() {
                Ok(addr) => {
                    info!("Allocated ephemeral port: {}", addr.port());
                    Some(addr.port())
                }
                Err(e) => {
                    warn!("Failed to get local address for ephemeral port: {}", e);
                    None
                }
            },
            Err(e) => {
                warn!("Failed to bind to ephemeral port: {}", e);
                None
            }
        }
    }

    /// Register a service with the registry
    pub fn register_service(&self, name: String, port: u16, tags: Vec<String>) {
        let mut services = self.services.lock().unwrap();
        let info = ServiceInfo {
            name: name.clone(),
            port,
            tags,
        };
        services.insert(name.clone(), info);
        info!("Registered service '{}' on port {}", name, port);
    }

    /// Unregister a service
    pub fn unregister_service(&self, name: &str) {
        let mut services = self.services.lock().unwrap();
        if services.remove(name).is_some() {
            info!("Unregistered service '{}'", name);
        }
    }

    /// Get all registered services
    pub fn get_services(&self) -> Vec<ServiceInfo> {
        let services = self.services.lock().unwrap();
        services.values().cloned().collect()
    }

    /// Get the configured local domain
    pub fn local_domain(&self) -> &str {
        &self.local_domain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_registry() {
        let registry = ServiceRegistry::new(Some("test.local".to_string()));

        // Test domain
        assert_eq!(registry.local_domain(), "test.local");

        // Test registration
        registry.register_service("app1".to_string(), 8080, vec!["http".to_string()]);
        let services = registry.get_services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "app1");
        assert_eq!(services[0].port, 8080);

        // Test unregistration
        registry.unregister_service("app1");
        assert!(registry.get_services().is_empty());
    }

    #[test]
    fn test_port_allocation() {
        let registry = ServiceRegistry::new(None);
        let port = registry.allocate_port();
        assert!(port.is_some());
        assert!(port.unwrap() > 0);
    }
}
