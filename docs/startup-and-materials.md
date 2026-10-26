# Startup and native glass investigation

## Measured startup causes

The standalone single-file yt-dlp executable took about **7.2–8.0 seconds** to print `--version`, without a network request. A representative default extraction took **11.31 seconds**, followed by **4.66 seconds** of provider availability waiting, reaching a usable partial response in **16.39 seconds**.

Changes:

1. Build a frozen **directory** runtime from the pinned upstream tag/recipe. Warm `--version` runs measured **0.27–0.38 seconds**. A first-use directory run still took **5.76 seconds**; platform validation/loading is a likely contributor, not a traced OS-level finding.
2. Try the tested single `visionos` client first, retaining the normal default client as fallback. Three public song/video probes returned usable HTTP 206 audio URLs in approximately **2.01, 2.25, and 2.30 seconds**, with the available AAC/Opus formats retained. Required availability metadata is still honored.
3. Validate the selected source with a tiny range request. Keep at most eight resolved tracks privately in Rust for five minutes; reject near-expired signed URLs and probe cache hits. Do not persist stream URLs or headers.
4. Warm the extractor and native engine once in the background. There is no persistent Python/Node server or bulk audio prefetch. The warmed native engine increases idle residency; memory/energy budgets are not yet revalidated.

Real muted libmpv/CoreAudio tests observed **3.39 seconds** from selection to decoded output after runtime warm-up, and **0.87–0.88 seconds** for a cached repeat. An initial fresh-path run took **8.09 seconds**. These are individual host/network observations, not a p95 or a guarantee for every track/account/region. Fallback and first-use loading can be slower.

Final packaged-runtime checks, with PATH restricted to `/usr/bin:/bin`, measured **4.90 seconds** for the first selection, **0.89 seconds** for its cached repeat, and **3.39 seconds** for another guest track. The installed app measured **197.70 MiB**, compared with the earlier 184.27 MiB layout; a local ZIP measured approximately **97 MiB**, compared with the earlier 113 MiB. The frozen-directory layout trades additional installed files for reusable loading and a smaller compressed artifact in this build.

## Genuine Home data

Anonymous `FEmusic_home` returned real provider shelves, including Quick picks, Take it easy, and a regional releases/hits shelf in observed runs. A recommended Mellow Pop Classics collection opened **49 actual tracks**, including duration metadata. Titles and order are provider data. There is no artist spotlight/search-query fallback. Authentication/personalized account feeds remain separate work.

## Native desktop material contract

Tauri 2.12.5 / window-vibrancy 0.8.1 was inspected for compatibility. It supports `LiquidGlassClear` on macOS 26+, with a classic material fallback. Vessel uses clear glass and HUD fallback, a clear window background, non-opaque WebView, transparent main stage, and 18% default Duskfox tint. The previous default tint migrates while explicit customization and reduced-transparency choices are preserved.

An opt-in **real AppKit/WKWebView** report observed:

- `NSWindow.isOpaque = false`
- Native window background alpha **0**
- Installed native glass view with **clear style 1**, non-opaque
- Actual Wry/WKWebView and its parent: non-opaque
- Mounted production frontend/player, **three live Home shelves**, no alert
- Web tint **0.18**, main-stage background `rgba(0, 0, 0, 0)`

ScreenCaptureKit could not provide a reliable foreground desktop pixel comparison in this harness; those attempted captures are not counted as visual transparency proof. Native material/renderer properties above are actual observations. WebKit screenshots use controlled IPC fixtures. Fullscreen/dragging/accessibility and full compositor/power QA remain release work.

### Reproduce the material contract (macOS 26+)

With reduced transparency disabled, build the opt-in developer feature:

```sh
npm run tauri -- build --no-bundle --features native-diagnostics
VESSEL_NATIVE_DIAGNOSTICS="$TMPDIR/vessel-material.json" VESSEL_NATIVE_DIAGNOSTICS_EXIT=1 VESSEL_RUNTIME_DIR="$PWD/src-tauri/runtime" src-tauri/target/release/vessel-app -ApplePersistenceIgnoreState YES -NSQuitAlwaysKeepsWindows NO
node scripts/check-native-material.mjs "$TMPDIR/vessel-material.json"
```

The feature is disabled in normal builds. Reports contain native class/opacity/style data and aggregate DOM checks, not user text, provider identifiers, credentials, or screen captures. `VESSEL_NATIVE_DIAGNOSTICS_EXIT=1` quits cleanly after reporting. The OS persistence flags affect only this diagnostic launch.
