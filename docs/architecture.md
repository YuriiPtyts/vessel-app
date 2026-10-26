# Architecture direction

## Ownership and boundaries

The Rust application owns playback truth. Svelte renders snapshots and sends commands over narrowly scoped Tauri IPC. The first slice implements a guest yt-dlp resolver and a dynamically loaded libmpv adapter. Queue ownership/persistence and authenticated catalog access remain future work.

| Boundary | Owns |
| --- | --- |
| Provider | Catalog/search, browse pagination, capability discovery, library operations, stream resolution |
| Authentication | Guest/account lifecycle, credential storage, authenticated request context |
| Playback | Engine handle, transport, timing, seek, buffer/device events |
| Application/queue | Queue identity/order, shuffle/repeat, persistence, recovery coordination |
| Native | Media sessions, media keys, tray/menu bar, notifications, window lifecycle/materials |
| Frontend | Navigation, presentation, accessibility, transient interaction state |

Start with modules in one Rust crate, not services or an extensible plugin framework. Add abstractions only around volatile provider and engine boundaries.

## Guest and account modes

Use explicit capability results: available, account required, temporarily unavailable, or unsupported. Do not spread a global `loggedIn` condition through playback and presentation. Private library surfaces can request sign-in; guest search and available guest playback remain independent.

Do not assume Google OAuth or an embedded Google login provides the needed YouTube Music capabilities. Validate the actual flow before choosing dependencies. Keep credentials in platform credential storage behind Rust. Do not expose credentials, cookie exports, or signed stream URLs through frontend state or logs.

## Media path

Implemented path: Rust resolver -> short-lived URL plus required request context -> libmpv -> CoreAudio. Video and engine-side automatic extraction are disabled. Rust retains signed URLs and headers; public IPC DTOs contain metadata and available-format identifiers only.

- `provider.rs`: validates/canonicalizes track links, runs a bounded/time-limited bundled resolver, classifies failures, and discovers audio-only formats. It honors each format's `available_at` timestamp before exposing playable choices.
- `application.rs`: coordinates one cancellable inspection, opaque resolved sources, and playback commands. Failed inspection does not interrupt an already playing track.
- `playback.rs`: a bounded actor owns transport state and snapshots. The native engine is lazy-loaded and never shared across threads. The idle actor blocks; active updates are capped at four per second.
- `mpv.rs`: the isolated FFI boundary loads a replaceable dynamic library, applies audio-only options, bounded media caches, and native commands/events.
- `runtime.rs`: locates the development or packaged runtime. The installed app uses its own Resources/runtime directory, not PATH/Homebrew.

No queue, next-track prefetch, credentials, automatic renewal, or authentication flow is implemented yet. The libmpv demuxer forward/back byte caps total 24 MiB; this is a configured media-cache limit, not a measured whole-process-memory bound.

Keep current/next buffers bounded. Resolve the next track near playback, not every queue item up front. Cancel obsolete resolutions on queue changes. Stream expiration recovery, network retry, and user skip are distinct state transitions. Use generation/queue-entry identity to reject stale async results.

Do not fetch/decode on the audio callback. Do not introduce a permanent Python/Node server or duplicate media proxy without measured need.

## Native shell

Use Tauri 2 APIs and a native macOS titlebar/traffic lights first. Exercise native semantic vibrancy before adding any custom Objective-C integration. Windows/Linux adapters come later. Opaque materials are a first-class visual mode.

The initial shell intentionally uses an opaque background and native window decoration. Material effects must earn a place through compatibility and resource measurements.

## Closed-source packaging constraint

libmpv's license depends on its build and linked components. The spike uses the zapper v0.41.0-1 LGPL-3.0-or-later dynamic build, with complete pinned-source materials available upstream. yt-dlp is a separate GPL-licensed standalone process. Record artifact-specific obligations and complete the redistribution materials before publishing downloads; see native-dependencies.md. The preparation script checks SHA-256 digests and rejects libmpv builds referencing non-system dynamic dependencies on arm64.

Sources:

- https://v2.tauri.app/reference/javascript/api/namespacewindow/#effect
- https://mpv.io/manual/stable/#using-mpv-from-other-programs-or-scripts
- https://github.com/mpv-player/mpv/blob/master/Copyright
- https://github.com/yt-dlp/yt-dlp#dependencies
