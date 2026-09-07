//! Shared raw tunnel relay used by the CONNECT adapter and the updater
//! redirect forwarder. Protocol-agnostic byte pumping with privacy-safe
//! development logging (metadata only, never payloads).

#![allow(clippy::module_name_repetitions)]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

/// Development-only, privacy-safe connection metadata logging.
/// Enabled exclusively via OZIIDPI_LOG_CONNECTIONS=1. Logs host:port,
/// allow/reject, tunnel duration and byte counts. Never logs payloads,
/// headers, cookies or TLS plaintext.
pub fn dev_log(line: &str) {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    let enabled = *ENABLED.get_or_init(|| std::env::var("OZIIDPI_LOG_CONNECTIONS").is_ok());
    if !enabled {
        return;
    }
    let path = std::env::var("OZIIDPI_CONN_LOG_PATH").unwrap_or_else(|_| {
        let base = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
        format!("{base}\\OziiDPI\\logs\\connections.log")
    });
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{timestamp} {line}");
    }
}

fn pump(
    mut from: TcpStream,
    mut to: TcpStream,
    counter: Arc<AtomicU64>,
    label: &'static str,
    host: Arc<String>,
) {
    let mut buf = [0u8; 16_384];
    loop {
        match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                counter.fetch_add(n as u64, Ordering::Relaxed);
                if to.write_all(&buf[..n]).is_err() {
                    dev_log(&format!("[PUMP-EXIT] {host} {label} write_failed"));
                    break;
                }
            }
            Err(e) => {
                dev_log(&format!(
                    "[PUMP-EXIT] {host} {label} read_err={:?}",
                    e.kind()
                ));
                break;
            }
        }
    }
    let _ = from.shutdown(Shutdown::Read);
    let _ = to.shutdown(Shutdown::Write);
}

/// Relays raw bytes between client and engine until both directions end.
/// `log_ctx` enables the dev-only [CLOSE] metadata line.
pub fn relay_streams(client: &mut TcpStream, engine: &mut TcpStream, log_ctx: Option<String>) {
    let _ = client.set_nonblocking(false);
    let _ = client.set_read_timeout(None);
    let _ = client.set_write_timeout(None);
    let _ = engine.set_nonblocking(false);
    let _ = engine.set_read_timeout(None);
    let _ = engine.set_write_timeout(None);

    let started = std::time::Instant::now();
    let c1 = match client.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let e1 = match engine.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let c2 = match client.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let e2 = match engine.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };

    let c2e = Arc::new(AtomicU64::new(0));
    let e2c = Arc::new(AtomicU64::new(0));

    let c2e_counter = Arc::clone(&c2e);
    let host_arc = Arc::new(log_ctx.clone().unwrap_or_default());
    let h1 = Arc::clone(&host_arc);
    let t1 = thread::spawn(move || {
        pump(c1, e1, c2e_counter, "c2e", h1);
    });

    let e2c_counter = Arc::clone(&e2c);
    let h2 = Arc::clone(&host_arc);
    let t2 = thread::spawn(move || {
        pump(e2, c2, e2c_counter, "e2c", h2);
    });

    let _ = t1.join();
    let _ = t2.join();

    if let Some(host_port) = log_ctx {
        dev_log(&format!(
            "[CLOSE] {host_port} duration_ms={} bytes_client_to_engine={} bytes_engine_to_client={}",
            started.elapsed().as_millis(),
            c2e.load(Ordering::Relaxed),
            e2c.load(Ordering::Relaxed)
        ));
    }
}

/// Sends a CONNECT request to the engine and waits for the complete tunnel
/// establishment response header block. The response may arrive split across
/// multiple TCP segments, so we read until the `\r\n\r\n` terminator.
/// Returns any bytes received beyond the header end — they belong to the
/// engine->client stream and MUST be forwarded to the client by the caller.
pub fn establish_engine_tunnel(
    engine: &mut TcpStream,
    host: &str,
    port: u16,
) -> Result<Vec<u8>, String> {
    engine
        .set_read_timeout(Some(std::time::Duration::from_secs(15)))
        .map_err(|e| e.to_string())?;
    let req = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n");
    engine
        .write_all(req.as_bytes())
        .map_err(|e| format!("engine CONNECT write failed: {e}"))?;

    let mut buf: Vec<u8> = Vec::with_capacity(256);
    let mut chunk = [0u8; 512];
    let header_end = loop {
        let n = engine
            .read(&mut chunk)
            .map_err(|e| format!("engine CONNECT read failed: {e}"))?;
        if n == 0 {
            return Err("engine closed during CONNECT".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > 16 * 1024 {
            return Err("engine CONNECT response too large".to_string());
        }
    };

    let status_line = String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("malformed engine response: {status_line}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("engine refused CONNECT: {status_line}"));
    }

    engine.set_read_timeout(None).map_err(|e| e.to_string())?;
    Ok(buf.split_off(header_end))
}
