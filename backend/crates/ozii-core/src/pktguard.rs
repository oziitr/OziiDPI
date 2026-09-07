//! Packet-level DPI circumvention guard (Discord-only scope).
//!
//! A WinDivert loop that intercepts the *first* TLS ClientHello of each new
//! outbound TCP connection that targets a Discord server (destination IPs
//! resolved from the domain allowlist at activation time), fragments that
//! ClientHello across multiple TCP segments (so mid-path DPI can no longer
//! observe the SNI inside a single packet), and re-injects the fragments.
//! Every other packet of the flow â€” and every non-Discord flow â€” continues
//! through the kernel untouched. No system proxy, no hosts redirect: the
//! Discord Desktop app and its updater become transparently bypassed.
//!
//! Requires elevation (driver access); the launcher spawns this as an
//! elevated helper process that self-exits shortly after its parent dies.

#![allow(dead_code)]

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use etherparse::{IpHeaders, IpSlice, PacketBuilder};
use windivert::WinDivert;
use windivert::layer::NetworkLayer;
use windivert::prelude::{WinDivertError, WinDivertFlags, WinDivertRecvError};

use crate::engine::DpiMode;
use crate::tunnel::dev_log;

const CAPTURE_PORT: u16 = 443;

#[derive(Debug, Clone)]
pub struct GuardOptions {
    /// Domain names whose resolved IPs are considered Discord targets.
    pub domains: Vec<String>,
    pub mode: DpiMode,
    /// When set, the guard self-terminates shortly after this PID disappears.
    pub parent_pid: Option<u32>,
    /// Normal user's LocalAppData path, supplied before UAC elevation.
    pub local_app_data: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GuardStats {
    pub captured: u64,
    pub fragmented: u64,
    pub fallback_passthrough: u64,
}

/// Resolves target IPs (A + AAAA) via the system DNS for the given domains.
pub fn resolve_target_ips(domains: &[String]) -> Vec<IpAddr> {
    let mut ips: Vec<IpAddr> = Vec::new();
    for domain in domains {
        let host = format!("{domain}:{CAPTURE_PORT}");
        if let Ok(iter) = host.to_socket_addrs() {
            for addr in iter {
                let ip = addr.ip();
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
    }
    dev_log(&format!(
        "[GUARD] resolved {} target IPs from {} domains",
        ips.len(),
        domains.len()
    ));
    ips
}

fn build_filter(ips: &[IpAddr]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for ip in ips {
        match ip {
            IpAddr::V4(v4) => parts.push(format!("ip.DstAddr == {v4}")),
            IpAddr::V6(v6) => parts.push(format!("ipv6.DstAddr == {v6}")),
        }
    }
    if parts.is_empty() {
        parts.push("false".to_string());
    }
    let dst = format!("({})", parts.join(" or "));
    format!(
        "outbound and tcp and tcp.DstPort == {CAPTURE_PORT} \
         and tcp.Payload[0] == 22 and tcp.Payload[5] == 1 \
         and !impostor and {dst}"
    )
}

type Seg = (u32, Option<u32>);

pub fn segment_order(mode: &DpiMode) -> Vec<Seg> {
    match mode {
        DpiMode::Turbo => vec![(0, None)], // dev: single segment re-inject
        DpiMode::Balanced { chunk_size } => {
            let c = (*chunk_size).clamp(1, 128) as u32;
            vec![(0, Some(c)), (c, None)]
        }
        DpiMode::Strong | DpiMode::StrongAdvanced { .. } => {
            vec![(0, Some(1)), (1, Some(2)), (2, None)]
        }
    }
}

fn wants_fake(mode: &DpiMode) -> bool {
    matches!(mode, DpiMode::StrongAdvanced { fake_count } if *fake_count > 0)
}

struct PktView<'a> {
    ip: IpSlice<'a>,
    tcp: etherparse::TcpSlice<'a>,
}

impl<'a> PktView<'a> {
    fn parse(raw: &'a [u8]) -> Option<Self> {
        let ip = IpSlice::from_slice(raw).ok()?;
        let tcp = etherparse::TcpSlice::from_slice(ip.payload().payload).ok()?;
        Some(Self { ip, tcp })
    }

    fn is_client_hello(&self) -> bool {
        let p = self.tcp.payload();
        p.len() > 6 && p[0] == 0x16 && p[5] == 0x01
    }

    fn daddr(&self) -> IpAddr {
        self.ip.destination_addr()
    }

    fn saddr(&self) -> IpAddr {
        self.ip.source_addr()
    }

    fn ttl(&self) -> u8 {
        match &self.ip {
            IpSlice::Ipv4(v4) => v4.header().ttl(),
            IpSlice::Ipv6(v6) => v6.header().hop_limit(),
        }
    }

    fn payload(&self) -> &'a [u8] {
        self.tcp.payload()
    }
}

/// Builds one IP/TCP packet whose TCP payload is `payload[start..end]`,
/// reusing original header fields/options and offsetting the sequence
/// number by `start`.
fn build_segment(
    view: &PktView,
    start: u32,
    end: Option<u32>,
    payload_override: Option<&[u8]>,
    ttl_override: Option<u8>,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let payload = payload_override.unwrap_or_else(|| view.payload());
    let payload_len = payload.len() as u32;
    let start = start.min(payload_len);
    let end = end.map(|e| e.min(payload_len)).unwrap_or(payload_len);
    if start >= end || start > payload_len {
        return Err("segment out of bounds".to_string());
    }
    let slice = &payload[start as usize..end as usize];

    let mut tcp_hdr = view.tcp.to_header();
    tcp_hdr.sequence_number = tcp_hdr.sequence_number.wrapping_add(start);
    let tcp_options = view.tcp.options();

    out.clear();
    match &view.ip {
        IpSlice::Ipv4(v4) => {
            let mut ip_hdr = v4.header().to_header();
            if let Some(t) = ttl_override {
                ip_hdr.time_to_live = t;
            }
            let builder = PacketBuilder::ip(IpHeaders::Ipv4(ip_hdr, v4.extensions().to_header()))
                .tcp_header(tcp_hdr)
                .options_raw(tcp_options)
                .map_err(|e| e.to_string())?;
            builder.write(out, slice).map_err(|e| e.to_string())?;
        }
        IpSlice::Ipv6(v6) => {
            let mut ip6_hdr = v6.header().to_header();
            if let Some(t) = ttl_override {
                ip6_hdr.hop_limit = t;
            }
            let builder = PacketBuilder::ip(IpHeaders::Ipv6(ip6_hdr, Default::default()))
                .tcp_header(tcp_hdr)
                .options_raw(tcp_options)
                .map_err(|e| e.to_string())?;
            builder.write(out, slice).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn run_guard(opts: &GuardOptions) -> Result<GuardStats, String> {
    let ips = resolve_target_ips(&opts.domains);
    if ips.is_empty() {
        return Err("no Discord target IPs resolved; aborting".to_string());
    }
    let filter = build_filter(&ips);
    eprintln!("[ozii-guard] resolved {} IPs", ips.len());
    eprintln!("[ozii-guard] filter: {filter}");
    dev_log(&format!("[GUARD] filter: {filter}"));

    let recv_wd = WinDivert::network(&filter, -8, WinDivertFlags::new())
        .map_err(|e| format!("recv handle: {e}"))?;
    eprintln!("[ozii-guard] handles open");

    let stats = Arc::new(Mutex::new(GuardStats::default()));
    let hop_table: Arc<Mutex<HashMap<IpAddr, u8>>> = Arc::new(Mutex::new(HashMap::new()));

    // Optional hop-learning sniff for fake ClientHello (TTL-based fake must
    // expire one hop before the server so the real handshake still lands).
    let sniff_wd = if wants_fake(&opts.mode) {
        let wd = WinDivert::network(
            "!outbound and tcp and tcp.SrcPort == 443 and tcp.Syn and tcp.Ack and !impostor",
            -8,
            WinDivertFlags::new(),
        )
        .map_err(|e| format!("sniff handle: {e}"))?;
        let table = Arc::clone(&hop_table);
        let wd_arc = Arc::new(wd);
        let wd_thread = Arc::clone(&wd_arc);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            loop {
                match wd_thread.recv(&mut buf) {
                    Ok(pkt) => {
                        if let Some(v) = PktView::parse(&pkt.data) {
                            let hop = infer_hops(v.ttl());
                            table.lock().unwrap().insert(v.saddr(), hop);
                        }
                        let _ = wd_thread.send(&pkt);
                    }
                    Err(WinDivertError::Recv(WinDivertRecvError::NoData)) => break,
                    Err(e) => {
                        eprintln!("[ozii-guard] sniff recv: {e}");
                        break;
                    }
                }
            }
        });
        Some(wd_arc)
    } else {
        None
    };

    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let parent_pid = opts.parent_pid;

    std::thread::spawn(move || {
        let watcher = parent_pid.filter(|p| *p != 0).and_then(PidWatcher::spawn);
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                return;
            }
            if guard_stop_marker_exists() {
                dev_log("[GUARD] stop marker set; shutting down");
                let _ = std::fs::remove_file(guard_stop_file());
                stop_flag.store(true, Ordering::SeqCst);
                return;
            }
            if let Some(w) = &watcher
                && !w.is_alive()
            {
                dev_log("[GUARD] parent died; shutting down");
                stop_flag.store(true, Ordering::SeqCst);
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });

    dev_log("[GUARD] entering capture loop");
    let mut raw_buf = vec![0u8; 65536];
    let mut scratch: Vec<u8> = Vec::with_capacity(8192);
    let order = segment_order(&opts.mode);
    let want_fake = wants_fake(&opts.mode);

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let pkt = match recv_wd.recv(&mut raw_buf) {
            Ok(p) => p,
            Err(WinDivertError::Recv(WinDivertRecvError::NoData)) => break,
            Err(WinDivertError::Recv(e)) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                eprintln!("[ozii-guard] recv error: {e}");
                break;
            }
            Err(e) => {
                eprintln!("[ozii-guard] recv fatal: {e}");
                break;
            }
        };

        if !pkt.address.outbound() {
            let _ = recv_wd.send(&pkt);
            stats.lock().unwrap().fallback_passthrough += 1;
            continue;
        }

        let view = match PktView::parse(&pkt.data) {
            Some(v) => v,
            None => {
                let _ = recv_wd.send(&pkt);
                stats.lock().unwrap().fallback_passthrough += 1;
                continue;
            }
        };

        if !view.is_client_hello() {
            let _ = recv_wd.send(&pkt);
            stats.lock().unwrap().fallback_passthrough += 1;
            continue;
        }

        stats.lock().unwrap().captured += 1;

        // Dev: re-send the original packet bytes untouched.
        if std::env::var("OZIIDPI_GUARD_RAWSEND").is_ok() {
            let mut a = pkt.address.clone();
            a.set_outbound(true);
            a.set_impostor(true);
            a.set_ip_checksum(false);
            a.set_tcp_checksum(false);
            let spkt = unsafe {
                windivert::packet::WinDivertPacket::<NetworkLayer>::new(pkt.data.to_vec())
            };
            if recv_wd.send(&spkt).is_err() {
                eprintln!("[ozii-guard] rawsend failed");
            }
            stats.lock().unwrap().fragmented += 1;
            continue;
        }

        // Rebuild fragments and inject them (originals are dropped).
        let payload_len = view.payload().len() as u32;
        let mut sent_ok = true;

        if want_fake {
            let hop = hop_table.lock().unwrap().get(&view.daddr()).copied();
            let fake_ttl = hop.map(|h| h.saturating_sub(1).max(1)).unwrap_or(8);
            let fake_hello = fake_client_hello();
            if let Err(e) = build_segment(
                &view,
                0,
                None,
                Some(&fake_hello),
                Some(fake_ttl),
                &mut scratch,
            ) {
                eprintln!("[ozii-guard] fake build: {e}");
            } else {
                let mut faddr = pkt.address.clone();
                faddr.set_outbound(true);
                faddr.set_impostor(true);
                faddr.set_ip_checksum(false);
                faddr.set_tcp_checksum(false);
                let spkt = unsafe {
                    windivert::packet::WinDivertPacket::<NetworkLayer>::new(scratch.clone())
                };
                if recv_wd.send(&spkt).is_err() {
                    sent_ok = false;
                }
            }
        }

        for (start, end) in &order {
            if *start >= payload_len {
                continue;
            }
            if let Err(e) = build_segment(&view, *start, *end, None, None, &mut scratch) {
                eprintln!("[ozii-guard] build_segment({start:?},{end:?}): {e}");
                sent_ok = false;
                break;
            }
            let mut faddr = pkt.address.clone();
            faddr.set_outbound(true);
            faddr.set_impostor(true);
            faddr.set_ip_checksum(false);
            faddr.set_tcp_checksum(false);
            let spkt =
                unsafe { windivert::packet::WinDivertPacket::<NetworkLayer>::new(scratch.clone()) };
            if recv_wd.send(&spkt).is_err() {
                eprintln!("[ozii-guard] send failed");
                sent_ok = false;
                break;
            }
        }

        if sent_ok {
            stats.lock().unwrap().fragmented += 1;
        }
        let captured = stats.lock().unwrap().captured;
        if captured % 25 == 0 {
            eprintln!(
                "[ozii-guard] captured={captured} fragmented={} last_dst={}",
                stats.lock().unwrap().fragmented,
                view.daddr()
            );
        }
    }

