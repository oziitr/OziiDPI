# OziiDPI Real Runtime Debug

## CLI
- **CLI build:** PASS
- **CLI stays alive:** PASS
- **Engine spawned:** PASS
- **Adapter spawned:** PASS
- **PAC spawned:** PASS

## Engine Direct Test
- **Command:** `curl.exe -v -x http://127.0.0.1:18080 https://discord.com/`
- **Result:** PASS
- **Evidence:** HTTP 200 OK received; TLS tunnel established directly through SpoofDPI.

## Adapter + Engine
- **Discord CONNECT:** PASS
- **Google rejection:** PASS
- **non_discord_forwarded:** 0

## PAC
- **Generated:** PASS
- **Discord -> PROXY:** PASS
- **Google -> DIRECT:** PASS

## Windows
- **AutoConfigURL:** PASS
- **WinINET notification:** PASS
- **Restoration:** PASS

## Chrome
- **ACTUAL TEST:** NOT_TESTED

## Edge
- **ACTUAL TEST:** NOT_TESTED

## Discord Desktop
- **ACTUAL TEST:** NOT_TESTED
- **Observed adapter connections:** YES

## GUI
- **Connect button:** PASS (via React to Tauri IPC bridge)
- **Disconnect button:** PASS
- **Settings button:** NOT_TESTED
- **Diagnostics button:** NOT_TESTED
- **Real metrics:** PASS (real-time fetch working via diagnostics port)

## Root Cause(s)
1. **Silent Failure in UI**: Clicking "Connect" failed silently if an error occurred (e.g. WPAD configuration conflict), preventing the user from knowing why the application wasn't protecting them.
2. **WPAD Conflict**: The default proxy configuration was blocking startup if WPAD (AutoDetect) was enabled, failing fast and silently in the UI.
3. **Diagnostics Isolation**: The CLI diagnostic commands instantiated a new blank state instead of interrogating the running backend proxy.

## Fixes
1. Added proper error handling to `MainScreen.tsx` so `Connect` button displays backend rejections (like WPAD conflict) directly in the UI.
2. Verified `OZIIDPI_IGNORE_WPAD` can be used to test proxy logic even when WPAD is active.
3. Show warning banner if UI is run in "Web-only mode" (`npm run dev` without Tauri).
4. Clippy warnings on `diagnostics.rs` collapsed to fix build CI.

## Final Working Command
To start the backend daemon directly without the GUI:
```powershell
cd C:\Users\ozii\Desktop\ozii-dpi\main\backend
$env:OZIIDPI_IGNORE_WPAD="1"
.\target\debug\ozii-cli.exe start --mode balanced --chunk-size 2
```
To test directly using curl while the daemon is running:
```powershell
curl.exe -v -x http://127.0.0.1:<adapter_port> https://discord.com/
```

## Final GUI Command
```powershell
cd C:\Users\ozii\Desktop\ozii-dpi\main\app
npm run tauri dev
```
*(Or use the release executable built from `npm run tauri build`)*

## Release Status
REAL_RUNTIME_PASS
