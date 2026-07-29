# PrivateSparta Desktop

A lightweight desktop client for the PrivateSparta VPN service. Tauri v2 (Rust) +
React, with [sing-box](https://github.com/SagerNet/sing-box) as a supervised
sidecar process. No Electron, no Flutter, no telemetry.

## Privacy

**The app makes zero network requests the user didn't initiate.** No accounts,
no analytics, no crash reporting, no remote config. The only network activity
is the tunnel itself and subscription fetches the user explicitly adds.

## Bundled core

| | |
|---|---|
| Core | sing-box v1.13.15 (windows-amd64) |
| Source | official SagerNet GitHub release |
| SHA-256 | `4DB8218DEA131668CCD5E0B32E773E916A37BE730E655B176E0D3A930276CBE7` |

The binary lives at `src-tauri/binaries/sing-box-x86_64-pc-windows-msvc.exe`
and is spawned as a sidecar — never embedded, never granted more than a
generated, `sing-box check`-validated config. Its clash API and all inbounds
bind to `127.0.0.1` only, with a random 32-byte secret per launch that is never
written to disk.

## Building (Windows)

Prerequisites: Node 20+, Rust stable (MSVC), Visual Studio Build Tools with the
C++ workload, WebView2 runtime (preinstalled on Windows 11).

```
npm install
npm run tauri dev     # development
npm run tauri build   # NSIS installer
```

Tests: `cargo test` in `src-tauri/` (parsers, config generator, state machine,
store, redaction), `npx tsc --noEmit` for the frontend.

## Code signing

Release builds must be Authenticode-signed or SmartScreen will warn on every
install, which this audience will (rightly) treat as disqualifying. Needed: an
OV or EV code-signing certificate (EV skips SmartScreen reputation building),
configured via `bundle.windows.certificateThumbprint` in `tauri.conf.json` or
`signtool` in CI. Unsigned local builds are for development only.

## Security properties

- All parsing, credentials, and networking live in the Rust core. Node
  credentials never cross the IPC boundary; the WebView sees only names,
  protocol labels, and masked endpoints.
- Credential redaction (`uuid`, `pbk`, passwords/keys) is applied to every log
  path and error message.
- The sing-box process is assigned to a Windows job object with
  kill-on-close, so the core dies with the app — including on crash.
- The previous system-proxy state is written to disk *before* it is changed and
  restored on disconnect, quit, and (via a dirty-exit marker) on the next
  launch after a crash.
- Config is stored in the per-user app-data directory with atomic
  temp-file-and-rename writes and a versioned schema.
