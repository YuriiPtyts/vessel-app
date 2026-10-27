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

## Radio queue and optional Google connection milestone

### Verified

- Real YouTube Music radio returned **50 initial tracks** and **49 new continuation tracks**. Radio requests are separate from the latest-wins catalog/browse lane.
- The real Rust application, without a renderer, generated a 50-track queue and advanced once at native EOF. Against the final `.app` runtime with `PATH=/usr/bin:/bin`, next-source preparation took **1.63 seconds** and EOF-to-next-decoded-output took **357 ms**. An earlier development run observed **353 ms**. Stop left playback idle and retained the queue. CoreAudio output was muted; this does not establish gapless or acoustic handoff behavior.
- Final packaged-runtime regressions passed: search selection to decoded output **4.24 seconds**, cached repeat **0.89 seconds**, separate guest Opus playback **3.33 seconds**; pause/seek, paused AAC switching, resume/stop, and resolver cancellation remain green. These are individual host/network observations, not p95 guarantees.
- Guest Home returned real shelves and a recommended RELEASED collection containing **67 tracks** in the final run. Search returned 20 songs, continued to 38 unique results, and returned 20 music-video results.
- **27 ordinary library unit tests**, one native-binary IPC-origin unit test, and one non-network application integration check pass. Coverage includes exactly-once EOF, stopped/disabled-radio behavior, late-account generations, bounded collection order, stale rendered queue-entry identity, queue round-trip/corruption preservation, secret-free public state, stale restore/expiry rejection, and cancellation during a simulated OS vault write with rollback.
- **13 WebKit interface-contract tests** pass. New checks cover Rust-driven queue/selection updates, source-quality refresh, stale queue snapshots, next/previous/remove/clear/autoplay controls, connect/cancel/expiry/sign-out, local likes surviving sign-out, and narrow account/queue layouts. Account/IPC responses in these checks are mocked.
- A real native Google-window smoke run passed `scripts/check-native-sign-in.mjs`: private/nonpersistent WKWebsiteDataStore, Google HTTPS origin, Rust cookie API access, cancel/close cleanup, original account revision, and local collection preservation. No account credentials were entered or saved by this check.
- Native material diagnostics still passed: clear NSWindow background, non-opaque WebView, native clear glass, 18% tint, mounted frontend/player, and three live provider shelves. Native libmpv volume readback passed.
- Svelte diagnostics, production frontend, Rust fmt, Clippy with warnings denied (default and `native-diagnostics`), ordinary tests, and normal `npm run tauri -- build --bundles app` pass. Final installed bundle: **198.61 MiB**. The final app ran for eight seconds with system-only PATH and quit normally (`exit 0`); captured output consisted only of macOS notices for the test's OS-persistence flags.

### Account interpretation

The isolated native sign-in window, Rust-only session/vault implementation, account request context, and frontend account controls are implemented. **Real Google password/2FA acceptance, authenticated account-menu verification, a real session's Keychain save/restore/delete, personalized recommendations, and account-required/Premium playback have not been live-verified.** Google can refuse embedded sign-in. A page load or arbitrary cookies never mark the account Connected. See radio-and-accounts.md for the owner flow and reproduction commands.

Local likes/history are still local, not synchronized account library data. Automatic stream renewal/recovery, gapless, media keys, full resource/energy budgets, broader account/region matrices, minimum-OS testing, and distribution gates remain open.

## Readable tint, volume ordering, and Rust direct-source follow-up (2026-10-27)

### Verified

