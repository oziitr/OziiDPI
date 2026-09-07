//! Local-only proxy mode: engine + adapter, NOTHING else.
//!
//! This is the "Discord launcher" companion mode. It starts the DPI engine
//! and the CONNECT adapter on loopback so that a single application can be
//! pointed at it with `--proxy-server=http://127.0.0.1:39572`.
//!
//! Deliberately NOT touched (unlike `OziiService::start`):
//!   - Windows system proxy (ProxyEnable / ProxyServer / AutoConfigURL)
//!   - PAC server / WinHTTP / winsock
//!   - hosts file / DNS / routing / firewall
//!   - the updater forwarder or packet guard
//!
//! Stopping this mode leaves the system exactly as it was.

use std::sync::{Arc, Mutex};

use crate::adapter::AdapterProxy;
use crate::diagnostics::{DiagnosticsReporter, DiagnosticsState};
use crate::domain::DomainAllowlist;
use crate::engine::spoofdpi::SpoofDpiEngine;
use crate::engine::{DpiEngine, DpiMode};
use crate::error::OziiError;
use std::path::PathBuf;

pub struct LocalProxy {
    pub engine_port: u16,
    pub adapter_port: u16,
    pub diagnostics_port: u16,
    engine: SpoofDpiEngine,
    adapter: AdapterProxy,
    reporter: DiagnosticsReporter,
    forwarder: Option<crate::updater_forwarder::UpdaterForwarder>,
    diag_state: Arc<Mutex<DiagnosticsState>>,
}

impl LocalProxy {
    /// Starts engine + adapter + diagnostics reporter. No system state is
    /// read or written anywhere.
    pub fn start(
        engine_path: PathBuf,
        mode: &DpiMode,
        engine_port: u16,
        adapter_port: u16,
        diagnostics_port: u16,
    ) -> Result<Self, OziiError> {
        let mut engine = SpoofDpiEngine::new(engine_path);
        let _identity = engine
            .start("127.0.0.1", engine_port, mode)
            .map_err(|_| OziiError::EngineStartupFailed)?;

        let allowlist = Arc::new(DomainAllowlist::discord_default());
        let diag_state = DiagnosticsState::new();
        let adapter = AdapterProxy::start(
            adapter_port,
            engine_port,
            Arc::clone(&allowlist),
            Arc::clone(&diag_state),
        );
        let reporter = DiagnosticsReporter::start(Arc::clone(&diag_state), diagnostics_port);

        // Raw TLS receiver for the process-aware Discord guard. The guard
        // reflects only verified Discord.exe/Update.exe TCP/443 flows here;
        // this receiver validates SNI and CONNECTs through the adapter.
        let forwarder = crate::updater_forwarder::UpdaterForwarder::start_transparent(
            adapter_port,
            Arc::clone(&allowlist),
            Arc::clone(&diag_state),
        );

        // Compatibility PAC: some browsers cache the old PAC URL (39573) from
        // previous sessions. Serve a PAC that answers DIRECT so those browsers
        // recover instantly without touching any system setting.
        std::thread::spawn(move || {
            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nConnection: close\r\nContent-Length: 33\r\n\r\nfunction FindProxyForURL(){return \"DIRECT\";}";
            if let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", 39573)) {
                for stream in listener.incoming().flatten() {
                    use std::io::{Read, Write};
                    let mut s = stream;
                    let mut buf = [0u8; 1024];
                    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                    let _ = s.read(&mut buf);
                    let _ = s.write_all(response.as_bytes());
                }
            }
        });

        Ok(Self {
            engine_port,
            adapter_port,
            diagnostics_port,
            engine,
            adapter,
            reporter,
            forwarder,
            diag_state,
        })
    }

    pub fn is_stop_requested(&self) -> bool {
        self.diag_state.lock().unwrap().stop_requested
    }

    pub fn transparent_forwarder_port(&self) -> Option<u16> {
        self.forwarder.as_ref().map(|forwarder| forwarder.port)
    }

    pub fn stop(&mut self) {
        crate::pktguard::request_guard_stop();
        if let Some(forwarder) = self.forwarder.take() {
            forwarder.stop();
        }
        self.reporter.stop();
        self.adapter.stop();
        let _ = self.engine.stop();
    }
}
