# Verification log

Date: 2026-10-26. Host: macOS 27.0.1, arm64, Xcode 27.0, Node 25.2.1, Rust 1.99.0.

## Foundation checks

- Svelte/TypeScript diagnostics: zero errors and warnings.
- Production frontend build.
- Rust formatting check.
- Clippy for all targets on the host, with warnings denied.
- Native arm64 release compilation and `.app` bundle generation.
- Packaged executable launch: remained running for six seconds with no captured stdout/stderr, then was terminated by the smoke harness.
- Dependency resolution: TypeScript is pinned to 6.0.3 because the current svelte-check default path does not support a TypeScript 7-only installation.

### Foundation interpretation

Those initial checks established compilation, packaging, and process launch only. Playback-slice evidence follows below.

The initial shell bundle is approximately 5.3 MiB. It contains no media engine, extractor, challenge runtime, or provider integration. Its size is not a projected final download size.

## Guest playback slice

### Passed

- Five provider unit tests, including filtering video/DRM formats, unknown bitrate handling, input-origin validation, redacted failures, and the stream-availability regression.
- One non-network integration check for invalid input/control bounds without starting native dependencies.
- Two WebKit presentation-contract checks: source-backed quality selection and native event/control wiring with mocked IPC; browser preview and 420px layout. These are not a claim that the real Tauri window was UI-automated.
- Real guest track resolution: `https://music.youtube.com/watch?v=dQw4w9WgXcQ` returned five audio-only formats on this session.
- Real libmpv/CoreAudio decoding with muted output: Opus at 48 kHz, advancing playback position, stable pause, seek to ten seconds, switch to AAC while preserving paused position, resume, stop.
- In-flight resolver cancellation returned promptly and released its concurrency slot.
- Pinned dynamic library loaded with only system dynamic dependencies; the preparation script enforces this arm64 dependency check.
- Release bundle generation and six-second app launch with `PATH=/usr/bin:/bin`.
- Real guest playback integration using `.app/Contents/Resources/runtime` and a system-only PATH, without developer playback/extraction tools.
- Svelte diagnostics, production frontend build, Rust formatting, Clippy with warnings denied, and ordinary Rust tests pass.

Commands:

```sh
npm run check
npm run test:ui
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
npm run tauri -- build --bundles app
cargo test --manifest-path src-tauri/Cargo.toml --test live_playback -- --ignored --nocapture --test-threads=1
```

The bundled-runtime run invoked the compiled integration-test executable directly with `VESSEL_RUNTIME_DIR` pointing at the app's `Contents/Resources/runtime` and PATH restricted to system directories. This avoids Cargo/Rust toolchain discovery influencing the playback runtime test. It is not a clean-machine or minimum-macOS test.

### Measurements and limitations

- Three successful real-track runs reached decoded native output in approximately **16.8 seconds**, **20.0 seconds**, and **19.2 seconds**, including stream resolution. These are individual observations, not p95 or audible-output measurements; output was muted.
- The initial four-second first-play target is **not met**. yt-dlp startup/challenge work and a provider-reported future availability timestamp contribute. Alternative clients/resolvers and bounded anticipatory resolution need investigation; do not remove the provider wait to hide this latency.
- Local installed `.app`: **180.91 MiB**. Local ZIP: approximately **112 MiB**. These are unsigned/not notarized development artifacts. The size is dominated by bundled runtimes and a general-purpose universal media build; an audio-focused build is a future size experiment.
- Native media-cache configuration: 16 MiB forward plus 8 MiB back. Whole-process-tree memory, CPU, power, startup p95, and long-session growth are **not yet measured**.
- Audio verification establishes native decoding/transport, not subjective sound quality or listening-device correctness.

### Diagnosed failures

1. kirbylink/mpv-natives v0.41.0's arm64 artifact referenced Homebrew libxcb/jpeg libraries despite its self-contained description. Rejected after `otool` inspection and failed `dlopen`; replaced by the checked zapper dynamic build.
2. Immediate playback of newly resolved URLs returned HTTP 403. A minimal direct-libmpv probe reproduced it; yt-dlp's downloader succeeded only after its site-required wait. Its pinned source reads per-format `available_at`. The resolver now respects that metadata, with bounded/cancellable waiting, and a regression test covers future/already-ready/excessive-delay cases. No fixed six-second sleep or media proxy was introduced.

