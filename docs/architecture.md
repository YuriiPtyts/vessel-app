# Architecture direction

## Ownership and boundaries

The Rust application owns playback, the queue/radio, the account lifecycle, and the local collection. Svelte renders snapshots and sends commands over narrowly scoped Tauri IPC. The InnerTube catalog, yt-dlp resolver, dynamically loaded libmpv adapter, optional private Google window, and Keychain session storage are implemented. Guest radio/playback and the native sign-in window contract are verified; real authenticated-account behavior still needs owner verification.

| Boundary          | Owns                                                                                           |
| ----------------- | ---------------------------------------------------------------------------------------------- |
| Provider          | Catalog/search, browse pagination, capability discovery, library operations, stream resolution |
| Authentication    | Guest/account lifecycle, credential storage, authenticated request context                     |
| Playback          | Engine handle, transport, timing, seek, buffer/device events                                   |
| Application/queue | Queue identity/order, shuffle/repeat, persistence, recovery coordination                       |
| Native            | Media sessions, media keys, tray/menu bar, notifications, window lifecycle/materials           |
| Frontend          | Navigation, presentation, accessibility, transient interaction state                           |

Start with modules in one Rust crate, not services or an extensible plugin framework. Add abstractions only around volatile provider and engine boundaries.

## Guest and account modes

Use explicit capability results: available, account required, temporarily unavailable, or unsupported. Do not spread a global `loggedIn` condition through playback and presentation. Private library surfaces can request sign-in; guest search and available guest playback remain independent.

Do not assume Google OAuth or an embedded Google login provides the needed YouTube Music capabilities. Validate the actual flow before choosing dependencies. Keep credentials in platform credential storage behind Rust. Do not expose credentials, cookie exports, or signed stream URLs through frontend state or logs.

`account.rs` owns explicit guest/checking/signing-in/connected/expired/unavailable states. Only a verified account-menu profile can activate a session. Vault mutations retain their own task and serialized lifetime even if their caller is cancelled, so an interrupted Keychain write can finish/roll back. `sign_in.rs` opens a nonpersistent WebView restricted to Google's/YouTube's sign-in origins. Cookies are read in Rust after returning to Music, validated, then verified by the catalog. Only public account state reaches Svelte. Main-window IPC checks both label and local origin; the Google window has no application command privileges. Details and actual verification limits are in radio-and-accounts.md.

## Media path

Implemented path: Rust resolver -> short-lived URL plus required request context -> libmpv -> CoreAudio. Video and engine-side automatic extraction are disabled. Rust retains signed URLs and headers; public IPC DTOs contain metadata and available-format identifiers only.

- `provider.rs`: validates track links and owns a bounded resolver. A tested guest `visionos` client is attempted first, then ordinary guest extraction, then authenticated extraction when a verified session exists and guest extraction failed. The chosen URL is probed with a tiny range request. A maximum of eight resolved tracks is kept privately in memory for at most five minutes, with early expiry guards and a probe on reuse. Signed URLs/headers are never serialized or persisted. Account revisions partition caches, reject late results, and invalidate old quality sources. A temporary owner-only cookie file exists only for an authenticated extractor attempt.
- Provider availability timestamps remain enforced. Only the selected format delays startup; selecting a different quality waits for that source's own availability rather than delaying every song for all formats.
- `catalog.rs`: bootstraps WEB_REMIX context; searches songs/music videos and reads `FEmusic_home`/recommended collections over bounded HTTPS. A latest-wins request slot, 60-second search cache, 15-minute context cache, 100-track page cap, and 256 known metadata entries bound work/storage. Verified account headers/context and all caches are tied to an account revision. Radio uses its own request slot so it does not cancel browsing/search.
- `home.rs`: normalizes up to eight provider shelves and twelve items per shelf, preserving provider titles/order. Public DTOs contain song/collection metadata; collection endpoint parameters stay in Rust. A five-minute guest Home cache and bounded collection pagination support real recommendation cards. No fixed artist/query fallback is used.
- `application.rs`: coordinates latest-wins cancellable resolution, radio/queue transitions, account changes, opaque sources, one-click playback, and local collection operations. Weak-reference background monitors advance on native EOF without renderer participation. Separate foreground, radio, and prefetch cancellation domains plus queue generations reject obsolete work. Catalog metadata is retained for native title/artwork.
- `queue.rs`: owns at most 100 tracks, current position, autoplay preference, transition/prefetch identity, and metadata-only atomic persistence. Natural EOF is claimed once for a bound playback instance and queue generation. Restored queues are stopped. Explicit contexts retain loaded order; search/discovery start seed radio. `radio.rs` normalizes `/next` playlist panels and private continuation cursors. Low-water refill and one next-source prefetch within 30 seconds of EOF bound background work. Published queue revisions reject out-of-order IPC snapshots.
- `library.rs`: atomic JSON persistence for at most 500 local likes and 40 recent tracks. Native Playing transitions record history once per playback instance, on the control actor rather than the real-time audio callback. Existing malformed data is preserved, with a nonfatal library error. Only public metadata is persisted.
- `playback.rs`: a bounded actor owns transport state and snapshots. The app prepares the native engine once in the background alongside a short-lived extractor warm-up, without opening an audio stream. The engine stays resident after preparation; its idle control actor blocks without polling when no track is loaded. Active updates remain capped at four per second. The idle-memory tradeoff still needs full process-tree measurement.
- `mpv.rs`: the isolated FFI boundary loads a replaceable dynamic library, applies audio-only options, bounded media caches, and native commands/events.
- `runtime.rs`: locates the development or packaged runtime. The installed app uses its own Resources/runtime directory, not PATH/Homebrew.