    if let Some(wd) = sniff_wd {
        let _ = wd.shutdown_handle().shutdown();
    }
    let _ = recv_wd.shutdown_handle().shutdown();
    recv_wd.close();

    let s = stats.lock().unwrap().clone();
    dev_log(&format!(
        "[GUARD] exited captured={} fragmented={} fallback={}",
        s.captured, s.fragmented, s.fallback_passthrough
    ));
    Ok(s)
}

// ---------------------------------------------------------------------------
// Process-aware TCP reflector (Discord Desktop only)
//
// The SOCKET layer supplies PID + exact 5-tuple before the corresponding
// network flow is established. Only canonical Discord executables may add a
// tuple to the allow cache. A single NETWORK handle is already open before
// Discord connects, so there is no per-flow handle race. It reflects only an
// exact cached tuple into the raw TLS receiver. Every unrelated packet is
// re-injected byte-for-byte with its original WINDIVERT_ADDRESS.
//
// Reflection follows WinDivert's official streamdump design: swap source and
// destination IPs, change the destination port to the local receiver, and
// inject inbound. The receiver's replies are reflected back in the opposite
// direction. The engine's own TCP/443 flow never enters the cache and is
// therefore always passed through unchanged; proxy loops are impossible.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FlowKey {
    local_addr: Ipv4Addr,
    local_port: u16,
    remote_addr: Ipv4Addr,
    remote_port: u16,
    protocol: u8,
}

