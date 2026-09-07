# Frontend Error Codes Matrix (`OziiError`)

The `OziiError` type prevents the frontend from relying on brittle string parsing. Error handling maps deterministically to states.

| Error Code | Occurs When | Recovery Strategy / Frontend Handling |
|------------|-------------|---------------------------------------|
| `ExistingPacConfigured` | A competing application (e.g. enterprise software) has locked the AutoConfigURL to a different script | Refuse to start, prompt the user to manually disable the other application. |
| `WpadConflict` | "Automatically Detect Settings" is enabled in Windows Settings and `OZIIDPI_IGNORE_WPAD` is not active. | Warn the user that WPAD breaks custom PAC routing. Provide a UI option to apply the ignore flag or prompt them to disable it manually in Windows. |
| `EngineBinaryMissing` | `ozii-dpi-engine.exe` is absent from the expected `engine/bin` path. | Indicates a corrupted installation. Prompt re-install. |
| `EngineBuildMismatch` | The engine version doesn't match the required core capability. | Corrupted installation. |
| `EngineStartupFailed` | SpoofDPI failed to initialize (e.g. port already locked, antivirus block). | Show error and attempt a safe cleanup. |
| `EngineExited` | The engine crashed silently during an active session (caught by watchdog loop). | Automatically triggers fail-open. The UI should notify the user that protection was disabled safely to preserve their internet connection. |
| `PacServerFailed` | Failed to spawn the internal rust thread for PAC hosting. | Internal system issue. |
| `PortAllocationFailed` | 0.0.0.0 is unable to allocate random ephemeral ports. | Very rare, implies a heavily starved TCP stack. |
| `ProxyReadFailed` / `ProxyWriteFailed` | Cannot modify or evaluate Windows registry configurations. | Provide instructions on running with the necessary Windows profile permissions (though Admin is typically not strictly required for HKCU). |
| `ProxyRestoreFailed` | Attempting to shut down failed because we could not remove the AutoConfigURL correctly. | Display a severe warning to the user; they may need to disable the proxy manually via `inetcpl.cpl`. |
| `RecoveryRequired` | The application was forcefully closed while active in a previous run. A dirty session lock exists. | Prompt the user to execute "Recovery Mode" to scrub their registry of dead proxy bindings. |
| `RecoveryFailed` | The system was unable to scrub the broken proxy from the registry. | Terminal failure. Requires manual user intervention in `inetcpl.cpl`. |
| `AlreadyRunning` / `NotRunning` | Lifecycle violations caused by race conditions on UI clicks. | Blocked by state machine, no action needed. |
