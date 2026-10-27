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
- Keep listening with YouTube Music radio, native automatic next-song playback, one-track source prefetch, and an editable saved queue.
- Open an optional private Google sign-in window; verified sessions are managed in Rust and stored in macOS Keychain.

The macOS shell uses native traffic lights, an overlay titlebar, clear Liquid Glass on macOS 26+, a vibrancy fallback on older systems, and reduced-transparency mode. Full album/artist browsers, automatic stream recovery, and OS media controls remain pending. Likes/history and the queue are local to this Mac. Radio is live-verified as a guest; the Google window/isolation/cancellation are native-verified, but a real account's login, personalized feed, and account-required playback still need owner verification. Google may decline embedded sign-in. Recorded tracks only; lossless requires a verified source.

## Listening

Search in the top bar (or press Command-K), then click a result to play. The heart saves a song to **Liked songs**. **Recently played** records tracks after native playback starts. Search and discovery selections start provider radio; collections and local lists retain their loaded order, with radio added after them. Player arrows use the Rust-owned queue, including when the renderer is hidden.

Open **Up next** using the player's queue icon to choose/remove songs, clear the queue, or turn off **Autoplay radio**. Turning radio off still lets already-queued songs finish. Up to 100 tracks and the radio preference survive a restart; playback resumes only when requested. Previous restarts the current song after three seconds, otherwise returning to the previous item.

Use the top-right avatar or sidebar session label for **Account**. **Connect Google** opens Google's own page in an isolated native window. Remembered sign-in data is restored from macOS Keychain and must be verified before authenticated requests resume. Closing/cancelling the window leaves guest listening available. Sign-out removes Vessel's saved Keychain session and isolated browser data, and stops playback while retaining local likes, history, queue, and appearance. See [radio and account behavior, verification, and remaining account QA](docs/radio-and-accounts.md).

Volume updates live while dragging, with bounded/coalesced native writes. Click the player's cover for a larger now-playing picture; on wide windows the artwork panel is shown by default. The app uses provider artwork and falls back to the original thumbnail when a higher-resolution image is unavailable.

## Appearance

The **Duskfox** palette, derived from the owner's Zed theme, remains the default, with a clear-glass material and 50% default tint. Open **Appearance** to change the accent, glass tint (50–100%), transparency, song-list density, sidebar layout, and artwork panel. Choices persist across restarts and can be reset. Transient panels use a short genie-style reveal that respects the system's reduced-motion preference. See [appearance and the planned theme library](docs/themes.md) and [motion behavior](docs/motion.md).

## Beta build

The first Apple Silicon development beta is **0.1.0-beta.1**. See [beta notes and the remaining physical-audio checks](docs/beta-release.md). Download packaging now verifies the complete app and nested code signatures after unzipping. The development build is ad-hoc signed; warning-free downloads require Developer ID signing and Apple notarization. See [macOS distribution](docs/macos-distribution.md). Automated command timings alone do not establish audible volume response or physical AirPods/headphone recovery.

## Development

Requires Node.js, Rust, Apple's developer tools, and Python 3.14 for the first frozen-extractor build on macOS. Set `VESSEL_BUILD_PYTHON` if that interpreter is not `python3`. These are development requirements; end users do not need Python or other tools installed.

```sh
npm ci
npm run tauri dev
```

The macOS arm64 runtime is prepared automatically. It builds the CoreAudio-patched libmpv engine from pinned source/static inputs, downloads checksum-verified Deno, and makes a cached directory-based yt-dlp extractor using the upstream build recipe and hash-pinned requirements. This avoids unpacking a Python runtime on each song. The engine build also requires Xcode, pkg-config, Ninja, and CMake. No system packages are installed. See [dependency provenance and remaining release obligations](docs/native-dependencies.md).

Build and open the local app:

```sh
npm run tauri -- build --bundles app
open src-tauri/target/release/bundle/macos/Vessel.app
```

Create a verified development ZIP and drag-and-drop DMG with `npm run package:macos`. Open the DMG and drag **Vessel** onto its **Applications** shortcut. For downloads intended to pass Gatekeeper, configure the Developer ID/Keychain credentials documented in [macOS distribution](docs/macos-distribution.md) and use `npm run release:macos`. Upload the packager's downloads and sidecars; do not ZIP the raw Tauri bundle directly.

If macOS blocks the beta's first launch, try opening Vessel once, then go to **System Settings → Privacy & Security → Security → Open Anyway** and confirm **Open**. See the [first-launch instructions](docs/beta-release.md#first-launch-if-macos-blocks-vessel).

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
cargo test --manifest-path src-tauri/Cargo.toml --test live_radio -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml live_radio_continuation_returns_new_recommendations -- --ignored --nocapture
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
