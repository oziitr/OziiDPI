//! ozii-guard: elevated transparent guard for OziiDPI.
//!
//! Runs either the ClientHello fragmentation guard or the SYN-DNAT
//! transparent proxy (Discord app path). Must be launched elevated so the
//! WinDivert driver can be touched. Self-exits when the parent dies.

use std::process::exit;

use clap::Parser;
use ozii_core::engine::DpiMode;
use ozii_core::pktguard::{GuardOptions, run_dnat_proxy};

#[derive(Parser, Debug)]
#[command(name = "ozii-guard", about = "OziiDPI packet guard (elevated)")]
struct Args {
    /// Mode: turbo|balanced|strong|strong_advanced
    #[arg(long, default_value = "turbo")]
    mode: String,

    /// Balanced chunk size; strong_advanced fake count.
    #[arg(long, default_value = "2")]
    chunk_size: u8,

    /// Strong_advanced fake ClientHello count.
    #[arg(long, default_value = "1")]
    fake_count: u8,

    /// Enable transparent SYN-DNAT proxy into the local adapter (Discord app path).
    #[arg(long, default_value_t = false)]
    dnat: bool,

    /// OziiDPI adapter port for DNAT mode.
    #[arg(long, default_value_t = 39572)]
    adapter_port: u16,

    /// Raw TLS receiver port used by process-aware reflection.
    #[arg(long, default_value_t = 39575)]
    forwarder_port: u16,

    /// OziiDPI engine port for DNAT mode (engine connections pass through).
    #[arg(long, default_value_t = 39571)]
    engine_port: u16,

    /// Comma-separated Discord domains to capture.
    #[arg(long, default_value = "discord.com")]
    domains: String,

    /// Parent PID to watch; exit when the parent terminates.
    #[arg(long)]
    parent_pid: Option<u32>,

    /// Normal user's LocalAppData path, passed explicitly before UAC elevation.
    #[arg(long)]
    discord_root: Option<String>,
}

fn parse_mode(s: &str, chunk: u8, fake: u8) -> Result<DpiMode, String> {
    match s {
        "turbo" => Ok(DpiMode::Turbo),
        "balanced" => Ok(DpiMode::Balanced { chunk_size: chunk }),
        "strong" => Ok(DpiMode::Strong),
        "strong_advanced" => Ok(DpiMode::StrongAdvanced { fake_count: fake }),
        other => Err(format!("unknown mode: {other}")),
    }
}

fn main() {
    let args = Args::parse();
    let mode = match parse_mode(&args.mode, args.chunk_size, args.fake_count) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ozii-guard: {e}");
            exit(2);
        }
    };
    let domains: Vec<String> = args
        .domains
        .split(',')
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .collect();

    let opts = GuardOptions {
        domains,
        mode,
        parent_pid: args.parent_pid.filter(|p| *p != 0),
        local_app_data: args.discord_root,
    };

    if !args.dnat {
        eprintln!(
            "ozii-guard: legacy domain/IP capture is disabled; use the process-aware --dnat mode"
        );
        exit(2);
    }

    let _ = args.adapter_port; // accepted for compatibility with older launch scripts
    match run_dnat_proxy(&opts, args.forwarder_port, args.engine_port) {
        Ok(stats) => {
            eprintln!(
                "[ozii-guard] reflector exited captured={} relayed={} fallback={}",
                stats.captured, stats.fragmented, stats.fallback_passthrough
            );
            exit(0);
        }
        Err(e) => {
            eprintln!("[ozii-guard] reflector fatal: {e}");
            exit(1);
        }
    }
}
