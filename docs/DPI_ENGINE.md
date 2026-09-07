# DPI Engine Documentation

OziiDPI utilizes [SpoofDPI](https://github.com/xvzc/SpoofDPI) under the hood for application-layer TLS fragmentation and manipulation.

## Modes

- **Turbo**: Splits the packet exactly at the SNI boundary (`--https-split-mode sni`). This is the fastest method and works for many basic DPI systems.
- **Balanced**: Splits packets into chunks of a configured size (`--https-split-mode chunk --https-chunk-size 2`). Slower, but effectively evades DPIs that reconstruct smaller streams.
- **Strong**: Forces chunks of size 1 byte (`--https-split-mode chunk --https-chunk-size 1`). Highly evasive but requires more packet overhead.
- **StrongAdvanced**: Forces chunk size 1 AND injects fake Client Hello packets (`--https-fake-count X`). **Note:** This is the *only* mode that relies on Npcap/Winpcap packet injection.

## Engine Lifecycle (Watchdog)

The OziiDPI backend wraps the engine process in a robust lifecycle model:
1. It attempts to find an available dynamic port.
2. It launches `ozii-dpi-engine.exe` strictly on `127.0.0.1` and passes `--system-proxy=false` to prevent internal system registry tampering.
3. Once running, a watchdog thread checks the health of the engine.
4. If the engine dies unexpectedly (e.g. killed via Task Manager), the watchdog intercepts the exit, restores Windows proxy settings to standard, and exits gracefully (Fail-Open behavior).