- **16 WebKit interface-contract tests** pass, with mocked native IPC. Added checks cover 50–100% tint, saved low-value clamping, fully opaque CSS/native-material request at 100%, delayed volume echoes after release, and mute/button/`M` restoration. Existing account, queue, saved music, artwork, and narrow-layout checks remain green.
- **29 ordinary library unit tests**, the native-binary IPC-origin test, and the non-network application integration test pass. Rust fmt, Clippy with warnings denied (default and `native-diagnostics`), Svelte diagnostics, and the production frontend build pass.
- The first-selection timing test went red at **3.503 seconds** before the Rust direct guest source path, versus **0.630**, **0.657**, and **0.778 seconds** after it with explicit warm-up. The latest resolution/load acknowledgement was **147 ms**. These are muted decoded-output observations, not an acoustic or cold-install/p95 guarantee.
- Independent live checks passed: Opus decoding, advancing position, stable pause, seek, paused AAC quality change, resume/stop, search-to-play, Home/collection browsing, and catalog pagination. A separate unwarmed catalog selection took **2.262 seconds** with a **0.870-second** cached repeat.
- Both retained extractor profiles independently returned five usable guest audio formats: **2.779 seconds** for `visionos`, **7.750 seconds** for default. Direct request/probe work shares a two-second budget before fallback. Account revisions, cancellation, private cache/URLs, and provider availability handling remain in force. Cancellation now starts the request with one poll rather than racing a fixed sleep against the faster resolver.
- Rust-owned radio generated **50 tracks**, prepared the next source in **203 ms**, and advanced from natural EOF to the next decoded source in **359 ms**, without a renderer.
- The native 100-change volume test went red before event suppression (100 full playback events). The latest pass measured **129 µs p95**, **172 µs max**, **zero full-state events** during a **6.37 ms** sweep of local silent audio. This measures command acknowledgements, not acoustic gain latency. Playback refresh/EOF polling remains active; stop and radio advancement passed separately.
- Native AppKit/WKWebView diagnostics passed with a clear NSWindow, non-opaque WebView, native clear glass, **50% tint**, mounted production player, and **three live provider shelves**. The 100% material toggle is covered by the mocked IPC contract; this native diagnostic run exercised 50% glass.
- A real muted native test passed **20 aggregate-device selection/removal cycles**, verified restoration of the original default output, and checked continued position advancement within three seconds (observed **326 ms** after the final settling check). Its actual backend was **AVFoundation/S16**, not the CoreAudio output driver. An initial 200 ms assertion was too short; `node scripts/probe-audio-device.mjs` bounds the native test/teardown process group.
- Normal `npm run tauri -- build --bundles app` passed; the installed development app is **198.65 MiB**. The normal bundle ran for eight seconds with `PATH=/usr/bin:/bin` and quit via its own NSRunningApplication (`exit 0`, 301 captured output bytes). This establishes launch/quit, not native UI automation.
- Final `.app` runtime checks with a system-only PATH passed: warmed first-selection decoded output **664 ms** (load acknowledgement **119 ms**); separate guest Opus flow **2.263 seconds** including inspection and the test's position threshold; paused AAC switching, resume/stop, and cancellation; radio next-source preparation **205 ms**, natural EOF-to-next-decoded-source **356 ms**. Packaged-runtime volume sweep: **61 µs p95**, **167 µs max**, **zero full-state events** in **4.44 ms**. These muted tests do not measure audible output or physical-device correctness.

### Unresolved headphone crash

The original report contains a null-address SIGSEGV in libmpv during CoreAudio device-change notification. Synthetic device/samplerate stress has **not reproduced the physical headphone crash**. A float-format workaround did not keep playback on CoreAudio and was removed. This follow-up does not claim a headphone-crash fix. The connection type/model and a repeatable hardware reproducer are still needed; see [audio-device-investigation.md](audio-device-investigation.md) for evidence, rejected experiments, upstream references, and a next-session procedure.

Google password/2FA, real-account Keychain operations, authenticated/Premium playback, broader hardware/region coverage, and long-session CPU/energy behavior retain the earlier verification limits.

## Remembered Google account follow-up (2026-10-27)

The owner requested keeping their account signed in across restarts. The original isolated sign-in window was ephemeral. A fixture cookie disappeared on close/reopen; a persistent-store flag alone also failed repeated browser-only process-restart checks. Remembering now uses **Keychain as the durable source**, including separately scoped Google browser cookies restored before the sign-in window navigates, alongside the already implemented YouTube provider-session restore. A remembered profile remains Checking until server verification. Explicit sign-out removes the complete stored session and awaits deletion of the isolated browser store.

### Verification

