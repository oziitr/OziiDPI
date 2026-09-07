pub mod npcap;
pub mod spoofdpi;

use crate::process::ProcessIdentity;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DpiMode {
    Turbo,
    Balanced { chunk_size: u8 },
    Strong,
    StrongAdvanced { fake_count: u8 },
}

impl DpiMode {
    pub fn requires_npcap(&self) -> bool {
        matches!(self, DpiMode::StrongAdvanced { .. })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Failed to spawn process: {0}")]
    ProcessSpawnError(#[from] std::io::Error),
    #[error("Engine binary not found at: {0}")]
    BinaryNotFound(String),
    #[error("Missing prerequisite: {0}")]
    MissingPrerequisite(String),
    #[error("Port {0} is already in use or unavailable")]
    PortUnavailable(u16),
    #[error("Engine failed health check on port {0}")]
    HealthCheckFailed(u16),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub trait DpiEngine {
    /// Starts the engine asynchronously, returns the validated ProcessIdentity.
    fn start(
        &mut self,
        bind_address: &str,
        port: u16,
        mode: &DpiMode,
    ) -> Result<ProcessIdentity, EngineError>;

    /// Requests graceful shutdown of the engine.
    fn stop(&mut self) -> Result<(), EngineError>;

    /// Verifies the engine is running and accepting TCP connections.
    fn health_check(&self) -> bool;
}
