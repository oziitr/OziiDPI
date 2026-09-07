#![allow(clippy::new_without_default, clippy::collapsible_if)]
use crate::process::ProcessIdentity;
use crate::proxy_state::{OriginalProxyState, ProxyError, SystemProxy};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub schema_version: u32,
    pub session_id: String,
    pub original_proxy_state: OriginalProxyState,
    pub pac_port: u16,
    pub dpi_proxy_port: u16,
    pub diagnostics_port: Option<u16>,
    pub backend_pid: u32,
    pub engine_identity: Option<ProcessIdentity>,
    pub activation_timestamp: u64,
    pub clean_shutdown: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("I/O Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization Error: {0}")]
    SerdeError(#[from] serde_json::Error),
    #[error("Proxy Error: {0}")]
    ProxyError(#[from] ProxyError),
    #[error("Active session already exists, recovery required first")]
    DirtySessionExists,
    #[error("No active session to stop")]
    NoActiveSession,
}

pub struct SessionManager {
    state_dir: PathBuf,
}

impl SessionManager {
    pub fn new() -> Self {
        // Use %LOCALAPPDATA%\OziiDPI\state\ or fallback to a local temp folder
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());

        let mut state_dir = PathBuf::from(local_app_data);
        state_dir.push("OziiDPI");
        state_dir.push("state");

        std::fs::create_dir_all(&state_dir).unwrap_or_default();

        Self { state_dir }
    }

    #[cfg(test)]
    pub fn new_for_test(temp_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&temp_dir).unwrap_or_default();
        Self {
            state_dir: temp_dir,
        }
    }

    pub fn state_file_path(&self) -> PathBuf {
        self.state_dir.join("session.json")
    }

    pub fn read_state(&self) -> Result<Option<SessionState>, SessionError> {
        let path = self.state_file_path();
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)?;
        match serde_json::from_str::<SessionState>(&content) {
            Ok(state) => Ok(Some(state)),
            Err(_e) => {
                // Phase 12: Session Schema Robustness - Quarantine corrupted state
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let corrupt_path = path.with_extension(format!("corrupt.{}.json", timestamp));
                let _ = std::fs::rename(&path, &corrupt_path);

                // We return None because the state is irreparably lost, but we shouldn't block the app forever.
                // The user's proxy might be stuck, but at least the app can start a new session and overwrite it.
                Ok(None)
            }
        }
    }

    fn persist_state(&self, state: &SessionState) -> Result<(), SessionError> {
        let path = self.state_file_path();
        let tmp_path = path.with_extension("json.tmp");

        let content = serde_json::to_string_pretty(state)?;
        std::fs::write(&tmp_path, content)?;

        // Atomic rename
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    pub fn mark_clean_shutdown(&self) -> Result<(), SessionError> {
        if let Some(mut state) = self.read_state()? {
            state.clean_shutdown = true;
            self.persist_state(&state)?;
        }
        Ok(())
    }

    /// Attaches the running engine identity to the active session so crash
    /// recovery can safe-kill an orphaned engine process.
    pub fn set_engine_identity(&self, identity: ProcessIdentity) -> Result<(), SessionError> {
        if let Some(mut state) = self.read_state()? {
            state.engine_identity = Some(identity);
            self.persist_state(&state)?;
        }
        Ok(())
    }

    pub fn start_transaction<P: SystemProxy>(
        &self,
        proxy: &P,
        pac_port: u16,
        dpi_proxy_port: u16,
        diagnostics_port: u16,
        pac_url: &str,
    ) -> Result<SessionState, SessionError> {
        // 1. Detect stale previous session
        if let Some(existing) = self.read_state()? {
            if !existing.clean_shutdown {
                return Err(SessionError::DirtySessionExists);
            }
        }

        // 2. Capture original Windows proxy state
        let original_state = proxy.read_original()?;

        // 3. Persist original state BEFORE modifying Windows
        let state = SessionState {
            schema_version: 1,
            session_id: Uuid::new_v4().to_string(),
            original_proxy_state: original_state,
            pac_port,
            dpi_proxy_port,
            diagnostics_port: Some(diagnostics_port),
            backend_pid: std::process::id(),
            engine_identity: None, // Filled by caller after successful engine spawn
            activation_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            clean_shutdown: false,
        };
        self.persist_state(&state)?;

        // 4. Activate local PAC in Windows (Checks for conflict inside proxy)
        let proxy_server = format!("http://127.0.0.1:{}", dpi_proxy_port);
        match proxy.set_pac(pac_url, &proxy_server) {
            Ok(_) => {
                // 5. Notify WinINET
                proxy.notify_system();
                Ok(state)
            }
            Err(e) => {
                // If setting PAC fails (e.g. existing PAC conflict),
                // we rollback by just setting clean_shutdown so recovery isn't triggered,
                // because we never successfully modified the registry.
                self.mark_clean_shutdown()?;
                Err(SessionError::ProxyError(e))
            }
        }
    }

    pub fn stop_transaction<P: SystemProxy>(&self, proxy: &P) -> Result<(), SessionError> {
        // 1. Read state
        let state = self.read_state()?.ok_or(SessionError::NoActiveSession)?;

        if state.clean_shutdown {
            return Ok(()); // Already cleanly shutdown
        }

        // 2. Restore ORIGINAL Windows configuration EXACTLY
        // If restoration fails, DO NOT delete the recovery state.
        proxy.restore_original(&state.original_proxy_state)?;

        // 3. Notify Windows
        proxy.notify_system();

        // 4. Terminate engine safely if we started one
        if let Some(engine) = state.engine_identity {
            let _ = engine.safe_kill();
        }

        // 5. Mark clean
        self.mark_clean_shutdown()?;

        // 6. Delete file to complete stop
        let _ = std::fs::remove_file(self.state_file_path());

        Ok(())
    }

    pub fn recover<P: SystemProxy>(&self, proxy: &P) -> Result<bool, SessionError> {
        let state = match self.read_state()? {
            Some(s) => s,
            None => return Ok(false), // Nothing to recover
        };

        if state.clean_shutdown {
            let _ = std::fs::remove_file(self.state_file_path());
            return Ok(false);
        }

        // Restore exact original settings
        proxy.restore_original(&state.original_proxy_state)?;
        proxy.notify_system();

        // Safely kill previous engine
        if let Some(engine) = state.engine_identity {
            let _ = engine.safe_kill();
        }

        // Clean stale state after recovery succeeds
        let _ = std::fs::remove_file(self.state_file_path());
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_state::{MockSystemProxy, OriginalProxyState};
    use tempfile::tempdir;

    #[test]
    fn test_start_stop_transaction() {
        let dir = tempdir().unwrap();
        let session_manager = SessionManager::new_for_test(dir.path().to_path_buf());

        let initial = OriginalProxyState {
            proxy_enable: Some(0),
            proxy_server: None,
            proxy_override: None,
            auto_config_url: None,
            wpad_enabled: None,
        };
        let proxy = MockSystemProxy::new(initial.clone());

        // START
        let state = session_manager
            .start_transaction(&proxy, 8787, 8080, 9090, "http://127.0.0.1:8787/proxy.pac")
            .unwrap();

        assert_eq!(state.original_proxy_state, initial);
        assert!(!state.clean_shutdown);

        let modified = proxy.read_original().unwrap();
        assert_eq!(
            modified.auto_config_url,
            Some("http://127.0.0.1:8787/proxy.pac".to_string())
        );

        // STOP
        session_manager.stop_transaction(&proxy).unwrap();

        let restored = proxy.read_original().unwrap();
        assert_eq!(restored, initial);

        assert!(!session_manager.state_file_path().exists());
    }

    #[test]
    fn test_recovery_flow() {
        let dir = tempdir().unwrap();
        let session_manager = SessionManager::new_for_test(dir.path().to_path_buf());

        let initial = OriginalProxyState {
            proxy_enable: Some(1),
            proxy_server: Some("proxy.corporate.local:8080".to_string()),
            proxy_override: Some("<local>".to_string()),
            auto_config_url: None,
            wpad_enabled: None,
        };
        let proxy = MockSystemProxy::new(initial.clone());

        // Simulate a crash (START without STOP)
        session_manager
            .start_transaction(&proxy, 8787, 8080, 9090, "http://127.0.0.1:8787/proxy.pac")
            .unwrap();

        // RECOVER
        let recovered = session_manager.recover(&proxy).unwrap();
        assert!(recovered);

        let restored = proxy.read_original().unwrap();
        assert_eq!(restored, initial);

        assert!(!session_manager.state_file_path().exists());
    }
}
