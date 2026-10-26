# Vessel

A macOS-first, lightweight YouTube Music desktop client. Private, proprietary project; all rights reserved. Vessel is the working name inherited from this workspace.

## Status

Search-first music app on Apple Silicon macOS, using Tauri 2, Rust, and Svelte 5:

- Search YouTube Music by song or artist name, with Songs/Music videos filters and paginated results.
- Click a song or discovery card to play directly; resolution and format discovery happen behind the interface.
- Browse YouTube Music's guest Home recommendations and open recommended playlist/album track lists; save songs locally and return to recently played music.
- Use a persistent floating player; select available source quality from its compact popover.
- Play through Rust-managed libmpv/CoreAudio; pause, resume, seek, stop, and adjust volume.
- Change quality while preserving position and pause state.
- Cancel in-flight resolution; receive actionable provider/stream failures without exposing signed URLs.

The macOS shell uses native traffic lights, an overlay titlebar, clear Liquid Glass on macOS 26+, a vibrancy fallback on older systems, and reduced-transparency mode. Account support, full album/artist browsers, persistent queues, automatic recovery, and OS media controls are still pending. Likes/history are local to this Mac; Home currently uses the guest provider feed rather than a signed-in account. Recorded public tracks only; lossless requires a verified source.

## Listening

Search in the top bar (or press Command-K), then click a result to play. The heart saves a song to **Liked songs**. **Recently played** records tracks after native playback starts. Player arrows move through the result set you started from; this is not yet a persistent queue or automatic radio.

Volume updates live while dragging, with bounded/coalesced native writes. Click the player's cover for a larger now-playing picture; on wide windows the artwork panel is shown by default. The app uses provider artwork and falls back to the original thumbnail when a higher-resolution image is unavailable.

## Appearance

The **Duskfox** palette, derived from the owner's Zed theme, remains the default, now with a clear-glass material and 18% default tint. Open **Appearance** to change the accent, glass tint, transparency, song-list density, sidebar layout, and artwork panel. Choices persist across restarts and can be reset. See [appearance and the planned theme library](docs/themes.md).

## Development

Requires Node.js, Rust, Apple's developer tools, and Python 3.14 for the first frozen-extractor build on macOS. Set `VESSEL_BUILD_PYTHON` if that interpreter is not `python3`. These are development requirements; end users do not need Python or other tools installed.

```sh
npm ci
npm run tauri dev
```

The macOS arm64 runtime is prepared automatically. It downloads checksum-verified libmpv/Deno artifacts and yt-dlp source, then makes a cached directory-based frozen extractor using the upstream build recipe and hash-pinned requirements. This avoids unpacking a Python runtime on each song. No system packages are installed. See [dependency provenance and remaining release obligations](docs/native-dependencies.md).

Build and open the local app:

```sh
npm run tauri -- build --bundles app
open src-tauri/target/release/bundle/macos/Vessel.app
```

Browser-only visual preview (search/playback require the desktop app):

```sh
npm run dev
```

Checks:

```sh
npm run check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Real guest playback integration checks (network-dependent; CoreAudio output is muted):

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test live_playback -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --test live_catalog -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --lib native_volume_values_are_applied_immediately -- --ignored --nocapture
```

WebKit presentation-contract checks (mocked IPC, separate from real playback):

```sh
npx playwright install webkit
npm run test:ui
```

## Product direction

- macOS first; Windows and Linux later.
- Guest listening is a core requirement. Account support is an early priority, not a prerequisite for listening.
- Audio-only libmpv playback, validated for the first guest slice; broader reliability, account behavior, and distribution remain release gates.
- User-selectable quality based on formats actually available to the current source and account.
- Lossless only if a genuinely lossless source becomes available and can be verified.
- Self-contained direct downloads; no user-installed playback or extraction tools.
- Low resource use and reliable listening first, with premium native materials and context-preserving queues.

See [confirmed decisions](docs/decisions.md), [architecture](docs/architecture.md), [feasibility plan](docs/feasibility.md), and [opportunity backlog](docs/opportunities.md).

The [verification log](docs/verification.md) and [startup/native-material investigation](docs/startup-and-materials.md) record measurements and limits. The tested fast path reached decoded output in about 3.4 seconds with a warm runtime; a cached repeat took about 0.9 seconds. First-use platform loading and fallback extraction can take longer. These are observations, not p95 guarantees.
