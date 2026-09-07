# Public Backend API (`OziiService`)

The `OziiService` struct is the single authoritative state machine controlling the lifecycle of the DPI proxy system. It guards against race conditions (like mashing the "Start" button) and abstracts away the complexities of interacting with the Windows subsystem.

## Core Methods

- `pub fn new() -> Self`: Initializes the facade in a `Disconnected` state, setting up configuration and session management automatically.
- `pub fn get_events(&self) -> Arc<EventBroadcaster>`: Exposes an `mpsc` Receiver/Sender pair that broadcasts state changes and engine metrics.
- `pub fn start(&self, request: StartRequest) -> Result<(), OziiError>`: Validates that no proxy is currently active (checks memory state and disk session recovery files), spawns the `SpoofDpiEngine`, Rust `AdapterProxy`, and `PacServer`, and modifies the Windows auto-proxy configuration url.
- `pub fn stop(&self) -> Result<(), OziiError>`: Safely dismantles the engine and restores the Windows proxy settings precisely to their original states using the stored `SessionState`.
- `pub fn status(&self) -> BackendStatus`: Returns the enum representing the authoritative state of the backend lifecycle.
- `pub fn diagnose(&self) -> Result<DiagnosticReport, OziiError>`: Retrieves a safely typed bundle of diagnostics including routing metrics and active open ports.
- `pub fn recover(&self) -> Result<bool, OziiError>`: Enacts fail-open behavior. Reverts the system back to normal state utilizing the disk backup file if the application suffered a forceful termination (e.g., SIGKILL, sudden exit without `stop()` being called).
- `pub fn get_capabilities(&self) -> Capabilities`: Checks the system context for capabilities and requirements (e.g., if a prior crashed session exists, `recovery_required` is true).
- `pub fn get_config(&self) -> AppConfig`: Reads from the configuration JSON file in local AppData.
- `pub fn update_config(&self, config: AppConfig) -> Result<(), OziiError>`: Updates and persists configuration data cleanly.
