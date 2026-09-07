//! Local transparent TLS forwarder for Discord Desktop traffic.
//!
//! The Discord Desktop native updater bypasses both the system PAC and
//! Chromium proxy flags. When the scoped hosts redirect is active, the
//! updater connects to 127.0.0.1:443; this forwarder peeks the TLS
//! ClientHello SNI (without modifying or terminating TLS), verifies it
//! against the allowlist, opens a CONNECT tunnel through the DPI engine
//! and relays the raw stream so the engine can perform its normal
//! desynchronization on the outgoing ClientHello.
//!
//! No TLS termination. No payload inspection. Allowlist-enforced.

use crate::diagnostics::DiagnosticsState;
use crate::domain::DomainAllowlist;
use crate::tunnel::{dev_log, establish_engine_tunnel};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct UpdaterForwarder {
    pub port: u16,
    running: Arc<AtomicBool>,
}

/// The loopback address used for the scoped updater redirect.
pub const UPDATER_LISTEN_ADDR: &str = "127.0.0.2";
/// Unprivileged listener used by the process-aware WinDivert reflector.
/// The listener must bind INADDR_ANY because reflected packets retain the
/// machine's real interface address (the official WinDivert streamdump
/// pattern); binding loopback would make Windows reject those connections.
pub const TRANSPARENT_LISTEN_ADDR: &str = "0.0.0.0";
pub const TRANSPARENT_LISTEN_PORT: u16 = 39575;

impl UpdaterForwarder {
    /// Binds 127.0.0.2:443 (loopback). 127.0.0.2 is used so the scoped hosts
    /// redirect never collides with services already bound to 127.0.0.1:443.
    /// Returns None when the port is unavailable (feature then stays off;
    /// the rest of the product is unaffected).
    pub fn start(
        engine_port: u16,
        allowlist: Arc<DomainAllowlist>,
        diagnostics: Arc<Mutex<DiagnosticsState>>,
    ) -> Option<Self> {
        Self::start_on(
            UPDATER_LISTEN_ADDR,
            443,
            engine_port,
            allowlist,
            diagnostics,
        )
    }

    /// Starts the raw TLS receiver used by the process-aware Discord guard.
    /// `adapter_port` is the local HTTP CONNECT adapter, not a public target.
    pub fn start_transparent(
        adapter_port: u16,
        allowlist: Arc<DomainAllowlist>,
        diagnostics: Arc<Mutex<DiagnosticsState>>,
    ) -> Option<Self> {
        Self::start_on(
            TRANSPARENT_LISTEN_ADDR,
            TRANSPARENT_LISTEN_PORT,
            adapter_port,
            allowlist,
            diagnostics,
        )
    }