#[derive(Debug, Clone, Copy)]
struct FlowRecord {
    active: bool,
    expires_at: std::time::Instant,
}

/// Resolves a process id to its full executable path. A failure is always
/// fail-open: the flow is not tunneled.
fn exe_path_of(pid: u32) -> Option<String> {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winbase::QueryFullProcessImageNameW;
    use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;

    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return None;
    }
    let mut buf = [0u16; 512];
    let mut size: u32 = 512;
    let ok = unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size) };
    let _ = unsafe { CloseHandle(h) };
    if ok == 0 || size == 0 || size as usize > 512 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..size as usize]))
}

/// Resolves an IPv4 TCP socket's owning PID from Windows' authoritative TCP
/// table. This is used only for a new SYN when the WinDivert SOCKET event did
/// not bind cleanly; subsequent packets use the cached exact 5-tuple.
fn tcp4_owner_pid(local_port: u16, remote_port: u16) -> Option<u32> {
    use std::ptr;
    use winapi::shared::iprtrmib::TCP_TABLE_OWNER_PID_ALL;
    use winapi::shared::tcpmib::MIB_TCPROW_OWNER_PID;
    use winapi::um::iphlpapi::GetExtendedTcpTable;

    const AF_INET: u32 = 2;
    const NO_ERROR: u32 = 0;
    let mut size = 0u32;
    unsafe {
        let _ = GetExtendedTcpTable(
            ptr::null_mut(),
            &mut size,
            0,
            AF_INET,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
    }
    if size < 4 {
        return None;
    }

    // u32 backing guarantees sufficient alignment for the MIB rows.
    let mut storage = vec![0u32; (size as usize).div_ceil(4)];
    let status = unsafe {
        GetExtendedTcpTable(
            storage.as_mut_ptr().cast(),
            &mut size,
            0,
            AF_INET,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if status != NO_ERROR {
        return None;
    }

    let count = storage[0] as usize;
    let rows = unsafe { storage.as_ptr().add(1).cast::<MIB_TCPROW_OWNER_PID>() };
    for index in 0..count {
        let row = unsafe { &*rows.add(index) };
        let row_local = (row.dwLocalPort as u16).swap_bytes();
        let row_remote = (row.dwRemotePort as u16).swap_bytes();
        if row_local == local_port && row_remote == remote_port {
            return Some(row.dwOwningPid);
        }
    }
    None
}

/// True when `exe` is a canonical Discord Desktop executable:
/// %LOCALAPPDATA%\Discord\Update.exe or %LOCALAPPDATA%\Discord\app-*\Discord.exe
fn is_eligible_discord(exe: &str) -> bool {
    let Ok(base) = std::env::var("LOCALAPPDATA") else {
        return false;
    };
    is_eligible_discord_under(exe, &base)
}

fn normalize_windows_path(path: &str) -> String {
    let normalized = path.replace('/', "\\").to_lowercase();
    normalized
        .strip_prefix("\\\\?\\")
        .unwrap_or(&normalized)
        .trim_end_matches('\\')
        .to_string()
}

fn is_eligible_discord_under(exe: &str, local_app_data: &str) -> bool {
    let base = normalize_windows_path(local_app_data);
    let prefix = format!("{base}\\discord\\");
    let exe = normalize_windows_path(exe);
    let Some(rest) = exe.strip_prefix(&prefix) else {
        return false;
    };
    if rest == "update.exe" {
        return true;
    }
    let mut parts = rest.split('\\');
    let app_dir = parts.next().unwrap_or_default();
    let file = parts.next().unwrap_or_default();
    parts.next().is_none()
        && app_dir.starts_with("app-")
        && app_dir.len() > "app-".len()
        && file == "discord.exe"
}

fn guard_trace(local_app_data: Option<&str>, line: &str) {
    use std::io::Write;
    let Some(root) = local_app_data else {
        return;
    };
    let dir = std::path::PathBuf::from(root).join("OziiDPI");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("guard.trace.log"))
    {
        let _ = writeln!(file, "{line}");
    }
}

fn normalize_socket_ports(local: u16, remote: u16) -> Option<(u16, u16)> {
    if remote == CAPTURE_PORT {
        Some((local, remote))
    } else if remote.swap_bytes() == CAPTURE_PORT {
        Some((local.swap_bytes(), remote.swap_bytes()))
    } else {
        None
    }
}

fn socket_flow_key(
    address: &windivert::address::WinDivertAddress<windivert::layer::SocketLayer>,
) -> Option<FlowKey> {
    let (local_port, remote_port) =
        normalize_socket_ports(address.local_port(), address.remote_port())?;
    let (IpAddr::V4(local_addr), IpAddr::V4(remote_addr)) =
        (address.local_address(), address.remote_address())
    else {
        return None;
    };
    if address.protocol() != 6 || local_port == 0 {
        return None;
    }
    Some(FlowKey {
        local_addr,
        local_port,
        remote_addr,
        remote_port,
        protocol: 6,
    })
}

fn ipv4_packet_flow(data: &[u8]) -> Option<(FlowKey, usize, u8)> {
    if data.len() < 40 || data[0] >> 4 != 4 || data[9] != 6 {
        return None;
    }
    let ihl = ((data[0] & 0x0f) as usize) * 4;
    if ihl < 20 || data.len() < ihl + 20 {
        return None;
    }
    let key = FlowKey {
        local_addr: Ipv4Addr::new(data[12], data[13], data[14], data[15]),
        local_port: u16::from_be_bytes([data[ihl], data[ihl + 1]]),
        remote_addr: Ipv4Addr::new(data[16], data[17], data[18], data[19]),
        remote_port: u16::from_be_bytes([data[ihl + 2], data[ihl + 3]]),
        protocol: 6,
    };
    Some((key, ihl, data[ihl + 13]))
}

fn reflect_client_packet(
    pkt: &mut windivert::packet::WinDivertPacket<'_, NetworkLayer>,
    ihl: usize,
    forwarder_port: u16,
) {
    let raw = pkt.data.to_mut();
    let src = [raw[12], raw[13], raw[14], raw[15]];
    let dst = [raw[16], raw[17], raw[18], raw[19]];
    raw[12..16].copy_from_slice(&dst);
    raw[16..20].copy_from_slice(&src);
    raw[ihl + 2..ihl + 4].copy_from_slice(&forwarder_port.to_be_bytes());
    raw[ihl + 10..ihl + 12].fill(0);
    raw[ihl + 16..ihl + 18].fill(0);
    pkt.address.set_outbound(false);
    calc_checksums(pkt, false);
}

fn reflect_forwarder_packet(
    pkt: &mut windivert::packet::WinDivertPacket<'_, NetworkLayer>,
    ihl: usize,
) {
    let raw = pkt.data.to_mut();
    let src = [raw[12], raw[13], raw[14], raw[15]];
    let dst = [raw[16], raw[17], raw[18], raw[19]];
    raw[12..16].copy_from_slice(&dst);
    raw[16..20].copy_from_slice(&src);
    raw[ihl..ihl + 2].copy_from_slice(&CAPTURE_PORT.to_be_bytes());
    raw[ihl + 10..ihl + 12].fill(0);
    raw[ihl + 16..ihl + 18].fill(0);
    pkt.address.set_outbound(false);
    calc_checksums(pkt, false);
}

/// Recomputes IP/TCP checksums via WinDivert's helper (flags 0 = compute all).
fn calc_checksums(pkt: &mut windivert::packet::WinDivertPacket<'_, NetworkLayer>, _flags: bool) {
    let _ = pkt.recalculate_checksums(windivert_sys::ChecksumFlags::new());
}

pub fn run_dnat_proxy(
    opts: &GuardOptions,
    forwarder_port: u16,
    _engine_port: u16,
) -> Result<GuardStats, String> {
    let _ = (&opts.domains, &opts.mode); // routing never uses domains/IP lists
    dev_log("[REFLECT] process-aware Discord-only guard starting");
    eprintln!("[ozii-guard] Discord-only reflector -> port {forwarder_port}");

    // Open both handles before receiving anything. SOCKET is sniff-only: it
    // can never block an unrelated connect() even if this process stalls.
    let socket_handle = Arc::new(
        WinDivert::socket(
            "outbound and event == CONNECT and tcp and remotePort == 443",
            110,
            WinDivertFlags::new().set_sniff(),
        )
        .or_else(|_| {
            WinDivert::socket(
                "outbound and event == CONNECT and remotePort == 443",
                110,
                WinDivertFlags::new().set_sniff(),
            )
        })
        .map_err(|e| format!("socket watcher: {e}"))?,
    );
    let filter = format!(
        "tcp and (tcp.DstPort == {CAPTURE_PORT} or tcp.SrcPort == {CAPTURE_PORT} or \
         tcp.DstPort == {forwarder_port} or tcp.SrcPort == {forwarder_port})"
    );
    let network_handle = Arc::new(
        WinDivert::network(&filter, 100, WinDivertFlags::new())
            .map_err(|e| format!("network reflector: {e}"))?,
    );

    eprintln!("[ozii-guard] handles open; unrelated traffic is fail-open");
    dev_log(&format!("[REFLECT] network filter: {filter}"));
    let ready_path = guard_ready_file();
    if let Some(parent) = ready_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &ready_path,
        format!("pid={}\nport={}\n", std::process::id(), forwarder_port),
    );

    let flows: Arc<Mutex<HashMap<FlowKey, FlowRecord>>> = Arc::new(Mutex::new(HashMap::new()));
    let stats = Arc::new(Mutex::new(GuardStats::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let discord_root = opts
        .local_app_data
        .clone()
        .or_else(|| std::env::var("LOCALAPPDATA").ok());
    if let Some(root) = discord_root.as_deref() {
        let _ =
            std::fs::remove_file(std::path::PathBuf::from(root).join("OziiDPI/guard.trace.log"));
    }

    // PID resolution is intentionally not cached: a recycled PID must never
    // inherit Discord eligibility from a process that has already exited.
    let socket_thread = {
        let socket_handle = Arc::clone(&socket_handle);
        let flows = Arc::clone(&flows);
        let stop = Arc::clone(&stop);
        let discord_root = discord_root.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match socket_handle.recv() {
                    Ok(event) => {
                        let pid = event.address.process_id();
                        let Some(key) = socket_flow_key(&event.address) else {
                            continue;
                        };
                        let exe = exe_path_of(pid).unwrap_or_default();
                        let eligible = !exe.is_empty()
                            && discord_root
                                .as_deref()
                                .is_some_and(|root| is_eligible_discord_under(&exe, root));
                        if exe.to_ascii_lowercase().contains("discord") {
                            guard_trace(
                                discord_root.as_deref(),
                                &format!(
                                    "SOCKET pid={pid} local={}:{} remote={}:{} exe={exe} eligible={eligible}",
                                    key.local_addr,
                                    key.local_port,
                                    key.remote_addr,
                                    key.remote_port
                                ),
                            );
                        }
                        dev_log(&format!(
                            "[REFLECT] SOCKET_CONNECT pid={pid} exe={exe} eligible={eligible} flow={key:?}"
                        ));
                        if !eligible {
                            continue;
                        }
                        let now = std::time::Instant::now();
                        let mut cache = flows.lock().unwrap();
                        cache.retain(|_, record| record.expires_at > now);
                        cache.insert(
                            key,
                            FlowRecord {
                                active: false,
                                expires_at: now + Duration::from_secs(30),
                            },
                        );
                    }
                    Err(WinDivertError::Recv(WinDivertRecvError::NoData)) => break,
                    Err(e) => {
                        if !stop.load(Ordering::SeqCst) {
                            dev_log(&format!("[REFLECT] socket watcher failed: {e}"));
                        }
                        break;
                    }
                }
            }
        })
    };

    // Stop watcher also shuts down blocking Recv calls.
    {
        let stop = Arc::clone(&stop);
        let network = Arc::clone(&network_handle);
        let socket = Arc::clone(&socket_handle);
        let parent_pid = opts.parent_pid;
        std::thread::spawn(move || {
            let parent = parent_pid.filter(|p| *p != 0).and_then(PidWatcher::spawn);
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let marker = guard_stop_marker_exists();
                let parent_dead = parent.as_ref().is_some_and(|p| !p.is_alive());
                if marker || parent_dead {
                    if marker {
                        let _ = std::fs::remove_file(guard_stop_file());
                    }
                    stop.store(true, Ordering::SeqCst);
                    let _ = network.shutdown_handle().shutdown();
                    let _ = socket.shutdown_handle().shutdown();
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    }

    let mut raw_buf = vec![0u8; 65536];
    while !stop.load(Ordering::SeqCst) {
        let mut packet = match network_handle.recv(&mut raw_buf) {
            Ok(packet) => packet.into_owned(),
            Err(WinDivertError::Recv(WinDivertRecvError::NoData)) => break,
            Err(e) => {
                if !stop.load(Ordering::SeqCst) {
                    eprintln!("[ozii-guard] network recv failed: {e}");
                }
                break;
            }
        };
        let original = packet.clone();
        let Some((key, ihl, tcp_flags)) = ipv4_packet_flow(&packet.data) else {
            let _ = network_handle.send(&original);
            stats.lock().unwrap().fallback_passthrough += 1;
            continue;
        };

        // External replies are included so the reflector handle mirrors the
        // official WinDivert streamdump topology. Packets not generated by
        // our local receiver remain byte-for-byte unchanged.
        if !packet.address.outbound() {
            let _ = network_handle.send(&original);
            stats.lock().unwrap().fallback_passthrough += 1;
            continue;
        }

        // Replies emitted by our raw TLS receiver are structurally unique.
        if key.local_port == forwarder_port {
            guard_trace(
                discord_root.as_deref(),
                &format!("FORWARDER_OUT flags={tcp_flags:#04x} flow={key:?}"),
            );
            reflect_forwarder_packet(&mut packet, ihl);
            if network_handle.send(&packet).is_err() {
                guard_trace(discord_root.as_deref(), "FORWARDER_SEND_FAILED");
                let _ = network_handle.send(&original);
            } else {
                stats.lock().unwrap().fragmented += 1;
            }
            continue;
        }

        if key.remote_port != CAPTURE_PORT {
            let _ = network_handle.send(&original);
            stats.lock().unwrap().fallback_passthrough += 1;
            continue;
        }

        let is_syn = tcp_flags & 0x02 != 0;
        let mut eligible = false;
        // SOCKET_CONNECT normally precedes SYN. The tiny bounded retry only
        // covers scheduler ordering between our two receiver threads.
        for attempt in 0..=10 {
            let now = std::time::Instant::now();
            {
                let mut cache = flows.lock().unwrap();
                cache.retain(|_, record| record.expires_at > now);
                if let Some(record) = cache.get_mut(&key) {
                    record.active = true;
                    record.expires_at = now + Duration::from_secs(24 * 60 * 60);
                    eligible = true;
                } else if is_syn {
                    // WinDivert's SOCKET layer exposes IPv4 endpoints through
                    // an IPv4-mapped representation, while NETWORK packets
                    // carry ordinary IPv4 header bytes. Some wrapper/driver
                    // combinations therefore disagree on the address value.
                    // The ephemeral local TCP port still belongs to the exact
                    // PID-verified socket. Use it only to bind the first SYN,
                    // then replace the provisional entry with the complete
                    // packet-derived 5-tuple for every later packet.
                    let provisional = cache
                        .iter()
                        .find(|(candidate, record)| {
                            !record.active
                                && candidate.local_port == key.local_port
                                && candidate.remote_port == key.remote_port
                                && candidate.protocol == key.protocol
                        })
                        .map(|(candidate, _)| *candidate);
                    if let Some(provisional) = provisional {
                        cache.remove(&provisional);
                        cache.insert(
                            key,
                            FlowRecord {
                                active: true,
                                expires_at: now + Duration::from_secs(24 * 60 * 60),
                            },
                        );
                        eligible = true;
                        dev_log(&format!(
                            "[REFLECT] SOCKET_TUPLE_BOUND provisional={provisional:?} flow={key:?}"
                        ));
                    }
                }
            }
            if eligible || !is_syn || attempt == 10 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        if !eligible && is_syn {
            // On some Windows/WinDivert combinations SOCKET CONNECT events
            // are missing or expose a tuple representation that cannot be
            // joined to the first packet. Ask the Windows TCP table for the
            // exact local port owner instead. This remains process-scoped:
            // games and browsers have different owning PIDs and pass through.
            for _ in 0..10 {
                if let Some(pid) = tcp4_owner_pid(key.local_port, key.remote_port) {
                    let exe = exe_path_of(pid).unwrap_or_default();
                    let pid_is_discord = !exe.is_empty()
                        && discord_root
                            .as_deref()
                            .is_some_and(|root| is_eligible_discord_under(&exe, root));
                    if exe.to_ascii_lowercase().contains("discord") {
                        guard_trace(
                            discord_root.as_deref(),
                            &format!(
                                "TCP_OWNER pid={pid} flow={key:?} exe={exe} eligible={pid_is_discord}"
                            ),
                        );
                    }
                    if pid_is_discord {
                        flows.lock().unwrap().insert(
                            key,
                            FlowRecord {
                                active: true,
                                expires_at: std::time::Instant::now()
                                    + Duration::from_secs(24 * 60 * 60),
                            },
                        );
                        eligible = true;
                        dev_log(&format!(
                            "[REFLECT] TCP_OWNER_BOUND pid={pid} exe={exe} flow={key:?}"
                        ));
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        if !eligible {
            // Critical invariant: unrelated data and address metadata are not
            // changed at all. OpenCode, browsers and games remain DIRECT.
            let _ = network_handle.send(&original);
            stats.lock().unwrap().fallback_passthrough += 1;
            if is_syn {
                dev_log(&format!("[REFLECT] FLOW_PASS_UNRELATED {key:?}"));
            }
            continue;
        }

        reflect_client_packet(&mut packet, ihl, forwarder_port);
        if network_handle.send(&packet).is_err() {
            // Fail open even if reflection/injection fails.
            let _ = network_handle.send(&original);
            stats.lock().unwrap().fallback_passthrough += 1;
            dev_log(&format!("[REFLECT] SEND_FAILED_FAIL_OPEN {key:?}"));
        } else {
            stats.lock().unwrap().captured += 1;
            if is_syn {
                guard_trace(
                    discord_root.as_deref(),
                    &format!("REFLECTED_SYN flow={key:?}"),
                );
                dev_log(&format!("[REFLECT] DISCORD_FLOW_REFLECT {key:?}"));
            }
        }
    }

    stop.store(true, Ordering::SeqCst);
    let _ = network_handle.shutdown_handle().shutdown();
    let _ = socket_handle.shutdown_handle().shutdown();
    let _ = socket_thread.join();
    let _ = std::fs::remove_file(guard_ready_file());

    let result = stats.lock().unwrap().clone();
    dev_log(&format!(
        "[REFLECT] exited reflected={} reverse={} direct={}",
        result.captured, result.fragmented, result.fallback_passthrough
    ));
    Ok(result)
}
/// Crudely infer hop count from TTL (server initial TTL in {64,128,255}).
fn infer_hops(ttl: u8) -> u8 {
    let origin = if ttl <= 64 {
        64u8
    } else if ttl <= 128 {
        128u8
    } else {
        255u8
    };
    origin.saturating_sub(ttl)
}

/// Builds a syntactically valid TLS 1.2 ClientHello announcing
/// SNI "www.microsoft.com" with conservative cipher suites. The purpose is
/// purely to satisfy mid-path DPI blacklists before the fragmented real
/// handshake follows.
fn fake_client_hello() -> Vec<u8> {
    let host = b"www.microsoft.com";

    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(&[0x03, 0x03]); // legacy version
    let mut random = [0u8; 32];
    random[0] = 0x41;
    random[1] = 0x88;
    random[2] = 0x82;
    random[3] = 0x2d;
    body.extend_from_slice(&random);
    body.push(0x00); // session id len
    body.extend_from_slice(&[0x00, 0x08]); // cipher suites len
    body.extend_from_slice(&[0x13, 0x01, 0x13, 0x03, 0xc0, 0x2f, 0x00, 0x2f]);
    body.push(0x01); // compression methods len
    body.push(0x00); // null

    let mut ext_list: Vec<u8> = Vec::new();
    // SNI: www.microsoft.com
    let mut sni = Vec::new();
    sni.push(0x00); // host_name type
    sni.extend_from_slice(&(host.len() as u16).to_be_bytes());
    sni.extend_from_slice(host);
    ext_list.extend_from_slice(&[0x00, 0x00]);
    ext_list.extend_from_slice(&(sni.len() as u16).to_be_bytes());
    ext_list.extend_from_slice(&sni);
    // supported_versions: TLS 1.2 + 1.3
    let sv = [0x03, 0x03, 0x03, 0x04];
    ext_list.extend_from_slice(&[0x00, 0x2b, 0x00, 0x04]);
    ext_list.extend_from_slice(&sv);
    // ALPN: http/1.1
    let alpn_data: &[u8] = &[
        0x08, b'h', b'2', 0x08, b'h', b't', b't', b'p', b'/', b'1', b'.', b'1',
    ];
    ext_list.extend_from_slice(&[0x00, 0x10]);
    ext_list.extend_from_slice(&(alpn_data.len() as u16).to_be_bytes());
    ext_list.extend_from_slice(alpn_data);
    // signature algorithms
    let sig = [0x04, 0x03, 0x04, 0x04, 0x05];
    ext_list.extend_from_slice(&[0x00, 0x0d, 0x00, 0x05]);
    ext_list.extend_from_slice(&sig);

    body.extend_from_slice(&(ext_list.len() as u16).to_be_bytes());
    body.extend_from_slice(&ext_list);

    let mut hs = Vec::with_capacity(body.len() + 4);
    hs.push(0x01); // ClientHello
    let blen = body.len() as u32;
    hs.extend_from_slice(&[
        ((blen >> 16) & 0xff) as u8,
        ((blen >> 8) & 0xff) as u8,
        (blen & 0xff) as u8,
    ]);
    hs.extend_from_slice(&body);

    let mut record = Vec::with_capacity(hs.len() + 5);
    record.push(0x16); // handshake record
    record.extend_from_slice(&[0x03, 0x01]);
    record.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    record.extend_from_slice(&hs);
    record
}

/// Shared stop marker path (%LOCALAPPDATA%\OziiDPI\guard.stop). Written by
/// the backend on deactivation so the guard exits even while the parent
/// process stays alive (e.g. GUI toggles).
pub fn guard_stop_file() -> std::path::PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
    std::path::PathBuf::from(base)
        .join("OziiDPI")
        .join("guard.stop")
}

pub fn guard_ready_file() -> std::path::PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
    std::path::PathBuf::from(base)
        .join("OziiDPI")
        .join("guard.ready")
}

pub fn request_guard_stop() {
    let path = guard_stop_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, b"stop\n");
    let _ = std::fs::remove_file(guard_ready_file());
}

/// Starts the reflector with elevation. Only the guard receives admin
/// rights; the proxy, engine, launcher and Discord remain normal user
/// processes. No registry, hosts, DNS, firewall or route changes are made.
pub fn spawn_elevated_reflector(forwarder_port: u16, parent_pid: u32) -> Result<u32, String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::processthreadsapi::GetProcessId;
    use winapi::um::shellapi::{SEE_MASK_NOCLOSEPROCESS, ShellExecuteExW};
    use winapi::um::winuser::SW_HIDE;

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| "current executable has no parent directory".to_string())?;
    let mut guard_path = exe_dir.join("ozii-guard.exe");
    if !guard_path.exists() {
        guard_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or_else(|| "cannot locate backend target directory".to_string())?
            .join("target")
            .join("release")
            .join("ozii-guard.exe");
    }
    if !guard_path.exists() {
        return Err(format!(
            "ozii-guard.exe not found: {}",
            guard_path.display()
        ));
    }

    // A marker from a clean previous stop must not terminate the new guard.
    let _ = std::fs::remove_file(guard_stop_file());
    let _ = std::fs::remove_file(guard_ready_file());
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map_err(|_| "LOCALAPPDATA is unavailable before elevation".to_string())?;
    let params = format!(
        "--dnat --forwarder-port {forwarder_port} --parent-pid {parent_pid} --discord-root \"{local_app_data}\""
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
    let verb: Vec<u16> = "runas".encode_utf16().chain(std::iter::once(0)).collect();

    let mut info: winapi::um::shellapi::SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<winapi::um::shellapi::SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = path_wide.as_ptr();
    info.lpParameters = params_wide.as_ptr();
    info.lpDirectory = dir_wide.as_ptr();
    info.nShow = SW_HIDE;

    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let error = unsafe { winapi::um::errhandlingapi::GetLastError() };
        return Err(format!("guard elevation was declined or failed ({error})"));
    }
    if info.hProcess.is_null() {
        return Err("guard elevation returned no process handle".to_string());
    }
    let pid = unsafe { GetProcessId(info.hProcess) };
    let _ = unsafe { winapi::um::handleapi::CloseHandle(info.hProcess) };
    if pid == 0 {
        return Err("elevated guard PID unavailable".to_string());
    }
    Ok(pid)
}

fn guard_stop_marker_exists() -> bool {
    guard_stop_file().exists()
}

/// Parent-liveness probe (Windows). `is_alive` returns false after exit.
pub struct PidWatcher {
    handle: Option<winapi::um::winnt::HANDLE>,
}

// A duplicated kernel handle is trivially movable between threads.
unsafe impl Send for PidWatcher {}

impl PidWatcher {
    pub fn spawn(pid: u32) -> Option<Self> {
        use winapi::um::processthreadsapi::OpenProcess;
        use winapi::um::winnt::SYNCHRONIZE;
        let h = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
        if h.is_null() {
            return None;
        }
        Some(Self { handle: Some(h) })
    }

    pub fn is_alive(&self) -> bool {
        use winapi::um::synchapi::WaitForSingleObject;
        match self.handle {
            Some(h) if !h.is_null() => unsafe { WaitForSingleObject(h, 0) != 0 },
            _ => false,
        }
    }
}

impl Drop for PidWatcher {
    fn drop(&mut self) {
        use winapi::um::handleapi::CloseHandle;
        if let Some(h) = self.handle.take() {
            unsafe {
                CloseHandle(h);
            }
        }
    }
}

#[cfg(test)]
mod reflector_tests {
    use super::*;

    #[test]
    fn discord_path_check_is_strict() {
        let base = r"C:\Users\test\AppData\Local";
        assert!(is_eligible_discord_under(
            r"C:\Users\test\AppData\Local\Discord\Update.exe",
            base
        ));
        assert!(is_eligible_discord_under(
            r"C:\Users\test\AppData\Local\Discord\app-1.0.999\Discord.exe",
            base
        ));
        assert!(is_eligible_discord_under(
            r"\\?\C:\Users\test\AppData\Local\Discord\APP-1.0.999\DISCORD.EXE",
            base
        ));

        assert!(!is_eligible_discord_under(
            r"C:\Program Files\Game\Update.exe",
            base
        ));
        assert!(!is_eligible_discord_under(
            r"C:\Users\test\AppData\Local\Discord\app-1.0.999\tools\Discord.exe",
            base
        ));
        assert!(!is_eligible_discord_under(
            r"C:\Users\test\AppData\Local\Discord\app-\Discord.exe",
            base
        ));
    }

    #[test]
    fn socket_port_order_is_deterministic() {
        assert_eq!(normalize_socket_ports(50_000, 443), Some((50_000, 443)));
        assert_eq!(
            normalize_socket_ports(50_000u16.swap_bytes(), 443u16.swap_bytes()),
            Some((50_000, 443))
        );
        assert_eq!(normalize_socket_ports(50_000, 80), None);
    }

    #[test]
    fn packet_parser_returns_exact_five_tuple() {
        let mut packet = vec![0u8; 40];
        packet[0] = 0x45;
        packet[9] = 6;
        packet[12..16].copy_from_slice(&[192, 168, 1, 20]);
        packet[16..20].copy_from_slice(&[162, 159, 130, 1]);
        packet[20..22].copy_from_slice(&52_144u16.to_be_bytes());
        packet[22..24].copy_from_slice(&443u16.to_be_bytes());
        packet[33] = 0x02;

        let (key, ihl, flags) = ipv4_packet_flow(&packet).expect("valid packet");
        assert_eq!(ihl, 20);
        assert_eq!(flags, 0x02);
        assert_eq!(key.local_addr, Ipv4Addr::new(192, 168, 1, 20));
        assert_eq!(key.local_port, 52_144);
        assert_eq!(key.remote_addr, Ipv4Addr::new(162, 159, 130, 1));
        assert_eq!(key.remote_port, 443);
        assert_eq!(key.protocol, 6);
    }
}
