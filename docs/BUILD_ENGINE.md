# Engine Compilation Guide

The DPI engine (`ozii-dpi-engine.exe`) is compiled directly from the upstream [xvzc/SpoofDPI](https://github.com/xvzc/SpoofDPI) repository.

## Reproducible Build

Bypax utilized a hardcoded `SpoofDPI-1.2.1` zip source. In OziiDPI, we maintain a cloned repository pinned to the `v1.2.1` tag inside the `main/engine/SpoofDPI/` folder.

To build the executable deterministically:
1. Ensure you have `go` (version 1.20+) installed on your machine.
2. Open PowerShell in `main/engine/`.
3. Run `.\build.ps1`.

### Build Script Actions
The script compiles the `cmd/spoofdpi` entrypoint into the `main/engine/bin/ozii-dpi-engine.exe` executable with stripped debug symbols and no filesystem paths (using `-trimpath -ldflags="-s -w"`), resulting in a heavily optimized and minimal binary.

### Windows Go Compatibility
There was a minor compilation issue for Go >= 1.20 when building SpoofDPI v1.2.1 on Windows. We specifically patched `internal/netutil/conn.go:108` to cast `fd` to `syscall.Handle(fd)` instead of `int(fd)` to ensure it compiles out of the box.
