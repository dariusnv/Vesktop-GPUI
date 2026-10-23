# Native client research and roadmap

Research date: 2026-10-24. Six research agents covered Vesktop architecture,
GPUI/Windows, authentication/protocol, media, libraries and performance. Two also
reviewed the implementation. Main-task code was implemented by the primary agent.

## Feasibility decision

A native GPUI client is a Rust rewrite. Vesktop is an Electron shell that loads
Discord's proprietary web client and injects Vencord. It does not contain portable
implementations of Discord's messaging UI, stores or WebRTC stack. Removing the
browser requires implementing those features separately.

Sources: [Vesktop preload](https://github.com/Vencord/Vesktop/blob/main/src/preload/index.ts),
[main window](https://github.com/Vencord/Vesktop/blob/main/src/main/mainWindow.ts),
[screenshare](https://github.com/Vencord/Vesktop/blob/main/src/main/screenShare.ts).

## Weekend milestone

The first build supplies native navigation, input, a virtualized transcript, bounded
state and a public bot API connection. The remaining weekend work should prioritize
testing with an authorized bot in a private server, better text rendering and paging,
then persistence. Avoid spending the weekend on a new media or cryptography stack.

| Priority | Feature | Status / next step |
| --- | --- | --- |
| P0 | Build/run on Windows | Native release compiles; UI smoke test and results recorded separately |
| P0 | State correctness | Bounded cache, deduplication, snapshot reconciliation and draft preservation |
| P0 | Live text vertical slice | REST/Gateway adapter implemented; verify with user-provided bot credentials |
| P0 | Personal-account access | Unresolved platform access/product decision; public API is insufficient |
| P1 | Rich text / inline media | Implement native Markdown spans, mentions, emoji and bounded image decoding |
| P1 | History / threads | Sliding page window or SQLite-backed pagination; forums and thread navigation |
| P1 | Persistence | Background SQLite worker, account-specific cache, explicit clear operation |
| P1 | Desktop integration | Tray, notifications, protocol links and RPC |
| P2 | Voice | DAVE-capable audio transport plus devices and UX; separate major milestone |
| P2 | Video / screenshare | Capture, encoding, packetization, DAVE, decoding and per-OS integration |
| P2 | Plugin compatibility | Selectively reimplement features; JS/webpack patches cannot run in GPUI |
| P2 | Distribution | Installer, updates, signing and cross-platform builds |

## Authentication constraint

Ordinary OAuth2 `identify` and `guilds` scopes expose identity and guild summaries,
not unrestricted server messaging, DMs or friends. Several communication/RPC scopes
are partner-restricted. There is no documented general-purpose replacement-client
login flow in the public API. This prototype therefore labels its supported bot
developer session explicitly; it is not a signed-in personal client.
[Discord OAuth2 scope documentation](https://docs.discord.com/developers/topics/oauth2).

The Social SDK provides game communication integrations, but is not a general
desktop-client SDK, has access/review requirements, and terms that restrict competing
products. It does not solve full-client access automatically.
[SDK communication requirements](https://docs.discord.com/developers/discord-social-sdk/core-concepts/communication-features),
[SDK terms](https://support-dev.discord.com/hc/en-us/articles/30225844245271-Discord-Social-SDK-Terms).

## Media constraint

Discord reports that E2EE is required for ordinary voice/video and Go Live calls;
Stage is an exception. DAVE adds MLS membership/key management and media transforms
on top of the voice gateway, UDP/RTP/Opus and transport encryption. A generic Rust
WebSocket or audio library does not provide client voice/video parity.
[Discord E2EE announcement](https://discord.com/blog/every-voice-and-video-call-on-discord-is-now-end-to-end-encrypted),
[voice protocol](https://docs.discord.com/developers/topics/voice-connections),
[DAVE protocol](https://github.com/discord/dave-protocol),
[official libdave](https://github.com/discord/libdave).

## Technical choices

- Published GPUI `=0.2.2` and committed Cargo.lock. Current Zed main uses a different
  split-platform API; mixing examples from that generation breaks released builds.
  [Versioned GPUI docs](https://docs.rs/gpui/0.2.2/gpui/).
- Twilight `=0.17.1` for HTTP/models/Gateway: one protocol ecosystem handles rate
  limits, reconnect, heartbeat and resume. The UI does not implement those again.
  [Twilight HTTP](https://docs.rs/twilight-http/0.17.1/twilight_http/),
  [Gateway](https://docs.rs/twilight-gateway/0.17.1/twilight_gateway/).
- Separate core/transport/app crates allow fast state tests without the GPUI link.
- Variable-height GPUI ListState for messages; no full-history layout on each frame.
  Network I/O runs on a two-thread Tokio runtime, with bounded commands/events.
- Offline sessions perform no periodic network refresh. Live Gateway heartbeats
  are required protocol traffic, not avoidable idle overhead.

## Alternative if full daily-driver functionality this weekend is essential

Continue using Vesktop's existing Electron application, or evaluate a Rust webview
shell as a separate product choice. A Rust webview shell retains a browser engine
and does not meet the browser-free GPUI goal. Rewriting the shell alone cannot
eliminate the rendering/runtime cost of Discord's web application.