- Rust restart tests preserve the session/profile, keep authenticated requests disabled until verification, preserve legacy records without the new browser-cookie field, retain still-valid Google browser cookies when YouTube cookies expire, and keep Google-domain cookies out of provider headers and extractor files. Existing cancelled-write rollback and stale-attempt tests remain applicable.
- `node scripts/check-native-remember.mjs <native-diagnostics executable>` passes native close/reopen and **two-process macOS Keychain recovery**. Browser data is deliberately erased before recovery, so this is not a test that accidentally reuses WebKit's memory cache. The test uses fresh fixture-only Keychain/browser namespaces, restores native cookie data, checks main-window isolation, and verifies explicit forgetting. It neither reads the production vault entry nor logs cookie values. Real account verification is not simulated as Connected.
- The real Google-window smoke test passes supported persistence mode, Rust cookie API access, cancel/forget cleanup, and fixture account/library preservation. No account credentials are entered. These diagnostic modes now use memory-only application services and skip playback warm-up.
- Final checks pass: **33 ordinary library tests**, the binary IPC-origin test, the non-network application check, **16 mocked WebKit UI tests**, Svelte diagnostics, production frontend build, Rust fmt, and Clippy with warnings denied (default and `native-diagnostics`). The remembered-browser and Google-window checks also passed from the diagnostic `.app` bundle.
- The normal release `.app` packages successfully at **198.72 MiB**, runs for eight seconds with `PATH=/usr/bin:/bin`, and quits via its own NSRunningApplication (`exit 0`, 301 captured output bytes). Final packaged-runtime regressions passed guest Opus decoding, advancing playback, pause/seek, paused AAC switching, resume/stop, and cancellation; the separate guest flow observed **4.184 seconds** to its decoded/position threshold including **919 ms** inspection on that run. This is one network/host observation, not a latency guarantee.

Real Google password/2FA and a real account's server-side acceptance after restart still need owner verification. These account changes do not claim to fix the reported audible volume lag or matching libmpv/CoreAudio SIGSEGV; current evidence and limitations are in `audio-device-investigation.md`.

## Genie reveals and CoreAudio initialization cleanup (2026-10-27)

### Interface

- Added a shared macOS-inspired genie reveal to appearance, account, queue, audio-quality, and now-playing surfaces. Curved clipping/perspective unfurls each newly mounted surface from its anchor; routine playback updates do not repeat it. Closing is immediate.
- **18 mocked WebKit interface-contract tests** pass. New checks inspect real Web Animations keyframes, curved mid-reveal clipping, the settled layout and immediate close, and instant unwarped content with OS reduced motion enabled. A paused fixture screenshot was visually reviewed. This is not native compositor/power measurement.
- Svelte diagnostics report zero errors/warnings; the production frontend builds (approximately **117 kB JS**, **59 kB CSS**, uncompressed in this run).

### Targeted audio fix

- A controlled native initialization-failure audit went red with the previous engine: CoreAudio returned the injected parameter error, playback fell back to AVFoundation, and **two device callbacks remained registered** for the failed driver context. This exercises the resource-lifetime pattern at the captured crash callback rather than merely asserting a backend name.
- Rebuilt the LGPL engine as **0.41.0-vessel.1** with upstream #18383's cleanup backport and the v0.41.0 AudioUnit-pointer adaptation. The same audit passes with **zero** callbacks. All mpv/FFmpeg/MPVKit source archives, 17 static-dependency ZIP bundles, and the Meson build wheel are SHA-256 pinned; only system dynamic dependencies are accepted. Reuse validates the cached compiled file and recipe hash. No binary/cache is committed.
- The deterministic audit passes with both the prepared runtime and the final `.app` runtime. A real muted CoreAudio/S16 stereo run passed **20/20** aggregate-device selection/removal cycles, verified original-output restoration, and observed advancing output after **116 ms** of the final recovery check.
- Real guest Opus decoding, stable pause, seek, paused AAC quality change, resume/stop, and cancellation pass. Radio generated **50 tracks**, prepared the next source in **305 ms**, and advanced at natural EOF to decoded next-source output in **356 ms**. The warmed first-selection test observed **736 ms** in the prepared runtime and **728 ms** in the packaged runtime (packaged acknowledgement **121 ms**).
- Final packaged guest regression observed **2.459 seconds** to its decoded/position threshold, including **567 ms** inspection. Native volume readback/sweep passed (**59 µs p95**, **85 µs max**, zero full-state events during 100 writes); those are acknowledgements, not proof that the previously reported audible volume delay is resolved.
- Rust fmt, Clippy with warnings denied (default and diagnostic features), **33 ordinary library tests**, the binary IPC-origin test, and the non-network application check pass.
- Normal macOS bundle generation passes. The installed app is **166.40 MiB**, versus 198.72 MiB before the arm64 engine rebuild. It launched for eight seconds with system-only PATH and quit normally via its own NSRunningApplication (`exit 0`, 301 captured output bytes).

