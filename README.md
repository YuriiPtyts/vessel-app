# Vessel

A macOS-first, lightweight YouTube Music desktop client. Private, proprietary project; all rights reserved. Vessel is the working name inherited from this workspace.

## Status

First real guest-playback slice on Apple Silicon macOS, using Tauri 2, Rust, and Svelte 5:

- Paste a YouTube Music/YouTube track link and inspect its audio-only formats.
- Select the provider-recommended format or a specific available codec/quality.
- Play through Rust-managed libmpv/CoreAudio; pause, resume, seek, stop, and adjust volume.
- Change quality while preserving position and pause state.
- Cancel in-flight resolution; receive actionable provider/stream failures without exposing signed URLs.

Account support, catalog search, persistent queues, automatic recovery, native materials, and OS media controls are still pending. Recorded public tracks only; live streams are outside this slice. Lossless is not offered without a verified lossless source.

## Development

Requires Node.js, Rust, and Apple's developer tools on macOS. These are development requirements, not intended requirements for end users.

```sh
npm ci
npm run tauri dev
```

The macOS arm64 native runtime is prepared automatically for desktop development/builds. The first preparation downloads pinned, checksum-verified libmpv, yt-dlp, and Deno artifacts. It does not install system packages or require those tools on the end user's machine. See [dependency provenance and remaining release obligations](docs/native-dependencies.md).

Build and open the local app:

```sh
npm run tauri -- build --bundles app
open src-tauri/target/release/bundle/macos/Vessel.app
```

Browser-only preview (reports native diagnostics as unavailable):

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

The [verification log](docs/verification.md) records actual playback results, startup latency, local artifact sizes, and remaining release gates. Current first-play latency misses the initial four-second target; it is not hidden by a simulated player or a permanently running extraction service.
