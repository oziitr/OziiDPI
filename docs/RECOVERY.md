# Crash Recovery & State Management

A core design principle of OziiDPI is to **never permanently alter the user's OS network settings without a guaranteed restoration path**.

## Persistent State File
Every time OziiDPI activates its DPI proxy, it saves the **exact original Windows Internet Settings** to disk using atomic writes (temporary file + rename).
- **Location**: `%LOCALAPPDATA%\OziiDPI\state\session.json`

## Fail-Safe Transaction Protocol
1. **Start**: The exact registry state (`ProxyEnable`, `ProxyServer`, `ProxyOverride`, `AutoConfigURL`) is read. If an `AutoConfigURL` already exists, startup is **aborted** with `ExistingPacConfigured` to protect corporate networks. The backup is committed to disk *before* any registry modifications occur.
2. **Stop**: The backup is read from disk, the registry is meticulously restored, WinINET is notified, the engine processes are killed safely, and the session state is marked clean.
3. **Recovery**: On startup, if a `session.json` file is found indicating an unclean shutdown, a recovery routine is instantly executed. It restores the registry from the file and checks if the zombie engine process is still running.

## PID-Reuse Safety
A common flaw in recovery logic is terminating a process solely by its PID, which Windows might have reassigned to an unrelated program (e.g., svchost). OziiDPI uses the `ProcessIdentity` system to record the engine's PID **and** start timestamp. When tearing down, the PID's start timestamp and executable path are strictly validated before issuing a termination signal.