**Remaining hardware gate:** the owner reports physical AirPods output-switch crashes in addition to the analog 3.5 mm path. The matching listener-lifetime defect is fixed and covered by the controlled audit; a physical AirPods transition was not automated or certified. Restart into the rebuilt app and repeat that transition. Keep any distinct later crash signature separate. See `audio-device-investigation.md` for the evidence and `native-dependencies.md` for source/minimum-OS/distribution limits.

## Beta candidate verification (2026-10-27)

- Prepared version **0.1.0-beta.1** consistently across npm, Cargo, and Tauri metadata. The normal `npm run tauri -- build --bundles app` build passes; the installed Apple Silicon app is **166.40 MiB**. The ZIP is **80,887,565 bytes** (approximately **77.14 MiB**), passes archive integrity checks, and has a verified `SHA256SUMS` sidecar. Generated assets remain under ignored `.cache/releases/v0.1.0-beta.1/`.
- Svelte diagnostics report zero errors/warnings, the production frontend builds, and all **18 mocked WebKit UI contract tests** pass. Prettier passes for the changed frontend, manifests, and release notes. Rust fmt, Clippy with warnings denied (default and `native-diagnostics`), **33 ordinary library tests**, the binary IPC-origin test, and the non-network application integration check pass for the beta version.
- Against the actual beta `.app` runtime, the controlled initialization-failure audit leaves **zero** leaked CoreAudio callbacks. A real muted CoreAudio/S16 run completes **20/20** aggregate-device selection/removal cycles, verifies restoration of the original output, and observes continued advancement after **52 ms** in the final recovery check. These are not physical AirPods or analog-headphone acceptance checks.
- With the beta `.app` runtime and `PATH=/usr/bin:/bin`, native volume property readback and a 100-write sweep pass (**40 µs p95**, **54 µs max**, zero full playback events). This measures acknowledgements, not audible gain latency.
- The same system-only-PATH run passes real guest Opus decoding, pause/seek, paused AAC quality switching, resume/stop, and cancellation. The guest flow observes **2.368 seconds** to its decoded/position threshold, including **588 ms** inspection. Catalog selection observes **2.417 seconds**, with a **1.377-second** cached repeat. Live Home/collection browsing and search pagination pass. Native radio creates **50 tracks**, prepares the next source in **204 ms**, and advances from natural EOF to next decoded output in **356 ms**. These are individual muted host/network observations, not latency guarantees.
- The beta-version diagnostic executable passes isolated native browser close/reopen, two-process fixture Keychain recovery after browser-data deletion, and explicit forgetting. Real Google authentication is not exercised.
- The normal packaged beta app runs for eight seconds with a system-only PATH and quits through its own NSRunningApplication (`exit 0`, 301 captured output bytes). Its executable and libmpv are arm64, and the engine links only system dynamic dependencies.
- **Publication remains blocked on the owner's original physical-audio checks:** no-crash/recovery behavior for actual AirPods and 3.5 mm transitions, and prompt audible volume changes. Prepare a private draft prerelease with the verified download; do not publish it or describe those symptoms as resolved until acceptance. See [beta notes](beta-release.md).

## Download signing repair (2026-10-27)

