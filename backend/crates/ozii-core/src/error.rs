use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum OziiError {
    #[error("An active PAC configuration already exists.")]
    ExistingPacConfigured,

    #[error("WPAD configuration conflict detected.")]
    WpadConflict,

    #[error("Engine binary is missing.")]
    EngineBinaryMissing,

    #[error("Engine build version mismatch.")]
    EngineBuildMismatch,

    #[error("Failed to start the DPI engine.")]
    EngineStartupFailed,

    #[error("DPI engine exited unexpectedly.")]
    EngineExited,

    #[error("Failed to start the local PAC server.")]
    PacServerFailed,

    #[error("Failed to allocate necessary local ports.")]
    PortAllocationFailed,

    #[error("Failed to read from proxy.")]
    ProxyReadFailed,

    #[error("Failed to write to proxy.")]
    ProxyWriteFailed,

    #[error("Failed to restore original Windows proxy settings.")]
    ProxyRestoreFailed,

    #[error("A dirty session exists. Recovery is required before starting.")]
    RecoveryRequired,

    #[error("Recovery attempted but failed.")]
    RecoveryFailed,

    #[error("The backend is already running.")]
    AlreadyRunning,

    #[error("The backend is not currently running.")]
    NotRunning,

    #[error("Unsupported configuration provided.")]
    UnsupportedConfiguration,

    #[error("Advanced DPI mode is unavailable on this system.")]
    AdvancedModeUnavailable,

    #[error("Internal IO Error: {0}")]
    IoError(String),

    #[error("Configuration Error: {0}")]
    ConfigError(String),
}

impl From<std::io::Error> for OziiError {
    fn from(err: std::io::Error) -> Self {
        OziiError::IoError(err.to_string())
    }
}
