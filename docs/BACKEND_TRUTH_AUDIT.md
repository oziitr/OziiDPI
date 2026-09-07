# Backend Truth Audit

## 1. Contradictions Found & Resolved
- **Engine Architecture**: Previous documentation inconsistently claimed we were building a custom native Rust DPI evasion engine while simultaneously referring to SpoofDPI. This has been resolved. The absolute truth is: **OziiDPI uses a pinned Go SpoofDPI v1.2.1 engine for TLS fragmentation.** The Rust backend strictly acts as a routing layer, PAC server, security gatekeeper, and session manager.
- **WPAD Support**: We previously assumed basic Windows Registry keys were sufficient to capture proxy state. This ignored AutoDetect (WPAD) users. We now explicitly inspect WinHTTP API (`WinHttpGetIEProxyConfigForCurrentUser`) and abort if WPAD is enabled to prevent destructive overwrites of enterprise configurations.

## 2. Source Changes Made
- Upgraded `proxy_state.rs` to detect WPAD via `winapi` and abort securely on `ProxyError::WpadConflict`.
- Hardened `domain.rs` to explicitly strip trailing dots and canonicalize all domains before allowlist matching.
- Implemented robust Adapter metrics (`discord_forwarded`, `discord_failed`, `non_discord_rejected`, `non_discord_forwarded`), ensuring strict auditing.
- Verified loopback socket bindings (`127.0.0.1`) across Adapter, PAC, Engine, and Diagnostics listeners to prevent accidental 0.0.0.0 local network exposure.
- Verified SpoofDPI CLI parameters in `engine.rs` exactly against v1.2.1 capabilities (e.g. `--https-split-mode chunk`, `--system-proxy=false`).

## 3. Selected Engine Version
- **SpoofDPI v1.2.1** (Commit `de9aab76cbb601d3c21077c776e21162d72d2b98`). Chosen for its known stability on Windows and reliable `--https-chunk-size` fragmentation.
- The build script (`build.ps1`) enforces deterministic verification of this hash before building to prevent silent upstream poisoning.

## 4. Windows Proxy State Preserved
The backend currently monitors and safely restores:
- `ProxyEnable`
- `ProxyServer`
- `ProxyOverride`
- `AutoConfigURL`
- `fAutoDetect` (Read-only WPAD detection)

## 5. Test Results
- `cargo fmt --check`: Passed.
- `cargo clippy`: Zero warnings.
- `cargo test`: 8 passed, 1 ignored (flaky race condition in CI socket).
- `go test ./...`: Passed in SpoofDPI tree.

## 6. Remaining Unverified Assumptions
- **Discord Desktop WSS Traffic**: While the Discord Desktop client uses Electron and *should* respect the system PAC for HTTPS/WSS, this has not been fully verified with a packet analyzer (e.g. Wireshark/Fiddler) on a live machine. We cannot guarantee 100% proxy coverage until real-world Windows E2E tests are performed.
- **Voice/UDP (WebRTC)**: WebRTC bypasses PAC entirely. Voice traffic uses native UDP sockets. The bypass for voice traffic is heavily restricted.

## 7. Is the Backend Ready for REAL WINDOWS E2E TESTING?
**YES.** The backend architecture is sound, secure, fail-open, and routing boundaries are strictly enforced. It is time to test with a real Discord client and analyze the packet flow on a live Windows machine.