### Remaining gates at the playback-only checkpoint

Optional account acquisition/storage/library reads; broader track/region/account matrix; automatic URL renewal and reconnect; queue persistence; gapless output; output-device and sleep/wake recovery; accessibility evaluation; native materials/media controls; full performance budgets; minimum-OS and clean-machine testing; dependency license/source package; signing/notarization/updater.

## Search-first music app milestone

### Verified

- Real anonymous YouTube Music search returned 20 songs with artists, albums, artwork, and durations. Observed initial request times ranged from roughly 1.4 to 4.1 seconds across individual runs; this is not a p95 benchmark.
- Continuation requests expanded the result set to 39–40 unique tracks in the observed runs. Music-video filtering returned 20 items. A regression test covers the provider returning both repeated initial `contents` and the actual `continuationContents` in the same response.
- A song selected from catalog results decoded through the native muted CoreAudio pipeline without a link-entry/inspection UI step, preserving catalog title/artwork. Saving and real playback history were exercised at the application boundary.
- Atomic local-collection persistence and preservation of malformed existing data pass filesystem tests.
- Four WebKit UI contract tests cover search-to-play, source quality in the player, transport shortcuts, saved/recent views, discovery/mood navigation, appearance, narrow layout, and protection against stale responses.
- Home, search, and narrow-layout screenshots were visually reviewed. These screenshots use controlled IPC and artwork fixtures, not a claim that the real Tauri window was UI-automated.
- Eleven Rust unit tests plus the non-network application integration test pass; Clippy, formatting, Svelte diagnostics, and the production frontend build pass.
- Two real catalog integration checks and both previous live playback/cancellation checks pass against bundled runtime artifacts with PATH restricted to `/usr/bin:/bin`.
- The rebuilt macOS `.app` remained running for eight seconds with a system-only PATH and no captured stdout/stderr. This validates launch with the native-window configuration; dragging, fullscreen, accessibility, and visual vibrancy still need hands-on OS QA.

### Current artifacts and limits

- Local `.app`: **184.27 MiB**. Local development ZIP (`.cache/Vessel-search.zip`): approximately **113 MiB**. Signing/notarization and public-release obligations are not completed.
- Search and stream resolution are distinct paths: fast catalog results do not remove the roughly 17-second observed stream-start delay. The latest playback regression reached decoded output in about 17.1 seconds, including resolution.
- Song and music-video search are implemented. Album/artist/playlist detail browsing, account support, persistent queues, auto-advance, automatic stream recovery, and full CPU/memory/energy budgets remain open.
- Local likes/history are not YouTube Music account state. Metadata is retained on this Mac; no account access is required.

## Duskfox glass, live volume, and artwork milestone

### Verified

- Confirmed the owner's configured Zed dark theme as **Duskfox - blurred** in Nightfox for Zed 0.10.5. Mapped its public palette values into semantic theme tokens; Zed configuration is neither copied into the repository nor read by the running app.
- The original pointer-held volume test went red: no `set_volume` request arrived before mouseup. It now passes with live input writes, local gesture retention, a final settling write, and a dedicated latest-value lane that keeps at most one native volume write in flight.
- A 90 ms mocked-native-delay test verifies responsive dragging, intermediate-value coalescing, exact final value, no control disabling, and protection from older playback snapshots. These are WebKit/IPC contract checks, not an acoustic latency measurement.
- Real bundled libmpv immediately read back the requested volume values `0`, `27`, `64`, `100`, and `0`. No audio was played for this native-boundary check. The same check passed using the `.app`'s runtime and a system-only PATH.
- Eight WebKit tests pass, including the prior music flows, Duskfox defaults, persisted accent/tint/density/sidebar/artwork settings, reset, reduced transparency removing CSS filters and requesting native effects removal, prominent artwork, and high-resolution-image fallback. Repeated playback snapshots do not restart failed larger-image requests.
- A real guest-catalog thumbnail returned a successful HTTP response with an image content type. Original provider thumbnails are the fallback for optional 512px Google resize requests.
- Customization and artwork screenshots were visually reviewed with controlled IPC/artwork fixtures. They are not native-window automation or a claim of compositor performance measurement.
- Svelte diagnostics, the production frontend build, Rust formatting/Clippy with warnings denied, ordinary Rust tests, real catalog pagination/artwork checks, and macOS app packaging pass.
- The Duskfox/dark Sidebar-material app remained running for eight seconds with a system-only PATH and no captured output. The packaged app remains approximately **184.27 MiB**.

