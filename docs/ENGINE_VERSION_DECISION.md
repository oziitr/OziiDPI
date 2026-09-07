# SpoofDPI Engine Version Decision

## Current Pinned Version
**SpoofDPI v1.2.1** (Commit: `de9aab76cbb601d3c21077c776e21162d72d2b98`)

## Rationale
During the Backend Truth Audit, we evaluated whether to upgrade from v1.2.1 to the latest upstream release (e.g., v1.5.x).

We decided to **stay pinned to v1.2.1** for the following critical reasons:

1. **Windows Compatibility and Stability**: v1.2.1 contains necessary patches and stable implementations for `internal/netutil/conn.go` that handle Windows dial behavior properly. Newer versions introduced passive gateway and interface discovery (e.g. `NetworkDetector` sniffing outbound packets, requiring different adapter bindings) which are currently unverified for our lightweight loopback proxy architecture.
2. **Fragmentation Strategy**: v1.2.1 provides the exact CLI arguments we rely on, primarily `--https-split-mode chunk` and `--https-chunk-size 1`. This works flawlessly for DPI evasion without needing complex packet capture drivers.
3. **No WinDivert/Npcap Dependency Required**: Keeping this specific proxy-based fragmentation means we do not need to require users to install Npcap/WinDivert, unlike newer or different DPI evasion techniques that rely on raw packet injection on Windows.

## Patch Management
If any patches are required to the v1.2.1 source tree (e.g. for Go module updates or compilation fixes), they must be stored in `engine/patches/`. 
The `build.ps1` script explicitly verifies the commit hash `de9aab76cbb601d3c21077c776e21162d72d2b98` before compiling. If the underlying source is modified, the build will fail, preventing accidental integration of unverified code.
