use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackendStatus {
    Disconnected,
    Starting,
    Connected,
    Stopping,
    Recovering,
    Faulted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EngineStatus {
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PacStatus {
    Active,
    Inactive,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingMetrics {
    pub discord_forwarded: u64,
    pub non_discord_rejected: u64,
    pub non_discord_forwarded: u64, // Always 0 for security invariant
    pub discord_failed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub strong_advanced_available: bool,
    pub npcap_available: bool,
    pub engine_version: String,
    pub browser_routing_supported: bool,
    pub discord_desktop_supported: bool,
    pub recovery_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub app_version: String,
    pub engine_version: String,
    pub windows_version: String,
    pub mode: String,
    pub backend_state: BackendStatus,
    pub pac_active: bool,
    pub active_ports: Option<ActivePorts>,
    pub routing_metrics: RoutingMetrics,
    pub error_codes: Vec<String>,
    pub recovery_status: String,
    pub updater_redirect_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivePorts {
    pub engine: u16,
    pub adapter: u16,
    pub pac: u16,
    pub diagnostics: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRequest {
    pub mode: crate::engine::DpiMode,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::DpiMode;

    #[test]
    fn test_start_request_deserialization() {
        let json_balanced = r#"
        {
            "mode": {
                "type": "balanced",
                "chunk_size": 2
            }
        }
        "#;
        let req_balanced: StartRequest = serde_json::from_str(json_balanced).unwrap();
        assert_eq!(req_balanced.mode, DpiMode::Balanced { chunk_size: 2 });

        let json_turbo = r#"
        {
            "mode": {
                "type": "turbo"
            }
        }
        "#;
        let req_turbo: StartRequest = serde_json::from_str(json_turbo).unwrap();
        assert_eq!(req_turbo.mode, DpiMode::Turbo);

        let json_strong = r#"
        {
            "mode": {
                "type": "strong"
            }
        }
        "#;
        let req_strong: StartRequest = serde_json::from_str(json_strong).unwrap();
        assert_eq!(req_strong.mode, DpiMode::Strong);

        let json_advanced = r#"
        {
            "mode": {
                "type": "strong_advanced",
                "fake_count": 3
            }
        }
        "#;
        let req_advanced: StartRequest = serde_json::from_str(json_advanced).unwrap();
        assert_eq!(req_advanced.mode, DpiMode::StrongAdvanced { fake_count: 3 });
    }
}