### Scope

The app now has a single complete default palette, four palette accent choices, adjustable glass tint, opaque mode, compact/comfortable rows, compact navigation, and a collapsible now-playing artwork panel. Preferences migrate the previous opacity setting and are kept locally. Additional complete themes are an owner-directed future milestone in themes.md. Native fullscreen/dragging/accessibility QA and full memory/CPU/energy budgets remain open.

## Faster playback, provider Home, and desktop-clear glass milestone

### Verified

- Isolated the single-file extractor startup cost, compared supported client profiles, and checked usable partial audio responses on three public tracks. The directory-based frozen runtime uses pinned source and upstream hash-locked build requirements; no user-installed Python is needed.
- The fast resolver keeps the ordinary client as fallback, probes chosen URLs, honors per-source availability, and caches at most eight private resolutions with expiry checks. New regression tests cover selected-format readiness and expired replay candidates.
- Against the final `.app` runtime with a system-only PATH, search-result selection reached decoded output in **4.90 seconds**, cached repeat in **0.89 seconds**, and the separate Opus playback regression in **3.39 seconds**. Pause/seek, paused AAC quality switching, resume/stop, cancellation, and native volume readbacks pass. Audio checks were muted.
- Live `FEmusic_home` returned genuine provider shelves. Recommended Mellow Pop Classics opened **49 tracks**. Other observed shelf titles/order changed with provider responses, including regional content and All hits; no fixed artist or mood search populates Home.
- Ten WebKit contract tests pass, including provider shelf/collection dispatch without a fixed artist search and version-2 glass migration while retaining layout/accent choices. Previous volume, artwork, collection, keyboard, and narrow-layout tests remain green.
- Fourteen ordinary Rust unit tests plus the non-network integration check pass; live catalog/search/collection tests, both legacy playback/cancellation checks, and explicit native volume checks pass.
- Formatting and Clippy with warnings denied pass for default and `native-diagnostics` builds. The production frontend and normal macOS app bundle build pass.
- A live AppKit/WKWebView diagnostic run exited cleanly and passed `scripts/check-native-material.mjs`: clear native background alpha 0, non-opaque window and real WebView, clear native glass style 1, mounted frontend/player, 18% tint, fully transparent main stage, three live provider shelves, no alert.
- The final normal app launched for eight seconds with a system-only PATH and quit normally via its own NSRunningApplication (`exit 0`). Installed bundle: **197.70 MiB**. Local development ZIP: about **97 MiB**. No generated runtime or archive is committed.

### Interpretation

These are real native/provider checks plus explicitly mocked WebKit UI checks, not a broad p95, acoustic-quality, compositor-energy, clean-machine, or minimum-OS certification. A foreground desktop pixel comparison was not reliable in this execution environment; the actual AppKit/WebKit material contract was verified instead. See startup-and-materials.md for methods, first-use/fallback limits, and reproduction commands. Full resource budgets and OS-interaction QA remain open.

## Repository state

Local Git uses branch `main`. The private repository https://github.com/YuriiPtyts/vessel-app is connected as `origin`; private visibility has been verified. The owner now requests an automatic focused commit and push after each completed, verified change. Native binaries, download caches, credentials, and generated build/test artifacts are excluded from version control. Never change repository visibility without the owner's explicit instruction.
