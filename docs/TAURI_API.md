# Tauri Frontend API integration

The `ozii-core` crate exposes the `OziiService` facade, which acts as the definitive boundary for Tauri applications. All business logic, state machines, and Windows registry mutations are safely encapsulated here. 

To bridge `ozii-core` into the Tauri frontend:
1. Initialize the `OziiService` using `OziiService::new()` and store it as a Tauri managed state (`tauri::State<OziiService>`).
2. Map `OziiService` commands directly to `#[tauri::command]` functions.
3. Spawns an event loop that listens to `service.get_events().try_recv()` and maps `BackendEvent` to Tauri `app_handle.emit_all()` calls.

### Sample Tauri Commands implementation
```rust
#[tauri::command]
fn start_engine(service: State<OziiService>, req: StartRequest) -> Result<(), OziiError> {
    service.start(req)
}

#[tauri::command]
fn stop_engine(service: State<OziiService>) -> Result<(), OziiError> {
    service.stop()
}

#[tauri::command]
fn get_diagnostics(service: State<OziiService>) -> Result<DiagnosticReport, OziiError> {
    service.diagnose()
}
```
