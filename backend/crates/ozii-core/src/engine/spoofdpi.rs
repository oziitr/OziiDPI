use crate::engine::{DpiEngine, DpiMode, EngineError};
use crate::process::ProcessIdentity;
use std::fs::File;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

pub struct SpoofDpiEngine {
    binary_path: PathBuf,
    process: Option<Child>,
    bind_port: u16,
}

impl SpoofDpiEngine {
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            binary_path,
            process: None,
            bind_port: 0,
        }
    }

    fn build_args(&self, bind_address: &str, port: u16, mode: &DpiMode) -> Vec<String> {
        let mut args = vec![
            "--listen-addr".to_string(),
            format!("{}:{}", bind_address, port),
            "--system-proxy=false".to_string(),
            "--silent".to_string(),
            // Deterministic resolver: UDP 8.8.8.8 bypasses the system hosts
            // file, which guarantees no redirect loop with the scoped
            // updater hosts entry (updates.discord.com -> 127.0.0.1).
            "--dns-mode".to_string(),
            "udp".to_string(),
            // Use the local gateway resolver — 8.8.8.8 (default) UDP is
            // blocked/blackholed on some ISP/DPI setups; the router always
            // resolves local networks correctly.
            "--dns-addr".to_string(),
            local_dns_addr(),
        ];

        match mode {
            DpiMode::Turbo => {
                args.push("--https-split-mode".to_string());
                args.push("sni".to_string());
            }
            DpiMode::Balanced { chunk_size } => {
                args.push("--https-split-mode".to_string());
                args.push("chunk".to_string());
                args.push("--https-chunk-size".to_string());
                args.push(chunk_size.to_string());
            }
            DpiMode::Strong => {
                args.push("--https-split-mode".to_string());
                args.push("chunk".to_string());
                args.push("--https-chunk-size".to_string());
                args.push("1".to_string());
            }
            DpiMode::StrongAdvanced { fake_count } => {
                args.push("--https-split-mode".to_string());
                args.push("chunk".to_string());
                args.push("--https-chunk-size".to_string());
                args.push("1".to_string());
                args.push("--https-fake-count".to_string());
                args.push(fake_count.to_string());
            }
        }

        args
    }
}

fn local_dns_addr() -> String {
    // Local gateway resolver — 8.8.8.8 (SpoofDPI default) UDP is blackholed
    // on some ISP/DPI setups; the router always resolves correctly.
    // Discovery via GetAdaptersAddresses avoided: use the common private
    // router addresses known to work for this deployment, defaulting to the
    // Windows DNS server list from the system resolver.
    let mut candidates = [
        "192.168.1.1:53".to_string(),
        "192.168.0.1:53".to_string(),
        "192.168.2.1:53".to_string(),
    ];
    // Prefer the FIRST system DNS server (read from registry — no admin).
    use crate::proxy_state::read_system_dns;
    if let Some(dns) = read_system_dns() {
        candidates[0] = dns;
    }
    candidates[0].clone()
}

impl DpiEngine for SpoofDpiEngine {
    fn start(
        &mut self,
        bind_address: &str,
        port: u16,
        mode: &DpiMode,
    ) -> Result<ProcessIdentity, EngineError> {
        if !self.binary_path.exists() {
            return Err(EngineError::BinaryNotFound(
                self.binary_path.to_string_lossy().to_string(),
            ));
        }

        if mode.requires_npcap() && !super::npcap::is_npcap_installed() {
            return Err(EngineError::MissingPrerequisite(
                "Npcap is required for StrongAdvanced mode".into(),
            ));
        }

        // Setup logging
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
        let mut log_dir = PathBuf::from(local_app_data);
        log_dir.push("OziiDPI");
        log_dir.push("logs");
        std::fs::create_dir_all(&log_dir)?;

        let log_file = File::create(log_dir.join("engine.log"))?;
        let err_file = log_file.try_clone()?;

        let args = self.build_args(bind_address, port, mode);

        let mut cmd = Command::new(&self.binary_path);
        cmd.args(&args)
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(err_file))
            .stdin(Stdio::null());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd.spawn()?;

        let pid = child.id();
        self.process = Some(child);
        self.bind_port = port;

        // Give it a moment to bind, looping for up to 1.5 seconds
        let mut started = false;
        for _ in 0..15 {
            std::thread::sleep(Duration::from_millis(100));
            if self.health_check() {
                started = true;
                break;
            }
        }

        if !started {
            // It failed to start properly or bind
            let _ = self.stop();
            return Err(EngineError::HealthCheckFailed(port));
        }

        ProcessIdentity::capture(pid)
            .ok_or_else(|| EngineError::Unknown("Failed to capture ProcessIdentity".into()))
    }

    fn stop(&mut self) -> Result<(), EngineError> {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait(); // Reap zombie
        }
        Ok(())
    }

    fn health_check(&self) -> bool {
        if self.bind_port == 0 {
            return false;
        }

        // Check if the process exited prematurely
        // We can't easily check child.try_wait() here since it requires a mutable borrow of self if self.process is Some,
        // but self is immutable here. That's fine, we will just try to connect to the TCP port.

        let addr = format!("127.0.0.1:{}", self.bind_port);
        TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_millis(500)).is_ok()
    }
}
