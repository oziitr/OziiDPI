use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::adapter::AdapterProxy;
use crate::app_config::{AppConfig, ConfigManager};
use crate::diagnostics::{DiagnosticsReporter, DiagnosticsState};
use crate::domain::DomainAllowlist;
use crate::engine::DpiEngine;
use crate::engine::spoofdpi::SpoofDpiEngine;
use crate::error::OziiError;
use crate::events::{BackendEvent, EventBroadcaster};
use crate::models::{
    ActivePorts, BackendStatus, Capabilities, DiagnosticReport, RoutingMetrics, StartRequest,
};
use crate::pac::PacGenerator;
use crate::process::ProcessIdentity;
use crate::proxy_state::{RealSystemProxy, SystemProxy};
use crate::session::SessionManager;
use crate::tunnel::dev_log;
use std::net::TcpListener;
use std::thread;

/// Preferred fixed ports. Reused across restarts when free so that browsers
/// holding a cached PAC / proxy result from a previous session keep working
/// after an OziiDPI restart (mitigates stale-PAC "stuck on Loading" states).
const PREFERRED_ENGINE_PORT: u16 = 39571;
const PREFERRED_ADAPTER_PORT: u16 = 39572;
const PREFERRED_PAC_PORT: u16 = 39573;
const PREFERRED_DIAG_PORT: u16 = 39574;

fn bind_preferred_or_free(preferred: u16) -> Result<u16, OziiError> {
    if let Ok(listener) = TcpListener::bind(("127.0.0.1", preferred)) {
        drop(listener);
        return Ok(preferred);
    }
    get_free_port()
}

// Core Facade
pub struct OziiService {
    state: Arc<Mutex<ServiceState>>,
    config_manager: ConfigManager,
    events: Arc<EventBroadcaster>,
    session_manager: Arc<SessionManager>,
    proxy_impl: Arc<RealSystemProxy>,
}

struct ServiceState {
    status: BackendStatus,
    active_ports: Option<ActivePorts>,
    pac_running: Option<Arc<AtomicBool>>,
    adapter: Option<AdapterProxy>,
    reporter: Option<DiagnosticsReporter>,
    engine: Option<SpoofDpiEngine>,
    diag_state: Arc<Mutex<DiagnosticsState>>,
    engine_path: Option<std::path::PathBuf>,
    engine_identity: Option<ProcessIdentity>,
    supervisor_running: Option<Arc<AtomicBool>>,
    active_mode: Option<crate::engine::DpiMode>,
    guard_pid: Option<u32>,
    updater_forwarder: Option<crate::updater_forwarder::UpdaterForwarder>,
    updater_redirect_applied: bool,
}

fn get_free_port() -> Result<u16, OziiError> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| OziiError::PortAllocationFailed)?;
    Ok(listener.local_addr().unwrap().port())
}

impl Default for OziiService {
    fn default() -> Self {
        Self::new(None)
    }
}

