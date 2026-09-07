# Configuration Management

The `ozii-core` crate handles all persistence, validation, and serialization of user preferences. The frontend must not write directly to configuration files, as this breaks validation schemas.

## Storage
- **Path**: `%LOCALAPPDATA%\OziiDPI\config.json`
- **Format**: JSON, governed by `AppConfig` struct serialization.

## Configurable Properties
- `mode` (String): Controls the active DPI bypass algorithm (Turbo, Balanced, Strong, Strong-Advanced).
- `chunk_size` (u8): Controls chunk fragmentation bounds for specific modes.
- `fake_count` (u8): Advanced option determining packet spoofing density.
- `ignore_wpad` (bool): If set to `true`, `ozii-core` will bypass the `WpadConflict` check and force the PAC overwrite.

## Architecture
The `ConfigManager` inside the `OziiService` initializes fallback defaults if the configuration doesn't exist. 
The UI calls `update_config()` with a fully populated `AppConfig` object to save changes, maintaining strict validation.
