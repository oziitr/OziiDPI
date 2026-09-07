use crate::domain::DomainAllowlist;

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Port for the local PAC server (e.g., 8787).
    /// If 0, the system assigns one dynamically.
    pub pac_port: u16,

    /// Port for the local proxy engine (e.g., 8080).
    /// If 0, the system assigns one dynamically.
    pub proxy_port: u16,

    /// Address to bind the proxy to.
    /// By default, MUST be "127.0.0.1" for safety.
    pub bind_address: String,

    /// Domain list to route through the proxy.
    pub allowlist: DomainAllowlist,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            pac_port: 0,   // dynamic by default
            proxy_port: 0, // dynamic by default
            bind_address: "127.0.0.1".to_string(),
            allowlist: DomainAllowlist::ozii_default(),
        }
    }
}
