#![allow(clippy::collapsible_if)]
use clap::{Parser, Subcommand};
use ozii_core::engine::DpiMode;
use ozii_core::models::StartRequest;
use ozii_core::service::OziiService;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "ozii-dpi", version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Start {
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        chunk_size: Option<u8>,
        #[arg(long)]
        fake_count: Option<u8>,
    },
    /// Local-only mode: engine + adapter on loopback. Touches NO system
    /// proxy, PAC, hosts, DNS, firewall or any other global setting.
    /// Point a single app at 127.0.0.1:39572 (e.g. Discord --proxy-server).
    Local {
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        chunk_size: Option<u8>,
        #[arg(long)]
        fake_count: Option<u8>,
        /// Do not start the elevated WinDivert guard. Use this when every
        /// selected application is launched with an explicit proxy flag.
        #[arg(long)]
        no_guard: bool,
    },
    Stop,
    Status,
    Diagnose,
    /// Discord Web readiness test through the RUNNING adapter (dev diagnostic)
    DiscordCheck,
}

/// Runtime state persisted to disk so external tools can find the running backend.
#[derive(Debug, Serialize, Deserialize)]
struct RuntimeInfo {
    pid: u32,
    diagnostics_port: u16,
    engine_port: u16,
    adapter_port: u16,
    pac_port: u16,
    pac_url: String,
    mode: String,
    #[serde(default)]
    local_only: bool,
    started_at: u64,
}

fn runtime_info_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
    let mut p = PathBuf::from(local_app_data);
    p.push("OziiDPI");
    std::fs::create_dir_all(&p).unwrap_or_default();
    p.push("runtime.json");
    p
}

fn write_runtime_info(info: &RuntimeInfo) {
    let path = runtime_info_path();
    if let Ok(json) = serde_json::to_string_pretty(info) {
        let _ = std::fs::write(&path, json);
    }
}

