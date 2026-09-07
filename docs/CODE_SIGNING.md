# OziiDPI Code Signing Status

## Current Status: UNSIGNED

All binaries and installers produced by the current build pipeline are **unsigned**.

## Implications

- **Windows SmartScreen**: Will display "Windows protected your PC" warning on first run.
- **Windows Defender**: May flag the executable due to lack of reputation (not a virus detection).
- **Enterprise environments**: Group policy may block unsigned executables.

## Files Requiring Signing for Public Distribution

| Artifact | Type | Priority |
|----------|------|----------|
| `OziiDPI.exe` | Main Application | HIGH |
| `ozii-dpi-engine.exe` | SpoofDPI Engine | HIGH |
| `OziiDPI_0.1.0_x64_en-US.msi` | MSI Installer | HIGH |
| `OziiDPI_0.1.0_x64-setup.exe` | NSIS Installer | HIGH |

## Signing Requirements

1. Obtain an **Extended Validation (EV) Code Signing Certificate** from a trusted CA (DigiCert, Sectigo, etc.).
2. Sign all executables using `signtool.exe` from the Windows SDK.
3. Sign MSI using the same certificate.
4. NSIS installer should be signed after generation.

## Tauri Signing Configuration

Tauri supports signing through `tauri.conf.json` with the `bundle.windows.signCommand` option or environment variables:

```json
{
  "bundle": {
    "windows": {
      "signCommand": "signtool sign /sha1 THUMBPRINT /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 \"%1\""
    }
  }
}
```

## Anti-Virus Considerations

DPI bypass tools and proxy executables may trigger heuristic AV detections. This is expected.

**Recommendations:**
- Build reproducibly from public source code.
- Submit binaries to VirusTotal after each release.
- Provide SHA-256 hashes for verification.
- Code signing with EV certificate significantly reduces false positives.
- Do NOT obfuscate or pack executables to avoid detection.
