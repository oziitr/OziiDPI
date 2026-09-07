# Selective Discord Routing Architecture

OziiDPI maintains a strict policy: **Only Discord domains go through our local DPI proxy. Unrelated traffic MUST bypass the engine entirely.**

## 1. Domain Allowlist
The routing is driven by a hardcoded static allowlist found in `DomainAllowlist`. This list covers all necessary boundary domains for Discord including:
- `discord.com` (Main web, api, canary, ptb)
- `discord.gg` (Invites and gateway `gateway.discord.gg`)
- `discordapp.com` (CDN endpoints)
- `discordapp.net` (Media endpoints)

## 2. The Local PAC Server
To tell Windows how to route traffic, OziiDPI generates and hosts a Proxy Auto-Config (PAC) file on a dynamic local port.
The PAC script instructs the OS to forward traffic to `127.0.0.1:<adapter_port>` *only* if the hostname matches one of the domains in the allowlist. If the user previously had a proxy configured, the PAC script gracefully falls back to using their original proxy server for non-Discord traffic.

## 3. Defense-In-Depth: The Adapter Proxy
Even with the PAC script in place, misbehaving applications might mistakenly send non-Discord `CONNECT` requests to our proxy port. To prevent the SpoofDPI engine from becoming an open general-purpose proxy, we implemented an **Adapter Proxy**.

The flow is:
`Client` -> `Adapter Proxy (Rust)` -> `SpoofDPI (Go)` -> `Internet`

The Adapter Proxy intercepts every HTTP `CONNECT` request, parses the destination hostname, and performs a secondary validation against the `DomainAllowlist`.
- If the domain is allowed, it streams the connection to the DPI Engine and increments the `discord_routed_connections` metric.
- If the domain is NOT allowed, it returns `HTTP/1.1 403 Forbidden` and increments the `non_discord_connections` metric.

This guarantees that **non-Discord connections through the DPI proxy are zero.**

## 4. Diagnostics
You can query real-time routing metrics safely using the CLI:
```powershell
ozii-dpi diagnose
```
This hits a lightweight local JSON endpoint maintained by the Adapter Proxy to report connection counts and observed hosts without logging any TLS payloads or exact URLs.