fn read_runtime_info() -> Option<RuntimeInfo> {
    let path = runtime_info_path();
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn remove_runtime_info() {
    let path = runtime_info_path();
    let _ = std::fs::remove_file(&path);
}

/// Query the diagnostics HTTP endpoint of the RUNNING backend
fn query_diagnostics(port: u16) -> Option<String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let _ = stream
        .write_all(b"GET /diagnostics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    let response = String::from_utf8_lossy(&buf).to_string();
    // Extract JSON body after headers
    if let Some(pos) = response.find("\r\n\r\n") {
        Some(response[pos + 4..].to_string())
    } else {
        Some(response)
    }
}

/// Send stop request to the diagnostics HTTP endpoint
fn send_stop_request(port: u16) -> bool {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    if let Ok(mut stream) = TcpStream::connect(format!("127.0.0.1:{}", port)) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let _ = stream
            .write_all(b"POST /stop HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        let mut buf = [0; 256];
        let _ = stream.read(&mut buf);
        return true;
    }
    false
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            mode,
            chunk_size,
            fake_count,
        } => {
            let service = OziiService::new(None);

            // Check recovery
            if service.get_capabilities().recovery_required {
                println!("[!] Recovered from an unexpected previous shutdown.");
                let _ = service.recover();
            }

            let mut config = service.get_config();
            if let Some(m) = &mode {
                config.mode = m.clone();
            }
            if let Some(c) = chunk_size {
                config.chunk_size = c;
            }
            if let Some(f) = fake_count {
                config.fake_count = f;
            }

            let mode_str = config.mode.to_lowercase();
            let dpi_mode = match mode_str.as_str() {
                "turbo" => DpiMode::Turbo,
                "balanced" => DpiMode::Balanced {
                    chunk_size: config.chunk_size,
                },
                "strong" => DpiMode::Strong,
                "strong-advanced" => DpiMode::StrongAdvanced {
                    fake_count: config.fake_count,
                },
                _ => {
                    eprintln!("Invalid mode. Use: turbo, balanced, strong, strong-advanced");
                    std::process::exit(1);
                }
            };

            let req = StartRequest { mode: dpi_mode };

            match service.start(req) {
                Ok(_) => {
                    // Write runtime info so external tools can find us
                    if let Ok(diag) = service.diagnose() {
                        if let Some(ports) = &diag.active_ports {
                            let info = RuntimeInfo {
                                pid: std::process::id(),
                                diagnostics_port: ports.diagnostics,
                                engine_port: ports.engine,
                                adapter_port: ports.adapter,
                                pac_port: ports.pac,
                                pac_url: format!("http://127.0.0.1:{}/proxy.pac", ports.pac),
                                mode: mode_str.clone(),
                                local_only: false,
                                started_at: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            };
                            write_runtime_info(&info);
                        }
                    }

                    println!("OziiDPI is now ACTIVE.");
                    println!("Press Ctrl+C to stop...");

                    let running = Arc::new(AtomicBool::new(true));
                    let r = running.clone();

                    ctrlc::set_handler(move || {
                        r.store(false, Ordering::SeqCst);
                    })
                    .expect("Error setting Ctrl-C handler");

                    // Watchdog Loop — also checks for remote stop request
                    while running.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_secs(1));
                        let status = service.status();
                        if status == ozii_core::models::BackendStatus::Faulted {
                            eprintln!("CRITICAL ERROR: Engine process died unexpectedly!");
                            eprintln!("Initiating fail-open recovery...");
                            remove_runtime_info();
                            let _ = service.recover();
                            std::process::exit(1);
                        }
                        // Check if remote stop was requested via diagnostics endpoint
                        if service.is_stop_requested() {
                            println!("\nRemote stop requested...");
                            break;
                        }
                    }

                    println!("\nShutting down...");
                    remove_runtime_info();
                    if let Err(e) = service.stop() {
                        eprintln!("Error during shutdown: {}", e);
                        let _ = service.recover();
                    } else {
                        println!("Windows settings successfully restored to original state.");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to start OziiDPI: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Local {
            mode,
            chunk_size,
            fake_count,
            no_guard,
        } => {
            use std::net::TcpListener;

            let service_cfg = OziiService::new(None).get_config();
            let mut mode_str = mode
                .clone()
                .unwrap_or_else(|| service_cfg.mode.clone())
                .to_lowercase();
            if mode_str == "strong-advanced" {
                mode_str = "strong_advanced".to_string();
            }
            let mut c_size = chunk_size.unwrap_or(service_cfg.chunk_size);
            let f_count = fake_count.unwrap_or(service_cfg.fake_count);
            if c_size == 0 {
                c_size = 2;
            }
            let dpi_mode = match mode_str.as_str() {
                "turbo" => DpiMode::Turbo,
                "balanced" => DpiMode::Balanced { chunk_size: c_size },
                "strong" => DpiMode::Strong,
                "strong_advanced" => DpiMode::StrongAdvanced {
                    fake_count: f_count,
                },
                _ => {
                    eprintln!("Invalid mode. Use: turbo, balanced, strong, strong_advanced");
                    std::process::exit(1);
                }
            };

            // Locate the engine binary (same search order as the full service).
            let current_exe = std::env::current_exe().unwrap_or_default();
            let exe_dir = current_exe
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            let engine_path = {
                let mut p = exe_dir.join("ozii-dpi-engine.exe");
                if !p.exists() {
                    p = exe_dir.join("engine/bin/ozii-dpi-engine.exe");
                    if !p.exists() {
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
            };
            if !engine_path.exists() {
                eprintln!("Engine binary not found at {}", engine_path.display());
                std::process::exit(1);
            }

            // Deterministic preferred ports (fall back to free ones).
            let engine_port = match TcpListener::bind(("127.0.0.1", 39571)) {
                Ok(l) => {
                    drop(l);
                    39571
                }
                Err(_) => TcpListener::bind(("127.0.0.1", 0))
                    .ok()
                    .and_then(|l| l.local_addr().ok())
                    .map(|a| a.port())
                    .unwrap_or(39571),
            };
            let adapter_port = match TcpListener::bind(("127.0.0.1", 39572)) {
                Ok(l) => {
                    drop(l);
                    39572
                }
                Err(_) => TcpListener::bind(("127.0.0.1", 0))
                    .ok()
                    .and_then(|l| l.local_addr().ok())
                    .map(|a| a.port())
                    .unwrap_or(39572),
            };
            let diag_port = match TcpListener::bind(("127.0.0.1", 39574)) {
                Ok(l) => {
                    drop(l);
                    39574
                }
                Err(_) => TcpListener::bind(("127.0.0.1", 0))
                    .ok()
                    .and_then(|l| l.local_addr().ok())
                    .map(|a| a.port())
                    .unwrap_or(39574),
            };

            let mut local = match ozii_core::local::LocalProxy::start(
                engine_path,
                &dpi_mode,
                engine_port,
                adapter_port,
                diag_port,
            ) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to start local proxy: {e}");
                    std::process::exit(1);
                }
            };

            // Only the small WinDivert reflector is elevated. It observes
            // process metadata and reflects exact Discord.exe/Update.exe
            // TCP/443 flows; unrelated applications are sent back unchanged.
            let guard_pid = if no_guard {
                None
            } else {
                match local.transparent_forwarder_port() {
                    Some(forwarder_port) => match ozii_core::pktguard::spawn_elevated_reflector(
                        forwarder_port,
                        std::process::id(),
                    ) {
                        Ok(pid) => Some(pid),
                        Err(e) => {
                            eprintln!("Discord native updater guard unavailable: {e}");
                            eprintln!(
                                "The local proxy is still active; accept UAC on the next start."
                            );
                            None
                        }
                    },
                    None => {
                        eprintln!(
                            "Discord native updater guard unavailable: transparent receiver port is busy"
                        );
                        None
                    }
                }
            };

            let info = RuntimeInfo {
                pid: std::process::id(),
                diagnostics_port: diag_port,
                engine_port,
                adapter_port,
                pac_port: 0,
                pac_url: String::new(),
                mode: mode_str.clone(),
                local_only: true,
                started_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            write_runtime_info(&info);

            println!("OziiDPI LOCAL proxy is ACTIVE (system untouched).");
            println!("Engine:   127.0.0.1:{}", engine_port);
            println!("Adapter:  127.0.0.1:{}", adapter_port);
            if let Some(pid) = guard_pid {
                println!("Guard:    Discord-only (PID {})", pid);
            } else if no_guard {
                println!("Guard:    disabled (explicit per-app proxy mode)");
            }
            println!("Use:      --proxy-server=http://127.0.0.1:{}", adapter_port);
            println!("Press Ctrl+C to stop...");

            let running = Arc::new(AtomicBool::new(true));
            let r = running.clone();
            ctrlc::set_handler(move || {
                r.store(false, Ordering::SeqCst);
            })
            .expect("Error setting Ctrl-C handler");

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs(1));
                if local.is_stop_requested() {
                    println!("\nRemote stop requested...");
                    break;
                }
            }

            println!("\nShutting down local proxy...");
            local.stop();
            remove_runtime_info();
            println!("Local proxy stopped. System settings untouched.");
        }
        Commands::Stop => {
            // Try to find the running backend via runtime.json and send remote stop
            if let Some(info) = read_runtime_info() {
                println!("Found running OziiDPI (PID {})", info.pid);
                println!(
                    "Sending stop request to diagnostics port {}...",
                    info.diagnostics_port
                );
                if send_stop_request(info.diagnostics_port) {
                    println!("Stop request sent. Backend should shut down shortly.");
                    // Wait briefly for the process to clean up
                    std::thread::sleep(Duration::from_secs(3));
                    // Check if runtime.json was removed (indicates clean shutdown)
                    if !runtime_info_path().exists() {
                        println!("Windows settings successfully restored to original state.");
                    } else {
                        println!(
                            "[!] Runtime file still exists. Backend may still be shutting down."
                        );
                    }
                } else {
                    eprintln!("Could not reach running backend. It may have already exited.");
                    eprintln!("Attempting local recovery...");
                    let service = OziiService::new(None);
                    let _ = service.recover();
                    remove_runtime_info();
                    println!("Recovery complete.");
                }
            } else {
                // No runtime.json — try local recovery if session exists
                let service = OziiService::new(None);
                if service.get_capabilities().recovery_required {
                    println!("[!] No running backend found, but dirty session detected.");
                    println!("Running recovery...");
                    let _ = service.recover();
                    println!("Recovery complete. Windows settings restored.");
                } else {
                    println!("No running OziiDPI backend found.");
                }
            }
        }
        Commands::Status => {
            // Query the RUNNING backend's diagnostics endpoint
            if let Some(info) = read_runtime_info() {
                if let Some(json) = query_diagnostics(info.diagnostics_port) {
                    println!("========================================");
                    println!("OziiDPI Status (PID {})", info.pid);
                    println!("========================================");
                    println!("Mode:            {}", info.mode);
                    println!("Engine port:     127.0.0.1:{}", info.engine_port);
                    println!("Adapter port:    127.0.0.1:{}", info.adapter_port);
                    println!("PAC URL:         {}", info.pac_url);
                    println!("Diagnostics:     127.0.0.1:{}", info.diagnostics_port);
                    println!();
                    // Parse and display the JSON diagnostics
                    if let Ok(report) = serde_json::from_str::<serde_json::Value>(&json) {
                        println!(
                            "Discord forwarded:      {}",
                            report
                                .get("discord_forwarded")
                                .unwrap_or(&serde_json::Value::Null)
                        );
                        println!(
                            "Discord failed:         {}",
                            report
                                .get("discord_failed")
                                .unwrap_or(&serde_json::Value::Null)
                        );
                        println!(
                            "Non-Discord rejected:   {}",
                            report
                                .get("non_discord_rejected")
                                .unwrap_or(&serde_json::Value::Null)
                        );
                        println!(
                            "Non-Discord forwarded:  {}",
                            report
                                .get("non_discord_forwarded")
                                .unwrap_or(&serde_json::Value::Null)
                        );
                        if let Some(hosts) = report.get("observed_hosts") {
                            println!("Observed hosts:         {}", hosts);
                        }
                    } else {
                        println!("Raw diagnostics: {}", json);
                    }
                } else {
                    eprintln!(
                        "Runtime file exists (PID {}) but cannot reach diagnostics port.",
                        info.pid
                    );
                    eprintln!("Backend may have crashed.");
                }
            } else {
                println!("No running OziiDPI backend found.");
            }
        }
        Commands::Diagnose => {
            // Same as Status but more raw
            if let Some(info) = read_runtime_info() {
                if let Some(json) = query_diagnostics(info.diagnostics_port) {
                    println!("{}", json);
                } else {
                    eprintln!(
                        "Cannot reach running backend at port {}",
                        info.diagnostics_port
                    );
                }
            } else {
                eprintln!("No running OziiDPI backend found. Start one first.");
            }
        }
        Commands::DiscordCheck => match read_runtime_info() {
            Some(info) => {
                println!(
                    "OziiDPI Discord Web Readiness (adapter 127.0.0.1:{})",
                    info.adapter_port
                );
                let results = ozii_core::diagnostics::discord_readiness(
                    info.adapter_port,
                    Some(info.diagnostics_port),
                );
                for r in &results {
                    let verdict = if r.pass { "PASS" } else { "FAIL" };
                    println!("{:<17} {:<6} {}", r.name, verdict, r.detail);
                }
                if results.iter().all(|r| r.pass) {
                    println!("RESULT: PASS");
                } else {
                    println!("RESULT: FAIL");
                    std::process::exit(1);
                }
            }
            None => {
                eprintln!("No running OziiDPI backend found. Start one first.");
                std::process::exit(1);
            }
        },
    }
}