Automatic signed-source renewal and stream recovery remain pending. The libmpv demuxer forward/back byte caps total 24 MiB; this is a configured media-cache limit, not a measured whole-process-memory bound. Next-track prefetch resolves/probes a source; it does not download/decode the next whole song or establish gapless output.

Keep current/next buffers bounded. Resolve the next track near playback, not every queue item up front. Cancel obsolete resolutions on queue changes. Stream expiration recovery, network retry, and user skip are distinct state transitions. Use generation/queue-entry identity to reject stale async results.

Do not fetch/decode on the audio callback. Do not introduce a permanent Python/Node server or duplicate media proxy without measured need.

## Native shell

Use Tauri 2 APIs and a native macOS titlebar/traffic lights first. Exercise native semantic vibrancy before adding any custom Objective-C integration. Windows/Linux adapters come later. Opaque materials are a first-class visual mode.

The macOS override configures a transparent window, native traffic lights, overlay titlebar, dark appearance, and `LiquidGlassClear` with a `HudWindow` fallback through Tauri 2.12.5. On macOS 26+ this is an actual AppKit clear glass view behind a non-opaque WKWebView, not an HTML background image. The web stage is transparent and the default Duskfox tint is 18%. Reduced transparency removes native/CSS effects and uses opaque surfaces. CSS blur remains bounded to player/popovers; there is no continuously animated full-window glow.

The optional `native-diagnostics` build feature checks live NSWindow background alpha/opacity, native effect style, WKWebView opacity, and mounted production DOM properties. It is disabled in normal builds, writes only deidentified developer-requested reports, and has no normal-user side effects. See startup-and-materials.md.

The frontend is organized into a shell and Artwork, TrackList, PlayerBar, QueuePanel, and AccountPanel components. Public artwork URLs are restricted to HTTPS Google image hosts and loaded lazily without a referrer. The renderer subscribes to playback, queue, selection/quality, and account snapshots; it refreshes them on visibility changes. Transport sends native queue commands rather than walking frontend search results. No renderer timer advances playback.

`volume.ts` owns a separate latest-value writer: slider input sends an immediate native update, keeps at most one write in flight, and coalesces intermediate values. It bypasses the transport `busy` gate. PlayerBar retains the local gesture value until the final write settles so older playback events cannot move the thumb under the pointer. Volume acknowledgments update only the volume field, never older track/transport state.

`appearance.ts` validates and persists versioned presentation preferences. Palette tokens and material rules are separate; Duskfox is the only complete theme currently shipped. AppearancePanel controls tint/accent/density/sidebar/artwork and reduced transparency. NowPlayingPanel requests up to 512px artwork using only Google's known resize syntax; Artwork retries the original thumbnail once and retains that fallback across playback snapshots. No canvas extraction or continuously animated artwork is introduced.

## Closed-source packaging constraint

libmpv's license depends on its build and linked components. The spike uses the zapper v0.41.0-1 LGPL-3.0-or-later dynamic build, with complete pinned-source materials available upstream. yt-dlp is a separate GPL-licensed standalone process. Record artifact-specific obligations and complete the redistribution materials before publishing downloads; see native-dependencies.md. The preparation script checks SHA-256 digests and rejects libmpv builds referencing non-system dynamic dependencies on arm64.

Sources:

- https://v2.tauri.app/reference/javascript/api/namespacewindow/#effect
- https://mpv.io/manual/stable/#using-mpv-from-other-programs-or-scripts
- https://github.com/mpv-player/mpv/blob/master/Copyright
- https://github.com/yt-dlp/yt-dlp#dependencies
