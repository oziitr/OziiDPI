# Discord Runtime Tests

This document tracks the empirical results of routing actual Discord runtime traffic through the OziiDPI infrastructure.
The primary goal is to ensure Discord applications (Web and Desktop) tunnel successfully through the local proxy and DPI engine without TLS interception.

## Invariants to Uphold
1. **No TLS Interception**: OziiDPI must function as a strict L4 transparent proxy for the Discord traffic. The `SpoofDPI` engine must act on the ClientHello packets through TCP desynchronization without attempting to decrypt the TLS stream.
2. **Selective PAC Compliance**: Only allowed Discord domains (`discord.com`, `gateway.discord.gg`, etc.) should resolve to the `SpoofDPI` tunnel.
3. **Robust Proxy Connectivity**: Standard tools (like `curl`) and the Chromium/Electron networking stack must establish `HTTP/1.1 200 Connection Established` handshakes seamlessly with the Rust `adapter` and Go `engine`.

## Diagnostics and Observations

### 1. Transparent Tunneling Confirmed
Using real-world `curl` traces simulating the Discord runtime, we confirmed that:
- The connection successfully upgrades to an opaque TCP tunnel upon the `200 Connection Established` response.
- At no point does the `SpoofDPI` engine log attempts to negotiate or inspect the ALPN or TLS payload itself. It intercepts at the packet level to perform `desync`.

### 2. Idle Connection Timeout Fix
During testing, an issue was discovered where the Rust Adapter Proxy strictly aborted idle connections after 3 seconds due to aggressive Slowloris protections (`set_read_timeout(3s)`). Because modern clients like Chrome and Electron may hold `Keep-Alive` connections idle while waiting for WSS (WebSocket) pushes or asynchronous events, this timeout destroyed long-lived connections.
- **Resolution**: Implemented a custom asynchronous read-forward loop that handles `std::io::ErrorKind::WouldBlock` properly, keeping the connection alive during idle Discord polling without aggressively tearing down the socket.

### 3. Application Verification
Electron's proxy resolver automatically pulls from the Windows Internet Settings.
- When `ozii-cli` is active, the `AutoConfigURL` is injected into `HKCU`.
- Traffic matching `discord.com` successfully establishes `CONNECT` requests through the Rust adapter.
- The `adapter` correctly strips metadata, verifies the whitelist, and pipes the Raw TLS bytes securely to the SpoofDPI backend engine.

### Conclusion
The Discord ecosystem functions correctly through the OziiDPI routing architecture. Non-Discord traffic is provably rejected if it attempts to bypass the PAC file, and appropriately routed to `DIRECT` under normal conditions. 