- Downloaded the original beta ZIP from GitHub rather than relying only on the local build. Both `codesign --verify --deep --strict` and `spctl --assess --type execute` failed with **“code has no resources but signature indicates they must be present.”** The executable had a linker-only ad-hoc signature, no bound Info.plist, and no resource seal.
- The new archive checker fails on that original download. An outer-app-only signing experiment makes `codesign --deep` pass, but explicit verification still rejects the frozen Python framework. Tauri had flattened its relative aliases; the packager restores the prepared runtime's symlinks, signs nested Mach-O files/frameworks inside out, and finally seals the app in an OS-temporary staging directory without Finder/resource-fork metadata. Source runtime/cache files are preserved.
- `npm run package:macos` passes the normal macOS app build and produces a ZIP that passes **107 explicit code-signature checks** and resource-seal verification after normal extraction. The new archive is **75,357,276 bytes**; archive integrity and `SHA256SUMS` pass. `SIGNING.json` records the SHA-256, verification count, and actual trust status. Outputs remain ignored under `.cache/releases/v0.1.0-beta.1/`.
- The extracted runtime works with `PATH=/usr/bin:/bin`: the frozen extractor reports version **2026.10.21**, Deno evaluates the JavaScript computation, native volume property/sweep checks pass, and real muted guest Opus decoding, pause/seek, paused AAC switching, resume/stop, and cancellation pass. Guest inspection observes **537 ms**, and the decoded/position threshold **2.351 seconds**; the volume acknowledgement sweep observes **33 µs p95**, **60 µs max**, and zero full playback events. These remain individual muted observations, not acoustic latency measurements.
- The extracted signed app runs for eight seconds and quits normally (`exit 0`, 301 captured bytes). An initial test launch through the `/var` symlink failed Tauri's protected executable-path lookup; launching through the canonical `/private/var` path passes. No application path protections were disabled.
- Svelte diagnostics, **18 mocked WebKit checks**, production frontend build, Rust formatting, default Clippy with warnings denied, **33 library tests**, the IPC-origin test, the non-network application check, and formatting of the changed scripts/docs pass.
- **Warning-free distribution is still blocked:** Keychain reports **zero valid signing identities**. The repaired ad-hoc app passes signature-integrity checks but **Gatekeeper rejects it**. The stronger distribution check rejects the missing notarization ticket, and distribution packaging rejects missing credentials before altering outputs. No Gatekeeper/quarantine policy was disabled or bypassed. A real Developer ID/notarization run remains unverified until the required identity and Keychain profile are available; see [macOS distribution](macos-distribution.md).
- GitHub already reported `v0.1.0-beta.1` as a published prerelease when this follow-up started. This correction repacks the same beta without moving its existing tag; the signing report explicitly records the missing Apple approval.

## DMG drag-and-drop installer (2026-10-27)

- `npm run package:macos` now produces both the ZIP and **`Vessel-0.1.0-beta.1-macos-arm64.dmg`**. The compressed read-only HFS+ image contains `Vessel.app` and an `Applications` symlink targeting `/Applications` for drag-and-drop installation.
- A real `hdiutil verify` and read-only attach pass. The verifier checks the actual shortcut target and **107 code signatures** plus the mounted app's resource seal, confirms the ZIP/DMG code inventories match, and detaches the image. This verifies the actual disk image, not a mocked installer interface or a manual Finder drag.
- The DMG is **76,452,806 bytes**; the ZIP remains **75,357,276 bytes**. `SHA256SUMS` checks both files successfully, and `SIGNING.json` records both hashes, mounted-image verification, and the shortcut target. All outputs remain ignored build artifacts.
- Svelte diagnostics, **18 mocked WebKit UI checks**, the production frontend build, normal macOS app packaging, Rust fmt, default Clippy with warnings denied, **33 library tests**, the IPC-origin test, the non-network application check, and formatting of the changed scripts/docs pass.
- The development DMG contains the same ad-hoc-signed app and is an unsigned container; changing the download format does not provide Apple trust. The credential-backed distribution path now also signs, notarizes, staples, and assesses the DMG, but a real Apple-backed run still requires the previously documented certificate and Keychain profile.

