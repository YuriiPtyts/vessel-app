# Project tree: how Vessel works

Vessel is a **Svelte interface inside a Tauri desktop app**. Rust owns the music services and playback state; libmpv decodes and plays the audio. macOS and Windows share the interface and Rust core, with platform-specific runtime, credential, and media-control adapters.

## Source tree

```text
Vessel-app/
├── src/                                  Frontend: presentation and user interactions
│   ├── main.ts                           Mounts the Svelte application
│   ├── App.svelte                        Navigation, view state, commands, event subscriptions
│   ├── app.css                           Application layout and base styles
│   └── lib/
│       ├── native.ts                     Typed Tauri commands, events, and public snapshots
│       ├── volume.ts                     Latest-value slider writes; one request in flight
│       ├── appearance.ts                 Local appearance preferences and migrations
│       ├── artwork.ts                    Artwork sizing and fallback helpers
│       ├── motion.ts                     Shared panel reveal transition
│       ├── themes/duskfox.css            Default semantic palette
│       ├── glass.css / controls.css      Surface materials and control styles
│       ├── home.css                      Home/discovery presentation
│       └── components/
│           ├── HomeView.svelte           Provider recommendation shelves
│           ├── TrackList.svelte          Song rows and actions
│           ├── PlayerBar.svelte          Transport, seek, volume, source quality
│           ├── QueuePanel.svelte         Rust-owned queue and autoplay controls
│           ├── AccountPanel.svelte       Public account status and sign-in controls
│           ├── AppearancePanel.svelte    Interface customization
│           ├── NowPlayingPanel.svelte    Expanded current-track artwork
│           └── Artwork.svelte            Lazy image loading and fallback rendering
│
├── src-tauri/                            Rust core and native application
│   ├── Cargo.toml                        Rust dependencies and diagnostic feature
│   ├── tauri.conf.json                   Shared app identity, resources, window, CSP
│   ├── tauri.macos.conf.json             macOS window/material configuration
│   ├── tauri.windows.conf.json           Windows installer and WebView2 configuration
│   ├── src/
│   │   ├── main.rs                       Startup, trusted IPC, native shell, shutdown
│   │   ├── lib.rs                        Shared Rust library entry point
│   │   ├── application.rs                Coordinates services, selections, queue transitions
│   │   ├── catalog.rs                    YouTube Music browsing/search and request context
│   │   ├── home.rs                       Home shelves and collection response parsing
│   │   ├── provider.rs                   Source cache, resolution, probing, cancellation
│   │   ├── direct.rs                     Rust-only fast guest source lookup
│   │   ├── playback.rs                   Single-owner player thread, transport, snapshots
│   │   ├── mpv.rs                        Dynamic libmpv FFI boundary and engine commands
│   │   ├── runtime.rs                    Platform-specific bundled runtime filenames
│   │   ├── queue.rs                      Queue identity, order, autoplay, JSON persistence
│   │   ├── radio.rs                      Radio track and continuation parsing
│   │   ├── library.rs                    Local likes and listening history
│   │   ├── account.rs                    Verified account lifecycle and credential boundary
│   │   ├── account/windows_vault.rs      Current-user DPAPI session storage
│   │   ├── sign_in.rs                    Isolated Google window and Rust cookie handling
│   │   ├── media_controls.rs             macOS Now Playing and media commands
│   │   ├── media_controls_windows.rs     Windows media transport controls
│   │   ├── provider/windows_job.rs       Owns the Windows extractor process tree
│   │   ├── error.rs                      Public, redacted application errors
│   │   └── *diagnostics.rs / test_audio.rs  Native verification helpers and fixtures
│   ├── tests/live_*.rs                   Explicit network/native integration checks
│   ├── runtime/                         Generated playback/extraction dependencies
│   └── target/                          Generated Rust binaries and app bundles
│
├── native/
│   ├── mpv-build.json                    Pinned macOS engine build inputs
│   └── patches/                         Engine build, CoreAudio cleanup, output-gain changes
├── native-dependencies.json              macOS dependency manifest and checksums
├── native-dependencies.windows.json      Windows dependency manifest and checksums
├── scripts/                              Runtime preparation, packaging, native verification
├── tests/ui/                             Playwright/WebKit interface checks with mocked IPC
├── .github/workflows/windows-build.yml   Native Windows build/install verification
├── docs/                                 Architecture, decisions, evidence, future work
├── package.json                          Frontend, desktop, packaging, and check commands
├── dist/                                 Generated frontend production build
└── .cache/                               Generated dependency caches and release downloads
```

