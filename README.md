# PrivateSparta Desktop

A lightweight desktop client for the PrivateSparta VPN service. Tauri v2 (Rust)
+ React, with [sing-box](https://github.com/SagerNet/sing-box) as the default
supervised sidecar and [Xray-core](https://github.com/XTLS/Xray-core) as an
XHTTP-only fallback. No Electron, no Flutter, no telemetry.

Written by **H. Talebi** — <https://github.com/mr-physicist2002>

---

## Privacy

**The app makes zero network requests the user didn't initiate.** No accounts,
no analytics, no crash reporting, no ads, no remote config. The only network
activity is:

| Activity | When |
|---|---|
| The tunnel itself | While connected |
| Subscription fetch | When you add or update a subscription, or on an interval you chose |
| Routing rule refresh | Only if you turn on auto-refresh (**off** by default) |
| Update check | Only when you press "Check for updates" |

Nothing runs on a background timer unless you switched it on.

## Features

- **Protocols:** VLESS (incl. REALITY), VMess, Trojan, Shadowsocks (SIP002 +
  legacy), Hysteria2, TUIC v5, WireGuard.
  Transports: tcp, ws, grpc, httpupgrade, h2, xhttp. Security: none, tls,
  reality. XHTTP uses Xray only when an XHTTP server is selected; other
  connections remain on sing-box.
- **Subscriptions:** base64 URI list, plain URI list, sing-box JSON, and
  Clash/Clash.Meta YAML. Reads `subscription-userinfo` for used/total traffic
  and expiry. Fetches direct when disconnected and **through the tunnel** when
  connected, so updates work under censorship.
- **Modes:** System Proxy, Proxy Only, and TUN (needs administrator rights).
- **Routing:** Iranian sites, private ranges, LAN, and loopback go direct;
  ads blocked; everything else through the tunnel. Rule-sets ship bundled, so
  the first connect never waits on a download.
- **DNS:** local resolver for direct queries, DoH (1.1.1.1) through the tunnel
  for proxied ones, FakeIP in TUN mode, DNS-leak protection on.
- **Server list:** virtualized (smooth past 500 nodes), searchable, grouped by
  subscription, latency badges, per-node and concurrent Test All, favorites.
- **Languages:** English, Persian (full RTL), Chinese (Simplified), Russian.

## Bundled components

| Component | Version | SHA-256 |
|---|---|---|
| sing-box (windows-amd64) | 1.13.15 | `4DB8218DEA131668CCD5E0B32E773E916A37BE730E655B176E0D3A930276CBE7` |
| Xray-core (windows-amd64 executable) | 26.3.27 | `15C2D007954AC53BA69B80EC91242786B3C0B71D52649165B4CA1D5CC96EF8F1` |
| wintun | 0.14.1 | `E5DA8447DC2C320EDC0FC52FA01885C103DE8C118481F683643CACC3220DAFCE` |

The selected core is spawned as a sidecar — never embedded, and never handed
anything but a freshly generated configuration validated by that core. Xray is
started only for XHTTP servers; all established sing-box paths are unchanged.
The Clash API and all local inbounds bind to `127.0.0.1` by default, with a
random 32-byte Clash secret per sing-box launch. CI re-downloads both cores per
platform and verifies Xray against the SHA-256 in GitHub's release metadata.

## Measured performance

Release build, Windows 11 x64 (8 cores), warm OS, measured against a live
subscription — not estimates.

| Metric | Target | Measured |
|---|---|---|
| Cold start to interactive window | < 800 ms | **539–735 ms** |
| Idle RAM (app process, core excluded) | < 70 MB | **5.2 MB** |
| Idle CPU while connected (app process) | < 1 % | **0.08 %** |
| Connect → tunnel up (warm) | < 1.5 s | **≈ 590 ms** |
| Installer size | < 12 MB app only | **13.5 MB total**, ≈ 3 MB app share |

How these were taken: cold start is process launch to a valid main-window
handle across repeated runs. Idle CPU is `TotalProcessorTime` delta over 30 s
while connected, divided by wall time and core count. Connect is config
generation + `sing-box check` (72 ms) plus spawn to the core accepting
connections (517 ms).

Two honest caveats:

- **First connect after a cold boot is slower.** Windows Defender scans the
  45 MB sing-box binary on its first execution, which pushed `sing-box check`
  from 72 ms to about 2 s in a cold run — roughly a 2.5 s first connect. Every
  later connect is warm. Excluding the bundled core from Defender's scan path,
  or shipping a smaller core build, would remove it.
- **Scroll frame time is not instrumented.** The list renders only the visible
  window plus overscan, so cost per frame is independent of node count, but no
  number here is measured — treat it as unverified.

The measured installer figure predates the optional XHTTP sidecar. Bundling
Xray increases installer size, but does not change the normal sing-box runtime
path or its measured connection performance.

WebView2 host processes add ~140 MB outside our process; that is the shared
system WebView runtime, not the app's own footprint, and no WebView-based
client can avoid it. The core process itself sits around 55 MB while connected.

### Verified end to end

Against a live subscription, on the release build:

- 12 nodes parsed with no skips, including emoji and Persian names.
- `subscription-userinfo` parsed (used/total traffic and expiry).
- Latency probes returned for 8 of 12 nodes through the clash API.
- Traffic genuinely leaves through the tunnel — the exit IP changes both via
  the test harness and via the running app's local inbound.
- Hard-killing the app mid-connection leaves **no orphaned core** (the Windows
  job object collects it), and the next launch restores the previous system
  proxy exactly, including its bypass list, then clears the dirty marker.

## Building

Prerequisites: Node 20+, Rust stable (MSVC on Windows), Visual Studio Build
Tools with the C++ workload, WebView2 runtime (preinstalled on Windows 11).

```bash
npm install
npm run tauri dev
```

```bash
npm run tauri build
```

Before building from a clean checkout, place the sing-box and Xray binaries at
`src-tauri/binaries/sing-box-x86_64-pc-windows-msvc.exe` and
`src-tauri/binaries/xray-x86_64-pc-windows-msvc.exe`. Xray's `geoip.dat`,
`geosite.dat`, and `LICENSE` belong in `src-tauri/resources/xray/`. These files
are intentionally not committed. The release workflow downloads them from the
official releases and verifies Xray's archive digest automatically.

### Tests

```bash
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
```

```bash
npx tsc --noEmit
```

An end-to-end test runs the real path — fetch, parse, generate, `sing-box
check`, spawn, latency-probe, and confirm the exit IP actually changed. It is
ignored by default and takes the subscription URL from the environment, so no
credential lives in the repository:

```bash
PRIVATESPARTA_TEST_SUB="https://your-subscription-url" cargo test --lib live -- --ignored --nocapture
```

## Releasing

Tag a version and CI builds and drafts a signed release for Windows, macOS
(arm64 + x64), and Linux:

```bash
git tag v0.1.1 && git push origin v0.1.1
```

Required repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY` — contents of the updater private key
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — its password (empty if none)

The updater public key is committed in `src-tauri/tauri.conf.json`; the private
key is **not** in the repository. If you lose it, existing installs can no
longer be updated and every user must reinstall manually.

### Windows code signing

Release builds should be Authenticode-signed, or SmartScreen warns on every
install — which this audience will rightly treat as disqualifying. You need an
OV or EV certificate (EV skips SmartScreen reputation building), configured via
`bundle.windows.certificateThumbprint` in `tauri.conf.json` or `signtool` in
CI. Unsigned local builds are for development only.

## Platform status

- **Windows 10/11 x64** — complete and tested.
- **macOS / Linux** — the app builds through CI, but the system-proxy
  integration (`networksetup` on macOS, GSettings on Linux) has not been run on
  those platforms. Treat it as untested. Proxy Only mode is unaffected. On
  Linux desktops without GSettings, System Proxy mode reports that plainly and
  suggests Proxy Only or TUN.

## Security properties

- All parsing, credentials, and networking live in the Rust core. Node
  credentials never cross the IPC boundary; the WebView sees only names,
  protocol labels, and masked endpoints. "Copy link" rebuilds the share URI in
  Rust and writes it straight to the clipboard.
- Credential redaction (UUIDs, `pbk`, passwords, keys) is applied to every log
  path and error message, including the Logs screen.
- Every tunnel-core process is assigned to a Windows job object with
  kill-on-close, so it dies with the app — including on crash.
- The previous system-proxy state is written to disk *before* it is changed and
  restored on disconnect, on quit, and on the next launch after a crash.
- Config is stored in the per-user app-data directory with atomic
  temp-file-and-rename writes and a versioned schema with migrations.

## License

MIT — see [LICENSE](LICENSE).
