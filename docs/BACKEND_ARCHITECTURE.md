# Backend Architecture

The OziiDPI backend is built on a fail-safe, session-based architecture ensuring the Windows OS global proxy settings are protected.

## Core Modules

- **config (`ozii-core/src/config.rs`)**: Manages port configurations, bind addresses, and the central domain allowlist.
- **domain (`ozii-core/src/domain.rs`)**: Implements the `DomainAllowlist` struct to robustly match domains with exact and wildcard suffixes (e.g. `discord.com` and `*.discord.com`).
- **pac (`ozii-core/src/pac.rs`)**: Dynamically generates a PAC (Proxy Auto-Configuration) script based on the current domain allowlist, routing only target traffic to our local proxy while allowing the rest to flow `DIRECT` (or via an existing manual proxy).
- **proxy_state (`ozii-core/src/proxy_state.rs`)**: A testing-friendly abstraction over the Windows Registry (`HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`). Captures and restores `ProxyEnable`, `ProxyServer`, `ProxyOverride`, and `AutoConfigURL`.
- **session (`ozii-core/src/session.rs`)**: Provides atomic, file-backed state management (`%LOCALAPPDATA%\OziiDPI\state\session.json`). Manages the exact lifecycle (start/stop) to guarantee the previous user settings are always restorable.
- **process (`ozii-core/src/process.rs`)**: Leverages `sysinfo` to safely validate PID and process executable identity before attempting termination. Prevents dangerous PID-reuse race conditions.

## Selective-Routing Strategy

Instead of globally routing the entire machine, we host a local PAC server (e.g., `127.0.0.1:8787`). The PAC script instructs the OS to proxy *only* Discord traffic through our DPI engine (`127.0.0.1:8080`), leaving Steam, YouTube, etc. entirely unmodified. This removes the need for unsafe global WinHTTP modification or PowerShell UWP loopback exemptions.