`runtime/`, `target/`, `dist/`, and `.cache/` are generated outputs. Their contents are prepared or built locally/CI rather than committed as application source.

## Runtime flow

```text
User action: search, browse, play, seek, change quality, edit queue
└── Svelte component / App.svelte
    └── src/lib/native.ts: typed command
        └── Tauri IPC → main.rs: verify main-window label and local origin
            └── application.rs: coordinate the request
                ├── Search / Home / collections
                │   └── catalog.rs → YouTube Music HTTPS → parsed public metadata → UI
                ├── Play a song
                │   ├── 1. provider.rs resolves and probes a source
                │   │   ├── Reuse a valid cached source
                │   │   ├── direct.rs: Rust guest source lookup
                │   │   └── Fallback: bundled yt-dlp + Deno
                │   └── 2. Validated signed URL + headers stay private in Rust
                │       └── playback.rs: player thread → mpv.rs → libmpv
                │           ├── macOS: CoreAudio / AVFoundation
                │           └── Windows: WASAPI
                ├── Queue / autoplay
                │   └── queue.rs + radio.rs + source prefetch
                ├── Likes / recent listening
                │   └── library.rs → local metadata JSON
                └── Account commands
                    └── sign_in.rs + account.rs → verification + protected session storage

Native playback events
└── mpv.rs → playback.rs → application.rs
    ├── Public snapshots → Tauri events → native.ts subscriptions → Svelte views
    ├── Native EOF → claim queue transition once → prepare/play next source
    └── Current metadata/state → macOS Now Playing / Windows media controls

Keyboard media buttons
└── Native media adapter → the same Rust previous/next/play/pause operations
```

The Rust player and queue can keep playing and advance without a renderer timer. Detailed module behavior and bounds are in [architecture](architecture.md); resolver/gain-specific evidence is in [volume response](volume-response.md) and [verification](verification.md).

## Where state lives

- **Rust:** playback/transport, current source and quality, queue/radio, provider work/caches, account verification, and local collection.
- **Frontend:** navigation, open panels, appearance preferences, and temporary interaction drafts such as the slider thumb during a drag. Native snapshots remain the playback source of truth.
- **Application-data files:** likes/history and queue metadata. Restoring a queue does not automatically start playback.
- **Protected account storage:** macOS Keychain or an atomically written, current-user DPAPI-encrypted Windows record. Credentials and signed stream URLs are not public frontend snapshots.

## Build and release tree

```text
Source + pinned dependency manifests
├── 1. prepare-native.mjs: select the host platform
│   ├── macOS → build pinned engine/extractor; verify Deno → src-tauri/runtime/
│   └── Windows → pinned DLL/executables → src-tauri/runtime/; icons → .cache/windows-icons/
└── 2. Frontend build (Vite) + Rust/Tauri build, using the prepared runtime
    ├── npm run package:macos
    │   └── Sign/verify staged app → ZIP + drag-and-drop DMG + hashes/signing report
    └── npm run package:windows
        └── NSIS setup executable + Windows hash/build report
            └── Native Windows CI checks install, runtimes, WebView2, launch, uninstall

Verified release files → private GitHub beta release
```

The common build step applies to both platforms. Packaging and actual verification scope are documented in [macOS distribution](macos-distribution.md), [Windows builds](windows-build.md), and [native dependencies](native-dependencies.md).

## Future work

**SoundCloud is a planned provider, not an implemented branch of the runtime flow.** Its unchecked task is in the [owner-directed future milestones](opportunities.md#owner-directed-future-milestones), alongside the existing theme milestone. Product decisions and longer-term plans remain in [decisions](decisions.md) and [feasibility](feasibility.md).
