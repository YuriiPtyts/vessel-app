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

## Repository state

Local Git uses branch `main`. The private repository https://github.com/YuriiPtyts/vessel-app is connected as `origin`; private visibility has been verified. Native binaries, download caches, credentials, and generated build/test artifacts are excluded from version control. Never change repository visibility without the owner's explicit instruction.
