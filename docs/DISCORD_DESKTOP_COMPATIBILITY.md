# Discord Desktop Compatibility

The Discord Desktop application is an Electron/Chromium-based application. When Windows Proxy settings (and our PAC file) are configured, Chromium naturally routes its standard HTTP/HTTPS/WebSocket traffic through the proxy based on the PAC rules.

## What is Proxied (DPI Evaded)
The following functionality reliably utilizes the proxy and benefits from the DPI evasion engine:
1. **Chat & Text Messages**: Communicated over WSS (WebSockets) via `gateway.discord.gg`.
2. **Media Loading**: Images and attachments fetched over HTTPS from `cdn.discordapp.com` and `media.discordapp.net`.
3. **API Requests**: Standard REST API calls to `discord.com/api`.

These connections use standard TCP `CONNECT` methods to our Adapter Proxy and are fragmented successfully by SpoofDPI.

## What is NOT Proxied (Voice & Video)
Discord WebRTC Voice and Video traffic does **NOT** flow over standard HTTP proxies. 
- It communicates using native UDP sockets (STUN/TURN/SRTP) to establish direct connections to voice servers.
- It intentionally bypasses WinINET proxy settings and PAC files.

### Design Decision
In the current architecture, **OziiDPI does not intercept or manipulate UDP traffic.**
- We avoid invasive global interception methods like WinDivert or Windows Filtering Platform (WFP) to prevent breaking generic gaming and unrelated UDP traffic.
- Since most localized DPI censorship regimes target SNI (Server Name Indication) in TCP TLS Handshakes for text/web access, UDP voice traffic is often left untouched or isn't aggressively fingerprinted. 
- **Compatibility Status**: Voice channels should function normally without DPI intervention, assuming the local ISP does not strictly drop UDP packets targeting Discord's voice IP ranges. If an ISP fully blocks Discord UDP ranges, a more invasive process-specific injection or global VPN/TUN mechanism would be required in the future.
