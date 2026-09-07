# OziiDPI v0.1.0 Release Notes

## Overview

OziiDPI v0.1.0 is the first release of a Discord-specific DPI bypass application for Windows. It selectively routes only Discord traffic through a local TLS fragmentation engine, leaving all other internet traffic completely untouched.

## Features

### Discord-Only Routing
- Automatic PAC-based selective routing for Discord domains
- Covers: `discord.com`, `discord.gg`, `discordapp.com`, `discordapp.net` and all subdomains
- Non-Discord traffic is never proxied (security invariant: `non_discord_forwarded == 0`)

### DPI Bypass Modes
- **Turbo**: SNI-based split — lowest overhead
- **Balanced**: Configurable chunk-based split — recommended default
- **Strong**: 1-byte chunk split — maximum fragmentation
- **StrongAdvanced**: Fake packet injection (requires Npcap)

### Crash Recovery
- Automatic Windows proxy state backup before activation
- Dirty session detection on startup
- One-click recovery if previous session did not close cleanly
- Corrupt session quarantine (no panic on malformed state files)

### Security
- All listeners bind to `127.0.0.1` only — no LAN exposure
- Adapter proxy rejects private IP addresses (SSRF protection)
- No system DNS modifications
- No WinHTTP modifications
- No blanket UWP loopback exemptions
- No telemetry or data collection

### Desktop Application
- Modern dark-themed Tauri + React desktop application
- Real-time routing metrics display
- Safe shutdown on window close (proxy settings always restored)

## Browser Support

| Browser | Status |
|---------|--------|
| Chrome | ✅ Honors Windows system PAC |
| Edge | ✅ Honors Windows system PAC |
| Firefox | ⚠️ Requires manual "Use system proxy" setting |

## Discord Desktop Support

| Client | Status |
|--------|--------|
| Discord Stable | ✅ Honors Windows system PAC (Chromium/Electron) |
| Discord Canary | ✅ Expected compatible (same Electron base) |
| Discord PTB | ✅ Expected compatible (same Electron base) |

## Known Limitations

- **Voice traffic**: UDP-based Discord voice is not routed through the DPI bypass. Voice uses direct UDP connections which are outside the TCP proxy architecture.
- **Firefox**: Does not automatically honor Windows PAC; requires manual configuration.
- **Code signing**: Binaries are currently unsigned. Windows SmartScreen warnings are expected.
- **Single instance**: No single-instance enforcement yet.

## Installation

See [USER_INSTALL_GUIDE.md](USER_INSTALL_GUIDE.md) for detailed instructions.

## Troubleshooting

See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for common issues and solutions.