    fn start_on(
        listen_addr: &str,
        listen_port: u16,
        tunnel_port: u16,
        allowlist: Arc<DomainAllowlist>,
        diagnostics: Arc<Mutex<DiagnosticsState>>,
    ) -> Option<Self> {
        let listener = TcpListener::bind((listen_addr, listen_port)).ok()?;
        let running = Arc::new(AtomicBool::new(true));
        let r_flag = Arc::clone(&running);

        thread::spawn(move || {
            listener
                .set_nonblocking(true)
                .expect("listener nonblocking");
            while r_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let al = Arc::clone(&allowlist);
                        let diag = Arc::clone(&diagnostics);
                        let run = Arc::clone(&r_flag);
                        thread::spawn(move || {
                            if run.load(Ordering::Relaxed) {
                                handle(&mut stream, tunnel_port, al, diag);
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
                thread::sleep(Duration::from_millis(10));
            }
        });

        Some(Self {
            port: listen_port,
            running,
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// Extracts the server_name extension value from a TLS ClientHello.
/// Returns None for malformed/non-TLS input.
fn extract_sni(buf: &[u8]) -> Option<String> {
    if buf.len() < 5 || buf[0] != 0x16 {
        return None;
    }
    let rec_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
    let end = std::cmp::min(buf.len(), 5 + rec_len);
    let mut p = 5;
    if p >= end || buf[p] != 0x01 {
        return None; // not a ClientHello
    }
    p += 1;
    if p + 3 > end {
        return None;
    }
    let hs_len = ((buf[p] as usize) << 16) | ((buf[p + 1] as usize) << 8) | buf[p + 2] as usize;
    p += 3;
    let hs_end = std::cmp::min(end, p + hs_len);
    // client_version(2) + random(32)
    p += 34;
    if p >= hs_end {
        return None;
    }
    // session id
    let sid_len = buf[p] as usize;
    p += 1 + sid_len;
    if p + 2 > hs_end {
        return None;
    }
    // cipher suites
    let cs_len = u16::from_be_bytes([buf[p], buf[p + 1]]) as usize;
    p += 2 + cs_len;
    if p >= hs_end {
        return None;
    }
    // compression methods
    let cm_len = buf[p] as usize;
    p += 1 + cm_len;
    if p + 2 > hs_end {
        return None;
    }
    // extensions
    let ext_total = u16::from_be_bytes([buf[p], buf[p + 1]]) as usize;
    p += 2;
    let ext_end = std::cmp::min(hs_end, p + ext_total);
    while p + 4 <= ext_end {
        let ext_type = u16::from_be_bytes([buf[p], buf[p + 1]]);
        let ext_len = u16::from_be_bytes([buf[p + 2], buf[p + 3]]) as usize;
        let data_start = p + 4;
        let data_end = std::cmp::min(ext_end, data_start + ext_len);
        if ext_type == 0x0000 {
            // server_name_list: list_len(2) name_type(1) name_len(2) name
            let d = &buf[data_start..data_end];
            if d.len() >= 5 && d[2] == 0x00 {
                let name_len = u16::from_be_bytes([d[3], d[4]]) as usize;
                if d.len() >= 5 + name_len {
                    return std::str::from_utf8(&d[5..5 + name_len])
                        .ok()
                        .map(str::to_string);
                }
            }
            return None;
        }
        p = data_end;
    }
    None
}

/// Returns the extension type list of a ClientHello (dev diagnostics).
fn hello_extension_types(buf: &[u8]) -> Vec<u16> {
    let mut types = Vec::new();
    let mut p = 9usize; // record(5) + hs header(4)
    if buf.len() < 43 || buf[0] != 0x16 || buf[5] != 0x01 {
        return types;
    }
    p += 34; // version + random
    if p >= buf.len() {
        return types;
    }
    p += 1 + buf[p] as usize; // session id
    if p + 2 > buf.len() {
        return types;
    }
    let cs = u16::from_be_bytes([buf[p], buf[p + 1]]) as usize;
    p += 2 + cs;
    if p >= buf.len() {
        return types;
    }
    p += 1 + buf[p] as usize; // compression
    if p + 2 > buf.len() {
        return types;
    }
    let ext_total = u16::from_be_bytes([buf[p], buf[p + 1]]) as usize;
    p += 2;
    let end = std::cmp::min(buf.len(), p + ext_total);
    while p + 4 <= end {
        let t = u16::from_be_bytes([buf[p], buf[p + 1]]);
        let l = u16::from_be_bytes([buf[p + 2], buf[p + 3]]) as usize;
        types.push(t);
        p += 4 + l;
    }
    types
}

/// Relays engine -> client with FRAGMENTATION of the first server flight.
/// TLS 1.2 servers send the certificate chain in plaintext; DPI middleboxes
/// match the hostname inside the certificate and reset the flow. Splitting
/// the first bytes into tiny TCP segments defeats that match the same way
/// client-hello splitting does. After `fragment_bytes`, relaying is normal.
fn relay_engine_to_client_fragmented(
    engine: &mut TcpStream,
    client: &mut TcpStream,
    log_ctx: Option<String>,
) {
    use std::io::Write;

    const FRAGMENT_BYTES: usize = 8192;
    const FRAGMENT_SIZE: usize = 3;

    let _ = client.set_nodelay(true);
    let started = std::time::Instant::now();
    let e2c = Arc::new(AtomicU64::new(0));
    let c2e = Arc::new(AtomicU64::new(0));
    let fragmented_left: usize = FRAGMENT_BYTES;

    let mut e = engine.try_clone().expect("engine clone");
    let mut c = client.try_clone().expect("client clone");
    let e2c_counter = Arc::clone(&e2c);
    let frag = Arc::new(AtomicUsize::new(fragmented_left));
    let frag2 = Arc::clone(&frag);
    let t2 = thread::spawn(move || {
        let mut buf = [0u8; 16_384];
        let mut first = true;
        loop {
            match e.read(&mut buf) {
                Ok(0) => {
                    if first {
                        dev_log("[RELAY] engine->client pump: EOF on first read");
                    }
                    break;
                }
                Ok(n) => {
                    if first {
                        dev_log(&format!("[RELAY] engine->client first read: {n} bytes"));
                    }
                    first = false;
                    e2c_counter.fetch_add(n as u64, Ordering::Relaxed);
                    let mut written = false;
                    let left = frag2.load(Ordering::Relaxed);
                    if left > 0 {
                        let take = std::cmp::min(left, n);
                        for chunk in buf[..take].chunks(FRAGMENT_SIZE) {
                            if c.write_all(chunk).is_err() {
                                return;
                            }
                        }
                        frag2.store(left.saturating_sub(take), Ordering::Relaxed);
                        if take < n && c.write_all(&buf[take..]).is_err() {
                            return;
                        }
                        written = true;
                    }
                    if !written && c.write_all(&buf[..n]).is_err() {
                        return;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = c.shutdown(std::net::Shutdown::Write);
    });

    let mut c2 = client.try_clone().expect("client clone2");
    let mut e2 = engine.try_clone().expect("engine clone2");
    let c2e_counter = Arc::clone(&c2e);
    let t1 = thread::spawn(move || {
        let mut buf = [0u8; 16_384];
        loop {
            match c2.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    c2e_counter.fetch_add(n as u64, Ordering::Relaxed);
                    if e2.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = e2.shutdown(std::net::Shutdown::Write);
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

fn handle(
    client: &mut TcpStream,
    engine_port: u16,
    allowlist: Arc<DomainAllowlist>,
    diagnostics: Arc<Mutex<DiagnosticsState>>,
) {
    let _ = client.set_read_timeout(Some(Duration::from_secs(5)));

    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let complete = loop {
        if buf.len() > 64 * 1024 {
            dev_log("[FWD] runaway read");
            return;
        }
        match client.read(&mut chunk) {
            Ok(0) => {
                dev_log("[FWD] client closed before full read");
                return;
            }
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() >= 5 && buf[0] == 0x16 {
                    let rec_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
                    if buf.len() >= 5 + rec_len {
                        break true;
                    }
                } else if buf.len() >= 5 {
                    dev_log(&format!("[FWD] not TLS, first byte={:02x}", buf[0]));
                    break false;
                }
            }
            Err(e) => {
                dev_log(&format!("[FWD] read error: {e}"));
                return;
            }
        }
    };
    if !complete {
        return;
    }
    dev_log(&format!("[FWD] got {} byte ClientHello", buf.len()));
    let Some(sni) = extract_sni(&buf) else {
        dev_log("[FWD] SNI extraction failed");
        return;
    };
    dev_log(&format!("[FWD] SNI={sni}"));

    let is_allowed = {
        let mut d = diagnostics.lock().unwrap();
        d.observed_hosts.insert(format!("{sni} (updater-redirect)"));
        allowlist.is_allowed(&sni)
    };

    if !is_allowed {
        diagnostics.lock().unwrap().non_discord_rejected += 1;
        dev_log(&format!("[FWD] REJECT {sni}:443"));
        return;
    }

    dev_log(&format!("[FWD] connecting to engine on port {engine_port}"));
    let Ok(mut engine) = TcpStream::connect(("127.0.0.1", engine_port)) else {
        diagnostics.lock().unwrap().discord_failed += 1;
        dev_log(&format!("[FWD] FAILED engine_unreachable {sni}"));
        return;
    };

    let excess = match establish_engine_tunnel(&mut engine, &sni, 443) {
        Ok(excess) => {
            dev_log(&format!("[FWD] tunnel OK, excess={}", excess.len()));
            excess
        }
        Err(e) => {
            diagnostics.lock().unwrap().discord_failed += 1;
            dev_log(&format!("[FWD] FAILED engine_tunnel {sni}: {e}"));
            return;
        }
    };

    diagnostics.lock().unwrap().discord_forwarded += 1;
    let exts = hello_extension_types(&buf)
        .iter()
        .map(|t| format!("{t:#06x}"))
        .collect::<Vec<_>>()
        .join(",");
    // DEV ONLY: hex dump of the handshake metadata (never TLS payload).
    let hex: String = buf
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("");
    dev_log(&format!(
        "[ALLOW] {sni}:443 (updater-redirect) hello_bytes={} exts=[{exts}] hex={hex}",
        buf.len()
    ));

    // Replay the peeked ClientHello. `excess` belongs to the server->client
    // side of the CONNECT tunnel and must never be echoed back upstream.
    if engine.write_all(&buf).is_err() {
        diagnostics.lock().unwrap().discord_failed += 1;
        return;
    }
    if !excess.is_empty() {
        let _ = client.write_all(&excess);
    }
    let _ = client.set_read_timeout(None);
    let _ = engine.set_nodelay(true);
    relay_engine_to_client_fragmented(&mut engine, client, Some(format!("{sni}:443")));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_hello(sni: &str) -> Vec<u8> {
        // Build a minimal valid ClientHello with one SNI extension.
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(&[0x03, 0x03]); // client version
        body.extend_from_slice(&[0u8; 32]); // random
        body.push(0x00); // session id len
        body.extend_from_slice(&[0x00, 0x02, 0x13, 0x01]); // cipher suites
        body.push(0x01);
        body.push(0x00); // compression methods

        let name = sni.as_bytes();
        let mut sni_ext: Vec<u8> = Vec::new();
        sni_ext.extend_from_slice(&((name.len() + 3) as u16).to_be_bytes());
        sni_ext.push(0x00);
        sni_ext.extend_from_slice(&(name.len() as u16).to_be_bytes());
        sni_ext.extend_from_slice(name);

        let mut exts: Vec<u8> = Vec::new();
        exts.extend_from_slice(&0x0000u16.to_be_bytes());
        exts.extend_from_slice(&(sni_ext.len() as u16).to_be_bytes());
        exts.extend_from_slice(&sni_ext);

        body.extend_from_slice(&(exts.len() as u16).to_be_bytes());
        body.extend_from_slice(&exts);

        let mut hs: Vec<u8> = Vec::new();
        hs.push(0x01);
        let bl = body.len();
        hs.extend_from_slice(&[(bl >> 16) as u8, (bl >> 8) as u8, bl as u8]);
        hs.extend_from_slice(&body);

        let mut rec: Vec<u8> = Vec::new();
        rec.push(0x16);
        rec.extend_from_slice(&[0x03, 0x01]);
        rec.extend_from_slice(&(hs.len() as u16).to_be_bytes());
        rec.extend_from_slice(&hs);
        rec
    }

    #[test]
    fn test_extract_sni_valid() {
        let hello = client_hello("updates.discord.com");
        assert_eq!(extract_sni(&hello).as_deref(), Some("updates.discord.com"));
    }

    #[test]
    fn test_extract_sni_garbage() {
        assert_eq!(extract_sni(&[0u8; 16]), None);
        assert_eq!(extract_sni(&[]), None);
        let mut not_hello = vec![0x17, 0x03, 0x03, 0x00, 0x05, 1, 2, 3, 4, 5];
        not_hello.extend_from_slice(&[0u8; 32]);
        assert_eq!(extract_sni(&not_hello), None);
    }

    #[test]
    fn test_extract_sni_real_world_shape() {
        // Chrome-like hello with additional extensions after SNI.
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(&[0x03, 0x03]);
        body.extend_from_slice(&[0xABu8; 32]);
        body.push(0x00); // no session id
        body.extend_from_slice(&[0x00, 0x06, 0x13, 0x01, 0x13, 0x02, 0xc0, 0x2f]);
        body.push(0x01);
        body.push(0x00);
        let name = b"gateway.discord.gg";
        let mut sni_ext: Vec<u8> = Vec::new();
        sni_ext.extend_from_slice(&((name.len() + 3) as u16).to_be_bytes());
        sni_ext.push(0x00);
        sni_ext.extend_from_slice(&(name.len() as u16).to_be_bytes());
        sni_ext.extend_from_slice(name);
        let mut exts: Vec<u8> = Vec::new();
        exts.extend_from_slice(&0x0000u16.to_be_bytes());
        exts.extend_from_slice(&(sni_ext.len() as u16).to_be_bytes());
        exts.extend_from_slice(&sni_ext);
        // padding extension after sni
        exts.extend_from_slice(&0x0015u16.to_be_bytes());
        exts.extend_from_slice(&0x0001u16.to_be_bytes());
        exts.push(0x00);
        body.extend_from_slice(&(exts.len() as u16).to_be_bytes());
        body.extend_from_slice(&exts);
        let mut hs: Vec<u8> = vec![0x01];
        let bl = body.len();
        hs.extend_from_slice(&[(bl >> 16) as u8, (bl >> 8) as u8, bl as u8]);
        hs.extend_from_slice(&body);
        let mut rec = vec![0x16, 0x03, 0x01];
        rec.extend_from_slice(&(hs.len() as u16).to_be_bytes());
        rec.extend_from_slice(&hs);
        assert_eq!(extract_sni(&rec).as_deref(), Some("gateway.discord.gg"));
    }
}
