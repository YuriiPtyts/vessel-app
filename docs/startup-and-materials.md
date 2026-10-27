# Startup and native glass investigation

## Extractor-only baseline (2026-10-26)

The standalone single-file yt-dlp executable took about **7.2–8.0 seconds** to print `--version`, without a network request. A representative default extraction took **11.31 seconds**, followed by **4.66 seconds** of provider availability waiting, reaching a usable partial response in **16.39 seconds**.

Changes:

1. Build a frozen **directory** runtime from the pinned upstream tag/recipe. Warm `--version` runs measured **0.27–0.38 seconds**. A first-use directory run still took **5.76 seconds**; platform validation/loading is a likely contributor, not a traced OS-level finding.
2. Try the tested single `visionos` client first, retaining the normal default client as fallback. Three public song/video probes returned usable HTTP 206 audio URLs in approximately **2.01, 2.25, and 2.30 seconds**, with the available AAC/Opus formats retained. Required availability metadata is still honored.
3. Validate the selected source with a tiny range request. Keep at most eight resolved tracks privately in Rust for five minutes; reject near-expired signed URLs and probe cache hits. Do not persist stream URLs or headers.
4. Warm the extractor and native engine once in the background. There is no persistent Python/Node server or bulk audio prefetch. The warmed native engine increases idle residency; memory/energy budgets are not yet revalidated.

Real muted libmpv/CoreAudio tests observed **3.39 seconds** from selection to decoded output after runtime warm-up, and **0.87–0.88 seconds** for a cached repeat. An initial fresh-path run took **8.09 seconds**. These are individual host/network observations, not a p95 or a guarantee for every track/account/region. Fallback and first-use loading can be slower.

Final packaged-runtime checks, with PATH restricted to `/usr/bin:/bin`, measured **4.90 seconds** for the first selection, **0.89 seconds** for its cached repeat, and **3.39 seconds** for another guest track. The installed app measured **197.70 MiB**, compared with the earlier 184.27 MiB layout; a local ZIP measured approximately **97 MiB**, compared with the earlier 113 MiB. The frozen-directory layout trades additional installed files for reusable loading and a smaller compressed artifact in this build.

## Rust guest direct path (2026-10-27)

A warmed first-selection regression still took **3.503 seconds** before this change (resolution/load acknowledgement **2.825 seconds**). The common guest path now requests the same tested player profile directly in Rust, avoiding extractor startup. Background warm-up primes a bounded visitor context and the native engine alongside account restore; it no longer launches the extractor's `--version` command.

- Bootstrap guest visitor data from YouTube; retain it in memory for 15 minutes. Both HTML and JSON responses are capped at 2 MiB.
- Accept only a matching video ID, playable recorded content, and direct HTTPS Googlevideo audio formats. Cipher/`n` transformation, DRM, live content, rejected/unusable sources, and unsupported responses use the existing extractor fallback. No account cookies are sent by the direct guest client.
- The direct request **and its source probe together** have a two-second budget. Existing account revision checks, cancellation, source probes, private eight-track cache, and per-source availability handling remain in force. Signed URLs and headers are not sent to the frontend or persisted.
- The native audio buffer is 50 ms. This is a configured buffer, not a measured acoustic-latency guarantee.

The warmed regression reached advancing decoded native output in **0.630**, **0.657**, and **0.778 seconds** on this host. A separate catalog flow without explicit warm-up took **2.262 seconds**, with a **0.870-second** cached repeat. These are individual muted-output observations, not cold-install or p95 guarantees. Independent real extractor checks returned usable sources through both compatibility profiles: **2.779 seconds** (`visionos`) and **7.750 seconds** (default); fallback can therefore exceed the direct-path target.

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked --test live_startup -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib live_compatibility_profiles_return_usable_guest_sources -- --ignored --nocapture --test-threads=1
```

## Genuine Home data

Anonymous `FEmusic_home` returned real provider shelves, including Quick picks, Take it easy, and a regional releases/hits shelf in observed runs. A recommended Mellow Pop Classics collection opened **49 actual tracks**, including duration metadata. Titles and order are provider data. There is no artist spotlight/search-query fallback. Authentication/personalized account feeds remain separate work.

## Native desktop material contract

Tauri 2.12.5 / window-vibrancy 0.8.1 was inspected for compatibility. It supports `LiquidGlassClear` on macOS 26+, with a classic material fallback. Vessel uses clear glass and HUD fallback, a clear window background, non-opaque WebView, and transparent main stage. Following the owner's readability correction, Duskfox tint now defaults to **50%**, with a **50–100%** control. Existing values below 50 are clamped and saved; other appearance choices and local collection data are preserved. At 100%, or with reduced transparency enabled, the frontend is opaque and native material effects are removed.

An opt-in **real AppKit/WKWebView** report, rerun on 2026-10-27, observed:

- `NSWindow.isOpaque = false`
- Native window background alpha **0**
- Installed native glass view with **clear style 1**, non-opaque
- Actual Wry/WKWebView and its parent: non-opaque
- Mounted production frontend/player, **three live Home shelves**, no alert
- Web tint **0.5**, main-stage background `rgba(0, 0, 0, 0)`

ScreenCaptureKit could not provide a reliable foreground desktop pixel comparison in this harness; those attempted captures are not counted as visual transparency proof. Native material/renderer properties above are actual observations. WebKit screenshots use controlled IPC fixtures. Fullscreen/dragging/accessibility and full compositor/power QA remain release work.

### Reproduce the material contract (macOS 26+)

With reduced transparency disabled, build the opt-in developer feature:

```sh
npm run tauri -- build --no-bundle --features native-diagnostics
VESSEL_NATIVE_DIAGNOSTICS="$TMPDIR/vessel-material.json" VESSEL_NATIVE_DIAGNOSTICS_EXIT=1 VESSEL_RUNTIME_DIR="$PWD/src-tauri/runtime" src-tauri/target/release/Vessel -ApplePersistenceIgnoreState YES -NSQuitAlwaysKeepsWindows NO
node scripts/check-native-material.mjs "$TMPDIR/vessel-material.json"
```

The feature is disabled in normal builds. Reports contain native class/opacity/style data and aggregate DOM checks, not user text, provider identifiers, credentials, or screen captures. `VESSEL_NATIVE_DIAGNOSTICS_EXIT=1` quits cleanly after reporting. The OS persistence flags affect only this diagnostic launch.
