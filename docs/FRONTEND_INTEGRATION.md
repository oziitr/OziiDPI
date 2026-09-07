# Frontend Integration Overview

The `OziiDPI` project utilizes an extremely strict thin-client architecture. The frontend UI (Tauri, React, etc.) acts exclusively as a dumb visualization layer. It holds zero business logic and makes zero systemic decisions. 

## Architectural Strictures
1. **The UI Never Guesses State**: It must always pull its state truth from `OziiService::status()` or rely on `BackendEvent::StateChanged` emitted by `ozii-core`.
2. **The UI Never Kills Processes**: It must call `stop()` or `recover()` which safely handles graceful teardown and registry restoration. Sending raw `kill` signals via Tauri is completely banned.
3. **The UI Never Parses Paths**: Engine binary paths, local AppData paths, and recovery JSON paths are abstracted within `ozii-core`.
4. **No Naked IPC Strings**: All errors and statuses passed over the Tauri bridge are strictly serialized via `serde` DTOs (e.g., `DiagnosticReport`, `OziiError`). 

## Recommended Tauri Setup 
1. Build out `crates/ozii-tauri` using `create-tauri-app` or similar.
2. Embed the `OziiService` into the Tauri application managed state (`app.manage(OziiService::new())`).
3. During setup, poll `service.get_capabilities().recovery_required`. If true, block the UI on a "Recovery Needed" splash screen.
4. Pass `DiagnosticReport` objects directly into React component properties to cleanly render health and routing metrics.
