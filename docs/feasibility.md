# macOS feasibility plan

Documentation research is not runtime evidence. Mark outcomes measured only after running the relevant experiment.

## Slice 0: native foundation

Deliver a runnable Tauri 2 / Svelte 5 diagnostic shell with a Rust command, production frontend build, pinned lockfiles, and an arm64 macOS compilation check. This does not establish playback or account feasibility.

## Slice 1: one real guest track

- Compare yt-dlp resolution with a maintained InnerTube option using a representative public track set.
- Exercise current JavaScript challenge-solving and attestation requirements; do not assume a single standalone extractor is sufficient.
- Identify a redistributable libmpv build compatible with the closed-source distribution model.
- Resolve an actual audio-only stream, propagate required headers, and play through Rust-owned libmpv.
- Confirm no video decoding, playback while hidden, pause, seek, and a useful failure state.
- Record dependency versions, codec/container, time to audible output, peak extraction memory, settled process-tree memory, CPU, and bundle size.
- Package the harness and launch on a machine without development playback tools.

## Slice 2: optional account support

- Validate authentication acquisition, expiry, revocation, and safe credential persistence on macOS.
- Read private playlists and Liked Songs for an explicitly authorized test account.
- Verify correct account/channel identity and paginated results.
- Test sign-out, expired authentication, and guest operation without losing local queue state.
- Compare guest and signed-in format availability. Test a Premium account only if available; do not infer Premium behavior from a guest test.
- No passwords, tokens, or browser-cookie exports belong in the repository or diagnostic output.

## Slice 3: quality and reliability

- Discover actual formats for each source/account combination; preserve unknown bitrate/timing values.
- Implement quality preferences and expose actual selected codec/format separately from the preference.
- Validate Opus/WebM and AAC/MP4 with the exact shipped decoder build.
- Exercise expired URLs, forced 403s, a 30-second network interruption, sleep/wake, unavailable tracks, rapid skips, seeks, and output-device changes.
- Validate transition continuity with known gapless fixtures and real album encodes. Do not substitute crossfade for gapless evidence.
- Perform a two-hour queue run followed by an eight-hour release soak.

## Initial budgets (targets, not measurements)

Measure release builds on named reference hardware; include the attributable WebView/extractor process tree. Avoid double-counting shared memory; record the platform metric and method. Report transient extraction peaks separately.

| Metric | Initial target |
| --- | --- |
| Settled idle / playback memory | <=200 / <=250 MiB |
| Idle background CPU | <=0.5% of one logical core averaged over five minutes |
| Background playback CPU | <=3% of one logical core |
| Cold / warm usable-shell startup | p95 <=1.5 seconds / <=700 ms |
| Resolution plus time to audible playback | p95 <=4 seconds on a defined healthy network |
| Current/next media buffers | <=32 MiB total |
| Download per architecture | <=150 MiB including required dependencies |

Benchmark the same listening workload against a browser/PWA and an existing wrapper before making comparative performance claims.

## Release gates after feasibility

Persistent queue and recovery -> browsing and supported library reads -> accessible glass interface -> macOS Now Playing/media keys/menu bar -> clean-machine signed/notarized self-contained downloads -> measured performance and soak readiness.

## Current outcomes

The first guest resolution/playback/quality slice passes on the host, including playback from bundled dependencies with a system-only PATH. The actual provider timing requirement caused and then fixed an early-opening HTTP 403 failure. See verification.md for commands and measured outcomes.

Guest YouTube Music catalog search and paginated songs/music-video results are now verified. A search result plays through the same native pipeline without a user-facing link or inspection step. The music shell, native-material integration, and local saved/recent collection replace the lab interface. Album/artist detail browsing and account synchronization remain separate work.

Home now uses real guest provider shelves and recommended collection track lists. A directory-based frozen extractor, tested fast client with fallback, bounded source cache, and background warm-up reduced the observed warm-runtime click-to-decode time to about 3.4 seconds and a cached repeat to about 0.9 seconds. Live AppKit/WKWebView diagnostics verify the non-opaque clear-glass window stack and mounted provider feed.

Authentication, broad source/account/region coverage, native material performance, minimum-version support, clean-machine operation, full resource budgets, and public distribution remain **unverified**. Some warm-path observations reach the initial four-second target; the first packaged selection took about 4.9 seconds. A p95 budget and first-ever/fallback startup behavior are not established. Next priorities include optional authentication/library reads, renewal/reconnect, and full startup/resource measurements.
