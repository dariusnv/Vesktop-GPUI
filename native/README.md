# Vesktop Native

A Windows-first Rust / GPUI prototype in a fork of [Vencord/Vesktop](https://github.com/Vencord/Vesktop).
The native executable has no Electron, Chromium, webview, DOM, or JavaScript runtime.
GPUI itself is Rust; this is the Rust rewrite track.

**Status: working native foundation, not a fully fledged personal Discord client.**
The default mode uses clearly labeled offline sample data. Live developer mode uses
the documented Discord bot API and sends as a bot. It cannot log in as your normal
Discord account, access your personal DMs/friends, or reuse Vencord's JS plugins.
Nothing is persisted yet: offline messages and drafts disappear on exit.

## Run

Prerequisites on Windows: stable Rust with the MSVC target, Visual Studio Desktop
C++ build tools, and Windows SDK 10.0.26100.0. This revision was built with Rust 1.99.0.
GPUI is pinned to 0.2.2; the lockfile pins the dependency graph.

From this directory:

```powershell
cargo run --release -p vesktop-gpui
```

The first build compiles GPUI and its dependencies. Later builds are incremental.
If GPUI cannot locate the shader compiler, set `GPUI_FXC_PATH` to the full path to
your Windows SDK `x64/fxc.exe`. The release build was verified on Windows; Linux
and macOS packaging and feature configuration are not validated in this milestone.

To build a portable Windows binary with prerequisites checked:

```powershell
./scripts/build.ps1
./dist/vesktop-gpui.exe
```

## What works

- Native server/channel navigation and variable-height virtualized message history.
- Unicode and IME-aware single-line composer, selection, clipboard, and horizontal scrolling.
- Per-channel drafts during the session; Enter sends locally in offline mode.
- Search within loaded messages; `Ctrl+F` focuses search, `Ctrl+L` focuses the composer.
- `Ctrl+R` refreshes live history; `Ctrl+Q` exits. Copy JSON exports the loaded channel to the clipboard.
- Bounded histories: 500 messages per channel, at most 8 cached channels.
- Optional bot REST/Gateway: list installed servers, list text/announcement channels,
  load recent history and older pages within the window, send text, receive edits,
  deletes and bulk deletes. Attachments open from Discord's HTTPS CDN.
- Bounded cross-thread queues, background I/O, rate-limit handling and reconnects
  through Twilight. In-flight history snapshots preserve newer live edits/deletions.

## Optional live bot session

Create/install your own Discord bot using the [Developer Portal](https://discord.com/developers/applications).
Grant View Channels, Read Message History, and Send Messages in your test server.
Enable the **Message Content** privileged intent. The UI may list channels whose
permissions deny the bot access; request failures are reported in the status bar.

Provide `VESKTOP_BOT_TOKEN` locally through your process environment, then run:

```powershell
cargo run --release -p vesktop-gpui -- --bot
```

Use a bot token. The program does not discover tokens, accept account passwords,
or persist credentials. Keep the token out of command history and source files.
Bot mode requires the explicit `--bot` flag even when the environment variable is set.
Messages are sent as the bot and allowed mentions are disabled. Live connectivity
requires your authorized token and server; it has not been verified against a real
account in this environment.

## Scope and weekend plan

Vesktop embeds Discord's web client. A full native port must implement the UI,
state, messaging, media stack and desktop integrations that Chromium currently
provides. This is a new client implementation, not a mechanical UI toolkit swap.

A weekend-sized deliverable is this native foundation plus a narrow messaging
vertical slice. Full personal-account parity is a separate project and has an
unresolved access constraint: ordinary Discord OAuth scopes do not provide full
message/DM/friend access for a replacement client. See the [research and roadmap](docs/ROADMAP.md).

Not implemented: personal login/OAuth flow, DMs, friends, thread/forum navigation,
rich Markdown, inline media/avatars/emoji, reactions, message editing/deleting UI,
uploads, notifications/tray/RPC, persistent cache/settings, plugins, voice/video,
screen share, update/install packaging, accessibility support equivalent to Discord.
History paging stops at the configured 500-message window. The composer is single-line.

## Validate

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --locked
```

Tests cover bounded/deduplicated state, Unicode content limits, stale REST snapshots
versus live changes, and IME selection offsets. Building does not verify real Discord
permissions or media interoperability.

Use [the benchmark script and protocol](docs/PERFORMANCE.md) for performance claims.
Removing Chromium does not remove all overhead or establish that this is the fastest
client. No Electron baseline comparison has been measured yet.

## Structure

`crates/core` owns data and bounded state; `crates/discord` owns the public bot API
adapter; `crates/app` owns GPUI presentation/input. The split lets state tests run
without building the renderer. The original Vesktop TypeScript application remains
at the repository root as migration reference.

The fork retains GPL-3.0-or-later. The input element is adapted from GPUI's
Apache-2.0 example; see `GPUI-INPUT-LICENSE` and `NOTICE`.
