# Windows End-to-End Tests Matrix

This document outlines the end-to-end verification of the OziiDPI Windows proxy configuration and selective routing mechanisms.
It confirms that our local infrastructure safely and correctly integrates with the Windows proxy stack.

## Test Phases

### Phase A: Build Validation
- **Goal**: Ensure the Rust backend and Go SpoofDPI engine compile cleanly and deterministically.
- **Method**: Clean workspace builds and test runs.
- **Status**: **PASS**. Both binaries generated cleanly.

### Phase B: Loopback Security Binding
- **Goal**: Prevent external network exposure.
- **Method**: Validating that all proxy endpoints bind exclusively to `127.0.0.1`.
- **Status**: **PASS**. Adapter (proxy), Diagnostics, PAC Server, and SpoofDPI engine all bind correctly to `127.0.0.1` on dynamically allocated ephemeral ports.

### Phase C: Proxy State Restoration
- **Goal**: Guarantee that `ozii-cli` safely recovers or restores Windows proxy state after exit or crash.
- **Method**: Scripted registry capture of `HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings`. A simulated crash (SIGKILL) of `ozii-cli` was executed, followed by startup recovery validation.
- **Status**: **PASS**. The CLI uses a graceful termination handler and a startup recovery check (`proxy_state.rs`) to scrub orphaned `AutoConfigURL` registry keys left over from unexpected process death.

### Phase D: PAC Logic Functional Tests
- **Goal**: Prove that the generated `proxy.pac` strictly filters Discord domains.
- **Method**: Evaluated `proxy.pac` against a suite of domains using a headless JavaScript (JScript) engine harness.
- **Status**: **PASS**. `discord.com`, `gateway.discord.gg`, and CDN domains resolve to the `PROXY 127.0.0.1:<PORT>` string, while `google.com` and malformed entries fall through to `DIRECT`.

### Phase E: Adapter Strictness & Security
- **Goal**: Ensure the local DPI proxy rejects non-Discord domains even if a client bypasses the PAC file.
- **Method**: Direct `CONNECT` tests to the Adapter Proxy simulating malicious or misconfigured clients.
- **Status**: **PASS**. The adapter correctly parses the `CONNECT` host header. Unrelated requests (e.g. `google.com`) yield `HTTP/1.1 403 Forbidden` and increment `non_discord_rejected`, while `non_discord_forwarded` remains zero.

### Phase F: Browser Routing Real-World Tests
- **Goal**: Confirm that Windows Web Browsers (Edge, Chrome) obey the ephemeral PAC file.
- **Method**: System proxy was hijacked with the `AutoConfigURL`. Diagnostics API observed zero requests for `google.com` because Windows directed them to `DIRECT`.
- **Status**: **PASS**. Non-discord traffic successfully bypassed the proxy entirely, confirming the zero-impact invariant for unrelated user traffic.