impl OziiService {
    pub fn new(custom_engine_path: Option<std::path::PathBuf>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ServiceState {
                status: BackendStatus::Disconnected,
                active_ports: None,
                pac_running: None,
                adapter: None,
                reporter: None,
                engine: None,
                diag_state: DiagnosticsState::new(),
                engine_path: custom_engine_path,
                engine_identity: None,
                supervisor_running: None,
                active_mode: None,
                guard_pid: None,
                updater_forwarder: None,
                updater_redirect_applied: false,
            })),
            config_manager: ConfigManager::new(),
            events: Arc::new(EventBroadcaster::new()),
            session_manager: Arc::new(SessionManager::new()),
            proxy_impl: Arc::new(RealSystemProxy),
        }
    }

    pub fn get_events(&self) -> Arc<EventBroadcaster> {
        Arc::clone(&self.events)
    }

    fn emit(&self, event: BackendEvent) {
        let _ = self.events.get_sender().send(event);
    }

    fn transition_state(&self, new_state: BackendStatus) {
        let mut s = self.state.lock().unwrap();
        let old_state = s.status.clone();
        s.status = new_state.clone();
        self.emit(BackendEvent::StateChanged {
            old_state,
            new_state,
        });
    }

    pub fn start(&self, request: StartRequest) -> Result<(), OziiError> {
        {
            let s = self.state.lock().unwrap();
            if s.status == BackendStatus::Starting || s.status == BackendStatus::Connected {
                return Err(OziiError::AlreadyRunning);
            }
        }

        // Check for dirty session — auto-recover if old engine is dead
        if let Ok(Some(session)) = self.session_manager.read_state()
            && !session.clean_shutdown
        {
            let engine_alive = session
                .engine_identity
                .as_ref()
                .is_some_and(|e| e.is_same_process());
            if engine_alive {
                return Err(OziiError::RecoveryRequired);
            }
            // Old engine is dead — silently recover: restore proxy, clear session
            let _ = self
                .proxy_impl
                .restore_original(&session.original_proxy_state);
            self.proxy_impl.notify_system();
            let _ = std::fs::remove_file(self.session_manager.state_file_path());
        }

        self.transition_state(BackendStatus::Starting);

        // Honor the persisted user preference for WPAD auto-detect conflicts.
        let cfg = self.config_manager.load();
        crate::proxy_state::WPAD_IGNORE.store(cfg.ignore_wpad, Ordering::SeqCst);

        let current_exe = std::env::current_exe().unwrap_or_default();
        let exe_dir = current_exe
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));

        // Try production locations first, then fallback to dev location
        let custom_path = self.state.lock().unwrap().engine_path.clone();

        let engine_path = custom_path.filter(|p| p.exists()).unwrap_or_else(|| {
            let mut p = exe_dir.join("ozii-dpi-engine.exe");
            if !p.exists() {
                p = exe_dir.join("engine/bin/ozii-dpi-engine.exe");
                if !p.exists() {
                    // Development fallback
                    p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .join("engine/bin/ozii-dpi-engine.exe");
                }
            }
            p
        });

        let mut engine = SpoofDpiEngine::new(engine_path);
        let engine_port = bind_preferred_or_free(PREFERRED_ENGINE_PORT)?;
        let adapter_port = bind_preferred_or_free(PREFERRED_ADAPTER_PORT)?;
        let pac_port = bind_preferred_or_free(PREFERRED_PAC_PORT)?;
        let diag_port = bind_preferred_or_free(PREFERRED_DIAG_PORT)?;

        match engine.start("127.0.0.1", engine_port, &request.mode) {
            Ok(identity) => {
                self.emit(BackendEvent::EngineStarted { pid: identity.pid });

                let allowlist = Arc::new(DomainAllowlist::ozii_default());
                let diag_state = self.state.lock().unwrap().diag_state.clone();
                let adapter = AdapterProxy::start(
                    adapter_port,
                    engine_port,
                    Arc::clone(&allowlist),
                    Arc::clone(&diag_state),
                );
                let reporter = DiagnosticsReporter::start(Arc::clone(&diag_state), diag_port);

                let generator = PacGenerator::new((*allowlist).clone());
                let original = self.proxy_impl.read_original().unwrap_or_default();
                let fallback = original.proxy_server.as_deref();
                let pac_script =
                    generator.generate_pac_script(&format!("127.0.0.1:{}", adapter_port), fallback);

                let pac_running = Arc::new(AtomicBool::new(true));
                let pac_r = pac_running.clone();
                std::thread::spawn(move || {
                    if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", pac_port)) {
                        listener.set_nonblocking(true).unwrap();
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                            pac_script.len(),
                            pac_script
                        );
                        use std::io::{Read, Write};
                        while pac_r.load(Ordering::Relaxed) {
                            if let Ok((mut stream, _)) = listener.accept() {
                                let mut buf = [0; 512];
                                let _ = stream.read(&mut buf);
                                let _ = stream.write_all(response.as_bytes());
                            }
                            std::thread::sleep(Duration::from_millis(50));
                        }
                    }
                });

                let pac_url = format!("http://127.0.0.1:{}/proxy.pac", pac_port);

                match self.session_manager.start_transaction(
                    &*self.proxy_impl,
                    pac_port,
                    adapter_port,
                    diag_port,
                    &pac_url,
                ) {
                    Ok(_) => {
                        let mut s = self.state.lock().unwrap();
                        s.active_ports = Some(ActivePorts {
                            engine: engine_port,
                            adapter: adapter_port,
                            pac: pac_port,
                            diagnostics: diag_port,
                        });
                        s.pac_running = Some(pac_running);
                        s.adapter = Some(adapter);
                        s.reporter = Some(reporter);
                        s.engine = Some(engine);
                        s.engine_identity = Some(identity.clone());
                        s.active_mode = Some(request.mode.clone());
                        drop(s);
                        // Attach engine identity to the persisted session so a
                        // crashed backend can safe-kill an orphaned engine.
                        let _ = self.session_manager.set_engine_identity(identity.clone());
                        self.spawn_engine_supervisor(identity);

                        // Activate Discord Updater Protection (hosts redirect + forwarder)
                        self.activate_updater_protection();
                    }
                    Err(crate::session::SessionError::ProxyError(
                        crate::proxy_state::ProxyError::WpadConflict,
                    )) => {
                        let _ = engine.stop();
                        adapter.stop();
                        reporter.stop();
                        pac_running.store(false, Ordering::Relaxed);
                        self.transition_state(BackendStatus::Faulted);
                        return Err(OziiError::WpadConflict);
                    }
                    Err(_) => {
                        let _ = engine.stop();
                        adapter.stop();
                        reporter.stop();
                        pac_running.store(false, Ordering::Relaxed);
                        self.transition_state(BackendStatus::Faulted);
                        return Err(OziiError::ProxyWriteFailed);
                    }
                }
            }
            Err(_) => {
                self.transition_state(BackendStatus::Faulted);
                return Err(OziiError::EngineStartupFailed);
            }
        }

        self.transition_state(BackendStatus::Connected);
        Ok(())
    }

    /// Watches the engine process while Connected. If the engine dies
    /// unexpectedly (crash / external kill), the Windows proxy state is
    /// restored immediately (fail-open) instead of leaving the system routed
    /// through a dead PAC — the exact condition that manifests as browsers
    /// hanging forever on sites like Discord.
    fn spawn_engine_supervisor(&self, identity: ProcessIdentity) {
        let flag = Arc::new(AtomicBool::new(true));
        let sup_flag = Arc::clone(&flag);
        {
            let mut s = self.state.lock().unwrap();
            s.supervisor_running = Some(flag);
        }
        let st = Arc::clone(&self.state);
        let sm = Arc::clone(&self.session_manager);
        let px = Arc::clone(&self.proxy_impl);
        let ev = Arc::clone(&self.events);

        std::thread::spawn(move || {
            while sup_flag.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(2));
                if !sup_flag.load(Ordering::SeqCst) {
                    return; // intentional shutdown in progress
                }
                if !identity.is_same_process() {
                    // Fail-open: restore original Windows proxy configuration.
                    let _ = sm.recover(&*px);
                    Self::deactivate_updater_redirect_static();
                    let reason =
                        "engine process terminated unexpectedly; Windows proxy settings restored"
                            .to_string();
                    let _ = ev.get_sender().send(BackendEvent::EngineFailed { reason });
                    let old_state = {
                        let mut s = st.lock().unwrap();
                        let old = s.status.clone();
                        s.status = BackendStatus::Faulted;
                        old
                    };
                    let _ = ev.get_sender().send(BackendEvent::StateChanged {
                        old_state,
                        new_state: BackendStatus::Faulted,
                    });
                    return;
                }
            }
        });
    }

    /// Activates the Discord Desktop updater protection exactly like
    /// BypaxDPI: hosts redirect (updates.discord.com -> 127.0.0.2) + local
    /// transparent TLS forwarder on 127.0.0.2:443 that peeks SNI, tunnels
    /// through the DPI engine and fragments the first server flight.
    fn activate_updater_protection(&self) {
        let cfg = self.config_manager.load();
        if !cfg.protect_discord_updater {
            return;
        }
        let mut s = self.state.lock().unwrap();
        let engine_port = match s.active_ports.as_ref() {
            Some(p) => p.engine,
            None => return,
        };
        let allowlist = Arc::new(DomainAllowlist::discord_default());
        let diag = Arc::clone(&s.diag_state);
        if let Some(fwd) =
            crate::updater_forwarder::UpdaterForwarder::start(engine_port, allowlist, diag)
        {
            s.updater_forwarder = Some(fwd);
            dev_log("[UPDATER] forwarder listening on 127.0.0.2:443");
        } else {
            dev_log("[WARN] updater forwarder not started: 127.0.0.2:443 unavailable");
        }
        match crate::hosts::set_updater_redirect(true) {
            Ok(crate::hosts::HostsUpdateResult::Applied)
            | Ok(crate::hosts::HostsUpdateResult::AppliedElevated) => {
                s.updater_redirect_applied = true;
            }
            Ok(crate::hosts::HostsUpdateResult::Failed) => {
                s.updater_redirect_applied = false;
                dev_log("[WARN] updater hosts redirect failed (UAC denied?)");
            }
            Err(_) => {
                s.updater_redirect_applied = false;
            }
        }
    }

    fn deactivate_updater_redirect_static() {
        Self::request_guard_stop();
        let _ = crate::hosts::set_updater_redirect(false);
    }

    fn shutdown_updater_protection(&self) {
        let mut s = self.state.lock().unwrap();
        s.guard_pid = None;
        if let Some(fwd) = s.updater_forwarder.take() {
            fwd.stop();
        }
        let redirect_applied = s.updater_redirect_applied;
        s.updater_redirect_applied = false;
        drop(s);
        if redirect_applied {
            let _ = crate::hosts::set_updater_redirect(false);
        }
        Self::request_guard_stop();
    }

    /// Launches `ozii-guard.exe` elevated via ShellExecuteExW("runas").
    /// Returns the helper PID when the elevation prompt was accepted.
    #[allow(dead_code)]
    fn spawn_guard(
        &self,
        domains: &[String],
        mode: &str,
        chunk_size: u8,
        fake_count: u8,
    ) -> Option<u32> {
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::processthreadsapi::GetProcessId;
        use winapi::um::shellapi::{SEE_MASK_NOCLOSEPROCESS, ShellExecuteExW};
        use winapi::um::winuser::SW_HIDE;

        let current_exe = std::env::current_exe().ok()?;
        let exe_dir = current_exe.parent()?;

        let mut guard_path = exe_dir.join("ozii-guard.exe");
        if !guard_path.exists() {
            guard_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()?
                .parent()?
                .parent()?
                .join("target/release/ozii-guard.exe");
        }
        if !guard_path.exists() {
            dev_log(&format!(
                "[GUARD] ozii-guard.exe not found at {}; transparent mode disabled",
                guard_path.display()
            ));
            return None;
        }

        // Remove a stale stop marker from a previous session.
        let _ = std::fs::remove_file(Self::guard_stop_file());

        let params = format!(
            "--mode {mode} --chunk-size {chunk_size} --fake-count {fake_count} \
             --domains {} --parent-pid {}",
            domains.join(","),
            std::process::id()
        );

        let path_wide: Vec<u16> = guard_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let params_wide: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();
        let dir_wide: Vec<u16> = exe_dir
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut exe_info: winapi::um::shellapi::SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        exe_info.cbSize = std::mem::size_of::<winapi::um::shellapi::SHELLEXECUTEINFOW>() as u32;
        exe_info.fMask = SEE_MASK_NOCLOSEPROCESS;
        let verb: Vec<u16> = "runas".encode_utf16().chain(std::iter::once(0)).collect();
        exe_info.lpVerb = verb.as_ptr();
        exe_info.lpFile = path_wide.as_ptr();
        exe_info.lpParameters = params_wide.as_ptr();
        exe_info.lpDirectory = dir_wide.as_ptr();
        exe_info.nShow = SW_HIDE;

        let ok = unsafe { ShellExecuteExW(&mut exe_info) };
        let err = unsafe { winapi::um::errhandlingapi::GetLastError() };
        if ok == 0 {
            dev_log(&format!(
                "[GUARD] ShellExecuteExW runas failed (err={err}); \
                     transparent mode disabled (UAC declined?)"
            ));
            return None;
        }
        if exe_info.hProcess.is_null() {
            dev_log("[GUARD] runas accepted but no process handle");
            return None;
        }
        let pid = unsafe { GetProcessId(exe_info.hProcess) };
        // We cannot transfer the elevated handle meaningfully to shutdown;
        // the guard self-terminates via parent-death + stop marker instead.
        let _ = unsafe { winapi::um::handleapi::CloseHandle(exe_info.hProcess) };
        Some(pid)
    }

    fn guard_stop_file() -> std::path::PathBuf {
        let base = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
        let dir = std::path::PathBuf::from(base).join("OziiDPI");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("guard.stop")
    }

    fn request_guard_stop() {
        let _ = std::fs::write(Self::guard_stop_file(), b"stop\n");
    }

    pub fn stop(&self) -> Result<(), OziiError> {
        {
            let mut s = self.state.lock().unwrap();
            if s.status == BackendStatus::Disconnected || s.status == BackendStatus::Stopping {
                return Err(OziiError::NotRunning);
            }
            if let Some(flag) = s.supervisor_running.take() {
                flag.store(false, Ordering::SeqCst);
            }
        }

        self.transition_state(BackendStatus::Stopping);

        self.shutdown_updater_protection();

        let res = self.session_manager.stop_transaction(&*self.proxy_impl);

        {
            let mut s = self.state.lock().unwrap();
            if let Some(mut engine) = s.engine.take() {
                let _ = engine.stop();
            }
            if let Some(adapter) = s.adapter.take() {
                adapter.stop();
            }
            if let Some(reporter) = s.reporter.take() {
                reporter.stop();
            }
            if let Some(pac_running) = s.pac_running.take() {
                pac_running.store(false, Ordering::Relaxed);
            }
            s.active_ports = None;
        }

        self.transition_state(BackendStatus::Disconnected);

        res.map_err(|_| OziiError::ProxyRestoreFailed)
    }

    pub fn status(&self) -> BackendStatus {
        let s = self.state.lock().unwrap();
        s.status.clone()
    }

    pub fn is_stop_requested(&self) -> bool {
        let s = self.state.lock().unwrap();
        let ds = s.diag_state.lock().unwrap();
        ds.stop_requested
    }

    pub fn diagnose(&self) -> Result<DiagnosticReport, OziiError> {
        let s = self.state.lock().unwrap();
        let ds = s.diag_state.lock().unwrap();

        // A dirty session only demands recovery when no backend is running.
        // While Connected, the session file intentionally exists.
        let dirty_session =
            matches!(self.session_manager.read_state(), Ok(Some(state)) if !state.clean_shutdown);
        let pac_active = s.status == BackendStatus::Connected;

        Ok(DiagnosticReport {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            engine_version: "dev".to_string(),
            windows_version: "10+".to_string(),
            mode: s
                .active_mode
                .as_ref()
                .map(|m| format!("{m:?}").to_lowercase())
                .unwrap_or_else(|| "UNKNOWN".to_string()),
            backend_state: s.status.clone(),
            pac_active,
            active_ports: s.active_ports.clone(),
            routing_metrics: RoutingMetrics {
                discord_forwarded: ds.discord_forwarded,
                non_discord_rejected: ds.non_discord_rejected,
                non_discord_forwarded: ds.non_discord_forwarded,
                discord_failed: ds.discord_failed,
            },
            error_codes: vec![],
            recovery_status: if dirty_session && s.status == BackendStatus::Disconnected {
                "RECOVERY_REQUIRED".to_string()
            } else {
                "OK".to_string()
            },
            updater_redirect_active: crate::hosts::updater_redirect_active(),
        })
    }

    pub fn recover(&self) -> Result<bool, OziiError> {
        self.emit(BackendEvent::RecoveryStarted);
        self.shutdown_updater_protection();
        let result = self
            .session_manager
            .recover(&*self.proxy_impl)
            .map_err(|_| OziiError::RecoveryFailed);
        let success = result.is_ok();
        self.emit(BackendEvent::RecoveryFinished { success });
        result
    }

    pub fn get_capabilities(&self) -> Capabilities {
        let recovery_required = if let Ok(Some(session)) = self.session_manager.read_state() {
            !session.clean_shutdown
        } else {
            false
        };

        Capabilities {
            strong_advanced_available: false, // E.g., no npcap
            npcap_available: false,
            engine_version: "dev".to_string(),
            browser_routing_supported: true,
            discord_desktop_supported: true,
            recovery_required,
        }
    }

    pub fn get_config(&self) -> AppConfig {
        self.config_manager.load()
    }

    pub fn update_config(&self, config: AppConfig) -> Result<(), OziiError> {
        self.config_manager
            .save(&config)
            .map_err(|e| OziiError::ConfigError(e.to_string()))
    }
}
