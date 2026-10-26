# Architecture direction

## Ownership and boundaries

The Rust application owns playback truth and the local collection. Svelte renders snapshots and sends commands over narrowly scoped Tauri IPC. A guest InnerTube catalog, yt-dlp resolver, and dynamically loaded libmpv adapter are implemented. Queue ownership/persistence and authenticated catalog access remain future work.

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
- `catalog.rs`: bootstraps anonymous WEB_REMIX context from the actual Music homepage; searches songs/music videos directly over bounded HTTPS; normalizes artist/album/artwork/duration; handles provider pagination. One latest-wins request slot, a 60-second result cache, a 15-minute context cache, a 100-result cap, and 128 known track metadata entries bound work/storage. No embedded provider webpage or permanent search daemon.
- `application.rs`: coordinates latest-wins cancellable resolution, opaque sources, one-click search-result playback, and local collection operations. Failed resolution does not interrupt an already playing track. Catalog metadata is retained for the native player's title/artwork.
- `library.rs`: atomic JSON persistence for at most 500 local likes and 40 recent tracks. Native Playing transitions record history once per track, on the control actor rather than the real-time audio callback. Existing malformed data is preserved, with a nonfatal library error. Only public metadata is persisted.
- `playback.rs`: a bounded actor owns transport state and snapshots. The native engine is lazy-loaded and never shared across threads. The idle actor blocks; active updates are capped at four per second.
- `mpv.rs`: the isolated FFI boundary loads a replaceable dynamic library, applies audio-only options, bounded media caches, and native commands/events.
- `runtime.rs`: locates the development or packaged runtime. The installed app uses its own Resources/runtime directory, not PATH/Homebrew.

No queue, next-track prefetch, credentials, automatic renewal, or authentication flow is implemented yet. The libmpv demuxer forward/back byte caps total 24 MiB; this is a configured media-cache limit, not a measured whole-process-memory bound.

Keep current/next buffers bounded. Resolve the next track near playback, not every queue item up front. Cancel obsolete resolutions on queue changes. Stream expiration recovery, network retry, and user skip are distinct state transitions. Use generation/queue-entry identity to reject stale async results.

Do not fetch/decode on the audio callback. Do not introduce a permanent Python/Node server or duplicate media proxy without measured need.

## Native shell

Use Tauri 2 APIs and a native macOS titlebar/traffic lights first. Exercise native semantic vibrancy before adding any custom Objective-C integration. Windows/Linux adapters come later. Opaque materials are a first-class visual mode.

The macOS platform override configures a transparent window, native traffic lights, an overlay titlebar, and `UnderWindowBackground` semantic material through Tauri's built-in v2 API. Version 2.12.5 exposes this without a separate vibrancy plugin; its current schema documents the old macOS-private-api toggle as a no-op since 2.12.1. Reduced transparency removes native effects and the player's CSS backdrop blur, then uses opaque surfaces. Reduced motion is respected. Only the floating player uses CSS backdrop blur; static hero art does not animate continuously.

The frontend is organized into a shell and Artwork, TrackList, and PlayerBar components. Public artwork URLs are restricted to HTTPS Google image hosts and loaded lazily without a referrer. The frontend maintains the transient result-set cursor used by previous/next buttons; it is not a durable queue and does not auto-advance in a hidden renderer. A Rust queue remains a separate milestone.

## Closed-source packaging constraint

libmpv's license depends on its build and linked components. The spike uses the zapper v0.41.0-1 LGPL-3.0-or-later dynamic build, with complete pinned-source materials available upstream. yt-dlp is a separate GPL-licensed standalone process. Record artifact-specific obligations and complete the redistribution materials before publishing downloads; see native-dependencies.md. The preparation script checks SHA-256 digests and rejects libmpv builds referencing non-system dynamic dependencies on arm64.

Sources:

- https://v2.tauri.app/reference/javascript/api/namespacewindow/#effect
- https://mpv.io/manual/stable/#using-mpv-from-other-programs-or-scripts
- https://github.com/mpv-player/mpv/blob/master/Copyright
- https://github.com/yt-dlp/yt-dlp#dependencies