## Output gain, native media controls, and process visibility (beta.2)

- The real Rust-player AVFoundation fixture went red with **1.804–1.848 seconds** of delayed gain changes, although native acknowledgements were already fast. The matching CoreAudio fixture observed **7–12 ms**. This isolates a queued-software-gain issue rather than merely retesting slider state or command acknowledgement.
- Rebuilt pinned **libmpv 0.41.0-vessel.2** with per-handle output gain; source hashes cover the new patch as well as the retained CoreAudio initialization cleanup. The Rust player keeps software gain at unity, uses the existing cubic loudness curve at the per-player output, and applies the retained gain before every recreated output starts. See [volume response](volume-response.md) for the seam and exact interpretation.
- Against the **final beta.2 `.app` runtime**, the same four-transition probe passes its 50 ms bound: CoreAudio **4.0–11.4 ms**, AVFoundation **0.047–2.369 ms**. Forced output recreation at 0%, 27%, 73%, and 0% preserves the first output gain on both backends. CoreAudio observations combine actual callback PCM with native unit gain; AVFoundation observations use actual queued PCM/timestamps, render clock, and live renderer gain. Physical output is silenced. These are not acoustic/Bluetooth latency measurements.
- The packaged-runtime CoreAudio initialization-failure audit still reports **zero** leaked callbacks. With a system-only PATH, native target/curve checks and 100-write bounds pass (**39 µs p95**, **622 µs max**, zero full playback events). Real muted Opus decoding, advancing playback, pause/seek, paused AAC switching, resume/stop, and cancellation pass. Guest inspection observes **600 ms**, and the decoded/position threshold **2.471 seconds**. Radio creates **50 tracks**, prepares the next source in **306 ms**, and advances from EOF to decoded next-source output in **357 ms**. These are individual observations, not p95 or acoustic guarantees.
- Added native macOS MediaPlayer previous/next, play, pause, and toggle commands connected to Rust queue/transport. Now Playing receives title/artist/album, position, duration, rate, and playback state. Metadata updates are skipped when state is unchanged and the native clock can extrapolate position. Idle/quit clears ownership and disables/removes handlers.
- `node scripts/check-native-media.mjs src-tauri/target/debug/vessel-app` passes in a **beta.2 native-diagnostics build**: five real native command registrations, the registered handler routes, native paused state, disabled idle commands, and cleanup. The fixture uses memory-only services and skips provider/account warm-up. It does not synthesize physical keyboard events or certify OS focus/routing on the owner's keyboard.
- The normal installed executable is now **`Vessel`**. A direct `ps` check against the extracted packaged app confirms that process name, so Neohtop can search for it. Older builds and direct Cargo development runs remain `vessel-app`. WebKit helpers may be separate processes.
- Brief main-process-only idle sampling after eight seconds of startup: five samples over **4.04 seconds**, RSS **130.42–133.48 MiB**, and **0.00 seconds of additional CPU time** at `ps` precision. The normal app then quits cleanly (`exit 0`, 293 captured output bytes). This excludes WebKit/helper resources and is not a full-app, playing, energy, or long-session budget.
- Svelte diagnostics, **18 mocked WebKit checks**, Rust fmt, Clippy with warnings denied (default and diagnostics), **33 ordinary library tests**, the IPC-origin unit test, and the non-network integration check pass. A clean `npm ci` repaired transient invalid formatter package reads before the final checks.
- Normal `npm run package:macos` builds the production frontend and native app and verifies **107 code signatures** in both ZIP extraction and the mounted drag-and-drop DMG. The app remains ad-hoc signed and the DMG unsigned; the documented Developer ID/notarization requirement and Open Anyway installation instructions still apply. Physical AirPods/headphone crash recovery remains an owner hardware check.

## Repository state

Local Git uses branch `main`. The private repository https://github.com/YuriiPtyts/vessel-app is connected as `origin`; private visibility has been verified. The owner now requests an automatic focused commit and push after each completed, verified change. Native binaries, download caches, credentials, and generated build/test artifacts are excluded from version control. Never change repository visibility without the owner's explicit instruction.
