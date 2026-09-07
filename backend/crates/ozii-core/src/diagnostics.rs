use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticReport {
    pub discord_forwarded: u64,
    pub discord_failed: u64,
    pub non_discord_rejected: u64,
    pub non_discord_forwarded: u64,
    pub observed_hosts: Vec<String>,
}

#[derive(Debug, Default)]
pub struct DiagnosticsState {
    pub discord_forwarded: u64,
    pub discord_failed: u64,
    pub non_discord_rejected: u64,
    pub non_discord_forwarded: u64,
    pub observed_hosts: HashSet<String>,
    pub stop_requested: bool,
}

impl DiagnosticsState {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }

    pub fn generate_report(&self) -> DiagnosticReport {
        let mut hosts: Vec<String> = self.observed_hosts.iter().cloned().collect();
        hosts.sort();
        DiagnosticReport {
            discord_forwarded: self.discord_forwarded,
            discord_failed: self.discord_failed,
            non_discord_rejected: self.non_discord_rejected,
            non_discord_forwarded: self.non_discord_forwarded,
            observed_hosts: hosts,
        }
    }
}

pub struct DiagnosticsReporter {
    pub port: u16,
    running: Arc<AtomicBool>,
}

impl DiagnosticsReporter {
    pub fn start(state: Arc<Mutex<DiagnosticsState>>, port: u16) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let r_flag = running.clone();

        thread::spawn(move || {
            if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port))
                && listener.set_nonblocking(true).is_ok()
            {
                while r_flag.load(Ordering::Relaxed) {
                    if let Ok((mut stream, _)) = listener.accept() {
                        let mut buf = [0; 512];
                        let _ = stream.read(&mut buf);
                        let request_str = String::from_utf8_lossy(&buf);
                        let is_stop = request_str.starts_with("POST /stop")
                            || request_str.starts_with("GET /stop");

                        if is_stop {
                            let mut lock = state.lock().unwrap();
                            lock.stop_requested = true;
                            let response = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\nOK";
                            let _ = stream.write_all(response.as_bytes());
                        } else {
                            let report = {
                                let lock = state.lock().unwrap();
                                lock.generate_report()
                            };

                            if let Ok(json) = serde_json::to_string(&report) {
                                let response = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                                    json.len(),
                                    json
                                );
                                let _ = stream.write_all(response.as_bytes());
                            }
                        }
                    }
                    thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        });

        Self { port, running }
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// One row of the Discord Web readiness report.
#[derive(Debug, Clone, Serialize)]
pub struct ReadinessCheck {
    pub name: &'static str,
    pub pass: bool,
    pub detail: String,
}

/// Performs a CONNECT-level probe through the running adapter:
/// client -> adapter -> SpoofDPI -> target. Returns the HTTP status the
/// tunnel returned (e.g. 200 = tunnel established end-to-end at TCP level).
pub fn connect_through_adapter(adapter_port: u16, host: &str, port: u16) -> Result<u16, String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect(("127.0.0.1", adapter_port))
        .map_err(|e| format!("adapter unreachable: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;

    let req = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("CONNECT write failed: {e}"))?;

    let mut buf = [0u8; 1024];
    let n = stream
        .read(&mut buf)
        .map_err(|e| format!("no CONNECT response: {e}"))?;
    let response = String::from_utf8_lossy(&buf[..n]);
    let status_line = response.lines().next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("malformed CONNECT response: {status_line}"))?;
    Ok(status)
}

fn https_get_via_adapter_curl(adapter_port: u16, url: &str) -> Option<String> {
    let out = std::process::Command::new("curl.exe")
        .args([
            "-sS",
            "--max-time",
            "15",
            "--proxy",
            &format!("http://127.0.0.1:{adapter_port}"),
            url,
        ])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

fn query_diag_counter(diag_port: u16) -> Option<DiagnosticReport> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect(("127.0.0.1", diag_port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let _ = stream
        .write_all(b"GET /diagnostics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    let mut body = Vec::new();
    let _ = stream.read_to_end(&mut body);
    let text = String::from_utf8_lossy(&body);
    let json = text.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(&text);
    serde_json::from_str(json.trim()).ok()
}

/// Discord Web readiness: verifies the critical Discord component classes
/// through the RUNNING adapter without credentials and without inspecting any
/// TLS payload. Supplementary to real browser testing.
pub fn discord_readiness(adapter_port: u16, diag_port: Option<u16>) -> Vec<ReadinessCheck> {
    let mut results = Vec::new();

    // 1. MAIN WEB
    match connect_through_adapter(adapter_port, "discord.com", 443) {
        Ok(s) if (200..300).contains(&s) => results.push(ReadinessCheck {
            name: "MAIN WEB",
            pass: true,
            detail: format!("discord.com CONNECT -> {s}"),
        }),
        other => results.push(ReadinessCheck {
            name: "MAIN WEB",
            pass: false,
            detail: format!("discord.com CONNECT -> {other:?}"),
        }),
    }

    // 2. API (gateway discovery endpoint, real HTTPS through the engine)
    match https_get_via_adapter_curl(adapter_port, "https://discord.com/api/v10/gateway") {
        Some(body) if body.contains("gateway.discord.gg") => results.push(ReadinessCheck {
            name: "API",
            pass: true,
            detail: "/api/v10/gateway returned gateway URL over HTTPS".to_string(),
        }),
        other => results.push(ReadinessCheck {
            name: "API",
            pass: false,
            detail: format!(
                "unexpected api response: {}",
                other
                    .as_deref()
                    .unwrap_or("<curl unavailable or empty>")
                    .chars()
                    .take(80)
                    .collect::<String>()
            ),
        }),
    }

    // 3. GATEWAY TLS (TCP/TLS path to the realtime gateway)
    match connect_through_adapter(adapter_port, "gateway.discord.gg", 443) {
        Ok(s) if (200..300).contains(&s) => results.push(ReadinessCheck {
            name: "GATEWAY TLS",
            pass: true,
            detail: format!("gateway.discord.gg CONNECT -> {s}"),
        }),
        other => results.push(ReadinessCheck {
            name: "GATEWAY TLS",
            pass: false,
            detail: format!("gateway.discord.gg CONNECT -> {other:?}"),
        }),
    }

    // 4. STATIC CDN
    match connect_through_adapter(adapter_port, "cdn.discordapp.com", 443) {
        Ok(s) if (200..300).contains(&s) => results.push(ReadinessCheck {
            name: "STATIC CDN",
            pass: true,
            detail: format!("cdn.discordapp.com CONNECT -> {s}"),
        }),
        other => results.push(ReadinessCheck {
            name: "STATIC CDN",
            pass: false,
            detail: format!("cdn.discordapp.com CONNECT -> {other:?}"),
        }),
    }

    // 5. NON-DISCORD LEAK (security invariant)
    match diag_port.and_then(query_diag_counter) {
        Some(report) => {
            let leak_free = report.non_discord_forwarded == 0;
            results.push(ReadinessCheck {
                name: "NON-DISCORD LEAK",
                pass: leak_free,
                detail: format!(
                    "non_discord_forwarded={} non_discord_rejected={} discord_forwarded={} discord_failed={}",
                    report.non_discord_forwarded,
                    report.non_discord_rejected,
                    report.discord_forwarded,
                    report.discord_failed
                ),
            });
        }
        None => results.push(ReadinessCheck {
            name: "NON-DISCORD LEAK",
            pass: false,
            detail: "diagnostics endpoint unreachable".to_string(),
        }),
    }

    results
}
