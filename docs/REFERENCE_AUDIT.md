# BypaxDPI Reference Architecture Audit

## 1. Existing Bypax Architecture
The existing reference implementation, BypaxDPI-Windows-main, is a Tauri-based desktop application. It acts as a wrapper around [SpoofDPI](https://github.com/xvzc/SpoofDPI) which is built and included as a sidecar binary (`bypax-proxy.exe`). 
- **Frontend**: React (Vite) application managing configuration, sidecar state, and logs.
- **Backend (Rust)**: Tauri commands handle Windows registry modifications (ProxyServer/ProxyEnable/ProxyOverride), PAC file generation/server, loopback exemption for UWP apps via `CheckNetIsolation.exe`, WinHTTP modifications via `netsh`, firewall rules via `netsh`, and crash handling/recovery via sentinel file and panic hook.
- **Network Engine**: A Go-based SpoofDPI executable processes the actual traffic to evade Deep Packet Inspection (DPI).

## 2. Which parts are reusable
- The concept of generating a PAC file on an ephemeral port.
- The concept of binding to `127.0.0.1` and avoiding `0.0.0.0` unless explicitly intended.
- The `get_safe_lan_ip` logic for finding non-virtual local adapters (useful if we ever add LAN sharing).

## 3. Which parts are unsafe or badly designed
- **Global Proxy Mutations**: Mutates the user's global WinHTTP system proxy by default.
- **UWP Exemptions**: Indiscriminately runs a PowerShell script to add loopback exemption to *all* UWP packages.
- **State Management**: Original proxy settings are backed up to memory (`OriginalProxySettings` in a `OnceLock`). If the application crashes, the memory is lost, and the global proxy state is cleared instead of restored, leaving the user with potentially broken enterprise proxy configurations.
- **Port Allocation & Reusability**: Hardcoded sidecar ports and binding configurations.
- **Bloat**: Large frontend logic for spawning processes that should live entirely in a backend engine module.

## 4. Which parts we will intentionally NOT copy
- **SpoofDPI as a Sidecar**: We will build our own routing logic.
- **Global WinHTTP and UWP changes**: We will NOT apply any global network configuration changes to the OS that affect traffic outside of the intended scope.
- **Frontend Process Management**: The frontend will not spawn sidecar processes. The Rust backend will manage the proxy lifecycle.

## 5. How selective Discord-only routing will work
Instead of a global proxy, the backend will host a proxy server that specifically routes only Discord domain requests through the DPI engine.
For browsers, we will generate a PAC file that matches Discord domains and routes them to the local proxy (`PROXY 127.0.0.1:port`), while all other traffic is returned as `DIRECT`.
For the Discord Desktop app (Electron), we can pass proxy arguments upon launch or rely on the PAC file if configured at the system level.

## 6. Risks with Discord desktop/Electron proxy behavior
Electron apps (like Discord desktop) may ignore system PAC files depending on their chromium command-line switches or how they resolve proxies. There is a risk that Discord may cache proxy settings or require explicit `--proxy-server` launch arguments if the PAC routing is not respected.

## 7. Risks with Windows PAC caching
Windows (and browsers like Chrome/Edge) caches PAC file responses. When the PAC server is started or stopped, browsers might not immediately fetch the new rules. BypaxDPI mitigates this by broadcasting `INTERNET_OPTION_SETTINGS_CHANGED`, which we must implement carefully to ensure the cache is purged without disrupting active connections.

## 8. Existing-proxy compatibility
Since Bypax stores the previous proxy in-memory, crashes cause data loss. We must persist the existing proxy settings to disk (or the Registry) before modification so that on crash, our recovery mechanism can correctly restore the user's prior configuration.

## 9. Crash recovery architecture
We will use a robust, file-backed state machine. Before altering any Windows settings, the exact state of the proxy will be saved to a `state.json` file. A recovery command (e.g., `ozii-dpi recover`) will read this file and revert the registry. If the program crashes, the next execution will detect an unclean shutdown and automatically invoke the recovery flow.

## 10. Sidecar source/reproducibility status
The reference project includes a build script (`tum_kod.txt` logs) that compiles SpoofDPI 1.2.1 into `bypax-proxy.exe`. However, relying on an external precompiled binary or 2. **Opaque Upstream Binary**: The project relied on an opaque compiled `SpoofDPI` binary. OziiDPI solves this by pinning to SpoofDPI v1.2.1 from source and applying deterministic builds, using a Rust routing layer and adapter to encapsulate the Go engine.

## Known Problems Verified
- **A & B**: Verified. `original_proxy_store()` is in-memory. Panic hook calls `reg add` with `ProxyEnable 0`, wiping previous settings.
- **C**: Verified. Sidecar port bind uses `0.0.0.0` in `get_sidecar_config` for game mode/LAN sharing.
- **D**: Verified. `set_system_proxy` calls `netsh winhttp set proxy`.
- **E**: Verified. `exempt_all_uwp_apps` adds exemption to all UWP packages via PowerShell.
- **F**: Verified. Strong mode uses `chunk-size 1` and `fake-count 3` (no `disorder` argument is passed to SpoofDPI).
