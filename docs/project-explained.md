# Vessel, explained simply

This guide explains the project from the first working player to the current beta: what it does, what we used, why we used it, and what the choices cost us.

It follows the project through **Vessel 0.1.0-beta.10** on **October 28, 2026**. Older measurements are labelled with their version. Results describe what we checked, not a promise that every computer, account, or song will behave the same way.

## Contents

1. [What Vessel is](#1-what-vessel-is)
2. [How the project developed](#2-how-the-project-developed)
3. [The basic structure](#3-the-basic-structure)
4. [Languages: what each one does, and its trade-offs](#4-languages-what-each-one-does-and-its-trade-offs)
5. [The main tools and why we chose them](#5-the-main-tools-and-why-we-chose-them)
6. [What happens when you play a song](#6-what-happens-when-you-play-a-song)
7. [Search, recommendations, and the queue](#7-search-recommendations-and-the-queue)
8. [Accounts, likes, and saved playlists](#8-accounts-likes-and-saved-playlists)
9. [What is saved, and where](#9-what-is-saved-and-where)
10. [Appearance and desktop features](#10-appearance-and-desktop-features)
11. [The important problems we solved](#11-the-important-problems-we-solved)
12. [How we test the app](#12-how-we-test-the-app)
13. [How the app is built and delivered](#13-how-the-app-is-built-and-delivered)
14. [What is finished, and what is next](#14-what-is-finished-and-what-is-next)
15. [Where to find the code and deeper explanations](#15-where-to-find-the-code-and-deeper-explanations)

## 1. What Vessel is

Vessel is a desktop music app for **Apple Silicon Macs and Windows x64 computers**. It uses YouTube Music for song information, recommendations, and playable sources. The repository and beta downloads are private.

The goal is simple: open the app, find music, and listen without managing links or installing extra playback tools.

You can:

- Search for songs and music videos, then play their audio.
- Browse recommendations supplied by YouTube Music.
- Open recommended albums/playlists and saved account playlists.
- Create playlists in YouTube Music and add songs to your own playlists.
- Open available song lyrics in a scrollable window while listening.
- Pause, seek, change volume, and choose from the audio formats actually available.
- Keep a queue, play related songs automatically, and use operating-system media controls.
- Listen as a guest or connect a Google account.
- Use YouTube Music account likes and return to recent listening on this computer.
- Change colors, transparency, row spacing, sidebar layout, and artwork display.

Vessel is an **audio player**, including when you select a music-video search result. It does not show the video. It also does not turn YouTube's audio into lossless audio or guarantee access to Premium-only formats.

“Lightweight” is a design goal. We have improved startup and limited background work, but we have not completed a full memory, processor (CPU), battery, and long-session comparison against other music apps.

## 2. How the project developed

The project grew in working stages. We tested individual pieces before adding more features.

| Stage                              | What we built or changed                                                                                                | Why it mattered                                                                                         |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| First working player               | A macOS shell that could resolve a real guest song, play audio, seek, and switch quality.                               | Proved that the basic playback approach could work.                                                     |
| Search-first app                   | Search, song rows, a persistent player, local likes, and recent listening replaced the link-inspection screen.          | Made the app useful for ordinary listening rather than technical experiments.                           |
| Personal appearance                | Duskfox colors, glass settings, live volume controls, and larger artwork.                                               | Gave the app its own look while preserving the owner's choices across restarts.                         |
| Real discovery and faster playback | YouTube Music's actual Home recommendations, a quicker bundled extractor, source reuse, and later a direct Rust lookup. | Removed hard-coded discovery content and reduced waiting before playback.                               |
| Radio and queue                    | Related songs, automatic next-song playback, queue editing, and saved queue information.                                | Let music continue without the interface running a next-song timer.                                     |
| Optional accounts                  | Google's own sign-in window, account verification, protected session storage, and remembered sign-in.                   | Added account features while keeping guest listening available.                                         |
| Audio-device cleanup               | A focused fix for cleanup after a failed macOS audio-output startup.                                                    | Addressed a native failure found during the device-change investigation.                                |
| Installable macOS beta             | Correct signing checks, ZIP packaging, and a drag-to-Applications DMG.                                                  | Made the built app deliverable as a self-contained download.                                            |
| Responsive volume and media keys   | Output-stage volume on macOS and operating-system playback controls.                                                    | Fixed the reproduced queued-audio volume delay and connected desktop media commands to the Rust player. |
| Windows support                    | Windows playback dependencies, protected account storage, media controls, and an installer.                             | Reused the shared app while adding the Windows-specific pieces.                                         |
| Readable glass                     | Better text contrast and Appearance/Queue panels that follow the same transparency setting.                             | Kept the glass look without making text hard to read.                                                   |
| Account library, beta.6            | Account likes, like/unlike requests, saved sidebar playlists, refresh, and protection from old account replies.         | Connected the collection views to YouTube Music while preserving local favorites.                       |
| Sidebar and shared glass, beta.7   | A separate scrolling playlist area, one transparency level, and text that changes between dark and light.               | Keeps Appearance reachable and removes the dark reading blocks that made the glass look uneven.         |

The [Git history](https://github.com/YuriiPtyts/vessel-app/commits/main/) records these changes. The [verification log](verification.md) records what was actually tested at each stage.

Beta.8 separates Light/Dark from transparency, gives panels a balanced opening animation, adds outside-click closing, and removes Device likes from the interface. It also replaces loaded background masks with live vector cutouts to fix the brief clear-window flash and gaps at the player edges.

Beta.9 adds the missing Windows native transparency setup, handles more trusted Google/YouTube sign-in completion paths, and replaces thin text outlines with filled, heavier letters and a shadow at high transparency. These fixes leave the chosen mode, background colors, icons, and saved data intact.

Beta.10 adds account playlist creation and song additions, with Private as the default visibility. It also adds an on-demand lyrics window, including provider credit and a clear message when a recording has no lyrics. These use the existing Rust/provider connection rather than a separate playlist store or lyrics server.

## 3. The basic structure

The app has two main parts and a separate audio engine:

```text
What you see and click
Svelte + TypeScript + HTML/CSS
          |
          | Named commands and updates through Tauri
          v
The part that runs the music and account work
Rust
          |
          +-- Requests music information from YouTube Music
          +-- Finds a playable source, with yt-dlp as a fallback
          +-- Manages accounts, likes, and the queue
          |
          v
The audio engine
libmpv -> macOS or Windows audio system -> your output device
```

**Tauri** puts the interface in a desktop window and connects it to Rust. The interface uses the operating system's web display engine: Apple's WKWebView on macOS and Microsoft's WebView2 on Windows.

The interface asks Rust to do things such as “play this song” or “set the volume.” Rust sends back information such as the current track, position, queue, and account status.

A few terms used below: the **provider** is YouTube Music; a **source** is the temporary internet address used to read a song's audio; a **cache** is a small temporary store that avoids repeating work. **Native** means using the operating system's own features, such as audio output or media controls.

We keep playback in Rust so it does not depend on a visible page or a JavaScript timer. Hiding the window does not hand responsibility for the next song back to the interface. Quitting the app still stops its playback.

There is no separate Vessel cloud server or database service. The desktop app talks to the provider and keeps local settings/data on the computer. It still needs the internet for provider searches and streamed music.

## 4. Languages: what each one does, and its trade-offs

### The main application languages

#### Rust: running the music

Rust handles playback, the queue, network requests, accounts, local data, and operating-system features. Most of this code is in `src-tauri/`.

We use it here because these jobs need careful control over data and work happening in the background.

- **Pros:** fast compiled code; catches many memory and threading mistakes before the app runs; can talk directly to audio libraries and the operating system.
- **Cons:** more demanding to write, and native builds can take time. The small parts that call C or operating-system functions need extra care. Rust cannot prevent a bug inside a third-party audio library.

#### TypeScript: making the interface work

TypeScript handles interface behavior, appearance settings, and the commands sent to Rust. That code lives in `src/`.

It lets us describe what data a function expects, such as a track with a title and artist, and catch many mistakes while building the app.

- **Pros:** helps catch wrong field names and incompatible data; works well with Svelte and browser functions; makes interface changes easier to check.
- **Cons:** the build converts it to JavaScript, so its type checks are not a substitute for checking incoming data while the app runs. The Rust and interface definitions still need to agree and be tested together.

#### HTML-style markup: arranging the page

The `.svelte` files and `index.html` describe buttons, song tables, forms, and page sections.

- **Pros:** easy to read; standard buttons and forms provide a useful starting point for keyboard and screen-reader support.
- **Cons:** accessible behavior still needs care. A web-based button or window does not automatically behave exactly like every native desktop control.

#### CSS: controlling the appearance

CSS controls layout, colors, glass, contrast, and the way the interface adjusts to small or large windows.

- **Pros:** appearance can change without touching playback code; the same page can fit different window sizes.
- **Cons:** style rules can affect one another unexpectedly. Blur and transparency may add graphics work, and different web display engines can produce different results.

#### JavaScript: preparing and checking the app

JavaScript is used in build/package scripts and many tests. The finished interface also runs as JavaScript after TypeScript and Svelte are built.

- **Pros:** Node.js makes it convenient to run tools, prepare files, and automate builds on both platforms.
- **Cons:** the scripts have fewer built-in type checks than the TypeScript app code. Windows and macOS commands still need different handling.

Svelte is a tool for building the interface, not another general-purpose language. A `.svelte` file combines the page structure with its TypeScript behavior and, where needed, CSS.

### Other languages used for specific jobs

| Language              | Its actual role in this project                                                                                                                                       | Benefit                                                                               | Trade-off                                                                                                                  |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| **Python**            | yt-dlp is written in Python. The macOS build creates a bundled copy that includes Python and its required packages.                                                   | Reuses a maintained tool that understands provider playback details.                  | Adds download size and startup work. Provider changes can still require updates. We do not keep a Python server running.   |
| **C and Objective-C** | Small macOS audio fixes and test helpers, including the files under `native/patches/` and `scripts/native/`. The audio engine also relies on third-party native code. | Lets us reach the exact audio functions needed to investigate or fix native problems. | Manual memory/lifetime rules and platform-specific code make this part harder to maintain. Changes need real native tests. |
| **Swift**             | A macOS audio-device test helper, `scripts/audio-hotplug.swift`, and some native diagnostic commands.                                                                 | Gives test code direct access to Apple's frameworks.                                  | These helpers only run on macOS; Swift is not the language of the main Vessel app.                                         |
| **PowerShell**        | Windows WebView2 and installer verification scripts.                                                                                                                  | A practical way to install, launch, inspect, and remove the app on Windows.           | These scripts are Windows-specific and need to allow for operations that finish in the background.                         |

JSON, TOML, and YAML hold settings, dependency information, and build instructions. Markdown holds documents like this one. They describe data and procedures; they are not additional playback languages.

## 5. The main tools and why we chose them

### Tauri 2: the desktop window and connection to Rust

Tauri lets us share a web-based interface between macOS and Windows while putting the music work in Rust.

**Why it helps:** it uses the system's web display engine rather than shipping a separate copy of Chromium with the app. It also gives us desktop windows, packaged resources, and a controlled way to call Rust.

**What we give up:** the two platforms do not use exactly the same display engine. We still need platform-specific work for glass, media controls, credentials, and installers. A small desktop shell also does not make the bundled audio/extraction tools disappear from the download size.

### Svelte 5: the visible interface

Svelte turns components such as the player, queue, and account panel into the code that updates the screen.

**Why it helps:** the component files are easy to follow, and we can change the interface without changing the audio engine.

**What we give up:** it is still a web-based interface. We must test keyboard use, layout, and desktop integration ourselves. Shared application behavior has also made `App.svelte` fairly large; smaller components and Rust modules keep some responsibilities separate.

### libmpv and FFmpeg: decoding and playing audio

**libmpv** is the playback library. It uses media libraries, including **FFmpeg**, to read formats such as AAC and Opus and turn them into audio the operating system can play.

**Why it helps:** we reuse a mature media engine instead of writing a decoder, buffering system, and audio-device integration from scratch.

**What we give up:** it adds native dependencies, package size, and license obligations. The macOS build carries targeted fixes that we must maintain. Windows uses a separately pinned upstream build, so its low-level volume path is different.

The app disables video playback and does not ask libmpv to run its own source extractor. Rust supplies the selected audio source.

### YouTube Music requests, yt-dlp, and Deno: finding the music

Rust requests song information and recommendations from YouTube Music. These requests use the provider's internal service rather than a stable, official public interface for third-party developers.

**Why it helps:** search, Home shelves, radio, and account collections can use real provider data. A fast guest source lookup also avoids starting another program when the response is usable.

**What we give up:** provider responses and playback rules can change. Available songs and formats can differ by account, region, and time.

When the direct source lookup cannot handle a response, Rust tries the bundled **yt-dlp** extractor. **Deno** runs JavaScript needed by yt-dlp's provider challenge handling.

An extractor is a tool that works out a playable address and the request details needed to use it. Provider challenges are extra checks that can be part of finding that address.

This gives us a fallback for more complicated sources, but it can be slower and needs maintenance too. Deno runs for that job; it is not a permanent application server. The Python project **ytmusicapi** was used as a reference for library request formats, not as a running service inside Vessel.

### The supporting tools

| Tool                                                  | What we use it for                                                                | Why, and the main trade-off                                                                                                                                   |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Tokio**                                             | Rust network/process tasks, waiting, time limits, and cancellation.               | Keeps waiting requests from blocking the interface; background tasks still need careful ordering and cancellation.                                            |
| **reqwest + rustls**                                  | HTTPS requests from Rust.                                                         | Keeps provider traffic and private request data out of interface code; the app must still validate responses and handle failures.                             |
| **Serde + serde_json**                                | Reading/writing structured data and passing public information to the interface.  | Avoids hand-building JSON text; unexpected provider response shapes still need explicit handling.                                                             |
| **url, tempfile, libloading**                         | Checking links, creating temporary files, and loading libmpv.                     | Reuses focused libraries; the app still owns the rules for acceptable links, file lifetime, and native function calls.                                        |
| **objc2 / security-framework; windows / windows-sys** | Calling Apple's and Microsoft's desktop features from Rust.                       | Lets the shared Rust app use each platform's media controls, window features, and protected storage; these parts still need separate platform code and tests. |
| **Node.js + npm**                                     | Installing frontend packages and running development/build scripts.               | Convenient cross-platform tooling; Node.js is a development requirement, not a permanent server shipped with Vessel.                                          |
| **Vite**                                              | Fast interface development and production frontend builds.                        | Makes interface changes quick to work on; the finished desktop app still needs a separate Rust/native build.                                                  |
| **Tailwind CSS 4 + ordinary CSS files**               | The frontend styling setup. Much of Vessel's appearance lives in named CSS rules. | Flexible styling and build tooling; web-engine compatibility and interactions between rules still need testing.                                               |
| **Lucide**                                            | Interface icons.                                                                  | Consistent icons without drawing each one; icons still need clear labels and useful button behavior.                                                          |
| **Cargo**                                             | Rust dependencies, compilation, and tests.                                        | One standard workflow; compiling native dependencies can be slow.                                                                                             |
| **Playwright**                                        | Automated interface tests using WebKit, plus focused Chromium checks.             | Can check real rendered pages and clicks; simulated native replies do not prove that Google or native audio works.                                            |
| **Prettier, rustfmt, Clippy**                         | Consistent formatting and checks for likely mistakes.                             | Make changes easier to review; none replaces behavior tests or listening on real hardware.                                                                    |
| **Git + private GitHub releases/Actions**             | Change history, private downloads, and automated Windows builds.                  | Reproducible work and native Windows checks; downloads still need signing and real-machine testing.                                                           |

The direct package lists are in [package.json](../package.json) and [Cargo.toml](../src-tauri/Cargo.toml). The lockfiles record exact resolved package versions. The two `native-dependencies*.json` files and `native/mpv-build.json` record the separate playback/extraction inputs.

## 6. What happens when you play a song

Here is the complete path from clicking a result to hearing audio:

1. **You choose a track.** The interface sends its song ID and, for a collection, the displayed song order.
2. **Tauri checks the caller.** Only the trusted main app page can use the app commands. The Google sign-in window cannot control playback through those commands.
3. **Rust prepares the queue.** It records the chosen track and the relevant surrounding songs, within the 100-track queue limit.
4. **Rust looks for a usable source.** It first checks its small, short-lived source cache and probes a cached source before reusing it.
5. **It tries the fast guest lookup.** The direct request and source check share a two-second time budget. It accepts supported, recorded audio, not every possible provider response.
6. **It falls back when needed.** yt-dlp tries the tested guest approach, then the ordinary guest approach. If guest extraction fails and a verified account exists, it can try authenticated extraction.
7. **Rust checks timing and identity.** It rejects outdated results and respects any time before which the provider says a source is not ready. Choosing a newer song cancels obsolete preparation.
8. **The source stays private in Rust.** Temporary playback addresses and request headers are passed to the audio engine, not displayed or saved as collection data.
9. **libmpv reads and decodes the audio.** macOS uses CoreAudio/AVFoundation; Windows uses WASAPI, its normal desktop audio system.
10. **Rust reports progress.** The interface shows the track, playback position, and available quality choices. The operating system receives the current media information too.

The app plays a stream; it is not an offline music-download library. Small media buffers and next-song source preparation are there to support playback, not to save a whole music collection. The engine's forward/back media-cache limits total 24 MiB; that limits one kind of stored audio data, not the app's total memory use.

### What “quality” means here

The player lists formats actually returned for that source. It shows the codec and available bitrate information instead of inventing a quality level. A codec describes how the audio is encoded; bitrate describes how much data it uses per second.

AAC and Opus are different compressed audio formats. A higher bitrate number alone does not prove one codec sounds better than another. Unknown information stays unknown.

Changing quality keeps the position and paused/playing state where possible. Converting already compressed audio into an uncompressed output does not restore information that was lost earlier. Lossless support remains conditional on a real, verified lossless source.

## 7. Search, recommendations, and the queue

Search, Home recommendations, and audio playback are related but separate jobs. Finding a song title quickly does not mean its audio source is ready yet.

**Search** reads YouTube Music results and can request another page. **Listen now** displays the provider's own shelves and order. It does not replace a failed feed with a fixed favorite artist or a made-up recommendation list.

Choosing a search/discovery song starts provider radio from that song. Choosing from a loaded collection keeps the relevant displayed order, then radio can add related music afterward.

Rust owns the queue. When the audio engine reports that a song finished naturally, Rust moves to the next song once. A manual stop or a late “finished” message from an older song must not skip the new selection.

The queue holds up to 100 tracks. When selecting deep into a large list, the app keeps the selected song, up to 20 earlier entries, and following entries within that bound. It does not resolve every song in a large account library in advance.

Near the end of a track, it may prepare **one** next source. This improves the handoff, but does not prove playback with no gap between songs, called gapless playback. The saved queue reopens stopped; restarting the app does not unexpectedly start music.

To prevent old work replacing new work, requests carry numbers tied to the current selection or account. If the number no longer matches, the reply is ignored.

**Benefit:** skipping, switching accounts, and searching again are less likely to produce stale screen or playback changes. **Cost:** background work needs more checks and tests than a simple list of songs in the interface would need.

## 8. Accounts, likes, and saved playlists

### Signing in

Google sign-in opens Google's own page in an isolated native window. Vessel does not provide its own password form.

Rust reads the browser session after the sign-in flow reaches a trusted YouTube completion page and asks YouTube Music to verify the account. It checks cookies available to Music first, allows the YouTube consent redirect, and retries incomplete verification briefly. A 45-second verification limit prevents a pending handoff from waiting indefinitely. Loading a page or finding an arbitrary cookie is not enough to display Connected; saving a verified session remains a separate protected step.

Remembered sessions use:

- **macOS Keychain** on a Mac.
- **Windows DPAPI**, which encrypts the stored record for the current Windows user.

On restart, the app checks a remembered session before using it as a verified connection. Google may refuse embedded sign-in or expire a session. Guest listening remains available when account connection fails.

### Likes belong to YouTube Music

Liked songs and heart actions use the connected, verified YouTube Music account. A successful write changes the visible heart; a failed write keeps its previous state and shows an error.

Device likes was an earlier local feature and was removed from the interface in beta.8 at the owner's request. Guests can still search, play, and use recent history; they connect Google to use likes. Earlier local files are preserved, without uploading or merging their contents into account likes.

Saved account playlists appear in the sidebar. Open one to browse its songs and request more pages.

Choose **Create playlist** in the sidebar to make one in YouTube Music. Give it a name, an optional description, and a visibility choice; Private is the default. The list-plus button beside a song or in the player opens **Add to playlist**. You can choose one of your editable playlists or create a new one containing that song. Saved playlists from other people remain read-only destinations.

Rust sends the change and waits for YouTube Music to confirm it. An unconfirmed request is not automatically repeated, because it might already have reached the provider. Newly created playlists appear immediately; a changed open playlist reloads its songs. Real-account writes still need the owner's verification. See [playlists and lyrics](playlists-and-lyrics.md).

### Refresh and account switching

The app refreshes after a verified connection and on returning to the app, with one-minute reuse after a successful refresh. The sidebar refresh button and Refresh likes bypass that reuse.

Likes and playlists can fail independently. A normal refresh failure keeps the last good data rather than claiming the collection is empty. Signing out or changing accounts clears the temporary account-library data and cancels old work.

Account-library limits are 10,000 liked IDs, 1,000 saved playlists, and 200 pages. Each library section has a two-minute loading limit. Opened playlists load incrementally, up to 10,000 tracks/200 pages. Exceeding a limit is an error rather than a claim that a partial result is complete.

These account-library operations are implemented and tested with controlled responses. **Live reads, writes, and cross-app refresh with the owner's real account still need verification.** The same applies to real Google/second-factor acceptance and account-required/Premium playback.

## 9. What is saved, and where

| Information                                    | Where it lives                                                                        | Why                                                                           |
| ---------------------------------------------- | ------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Recent listening and retained older local data | `collection.json` in Vessel's application-data folder.                                | Keeps history local and avoids deleting data from earlier versions.           |
| Queue and autoplay preference                  | `queue.json` in the application-data folder.                                          | Restores listening context without automatically restarting audio.            |
| Appearance choices                             | The main interface's local storage.                                                   | These are display preferences, so the interface owns them.                    |
| Remembered sign-in                             | Keychain on macOS; a current-user-encrypted `google-session.dpapi` record on Windows. | Keeps session credentials separate from ordinary song metadata.               |
| Account likes/playlists                        | Temporary Rust memory, refreshed from YouTube Music.                                  | Avoids confusing old local data with the connected account's current library. |
| Short-lived playback addresses and headers     | Temporary Rust memory, with expiry checks.                                            | They are private, temporary access details rather than durable music records. |

Recent listening is bounded to 40 tracks. Older files may contain up to 500 local favorites, retained for compatibility rather than offered as a Device likes collection. The files contain song titles, artists, and artwork links, not downloaded audio.

The local file code writes a replacement safely and preserves malformed existing collection/queue data rather than silently wiping it. Local metadata is ordinary local data, not the same thing as encrypted credential storage.

During authenticated extractor fallback, a private temporary cookie file may be needed for the external tool. It is removed afterward and does not become frontend state or part of a release.

Sign-out forgets the stored Google session, clears the dedicated sign-in browser data, and stops playback. Recent listening, queue information, appearance preferences, and earlier data files remain.

## 10. Appearance and desktop features

**Duskfox** is the default palette, based on the owner's preferred editor colors. Color roles are stored separately from layout rules so future themes can reuse the same interface.

Light/Dark has its own buttons in Appearance and a sun/moon shortcut in the top bar. Glass tint ranges from 50% to 100% and changes only transparency. It never switches the mode or recolors surfaces/icons. When the glass is very clear, only letters receive extra contrast. At 100%, or with reduced transparency, the chosen material becomes solid.

Below 70% tint, dark mode uses filled, heavier white text with a dark shadow behind the letters. We removed the thin outline because it made small letters harder to read. Light mode keeps dark text with extra weight.

All main areas share the same material level. Floating panels use live vector cutouts instead of loading replacement mask images, so the background stays painted while a menu opens. Appearance's contents scroll inside its fixed background. Menus close on outside clicks or Escape, and the outside control still works.

Saved playlists now scroll in their own sidebar area. The main navigation and Appearance/Account controls stay in place, even with a long playlist list.

On supported macOS versions, the window uses Apple's native clear-glass effect, with a fallback on older systems. Windows uses a transparent WebView2 surface and Microsoft's Acrylic backdrop, subject to Windows' transparency settings. These native effects are separate from CSS styling inside the page; reduced transparency removes the native effect and makes the chosen material solid.

Panels now open with a short, proportional scale and content fade. The old stretched genie is gone; the background keeps its alpha during the reveal. Reduced motion opens panels immediately. Artwork is loaded from provider image addresses, with a fallback if an optional larger image fails.

Operating-system media controls call the same Rust play/pause/previous/next operations as the onscreen buttons. macOS uses Now Playing; Windows uses System Media Transport Controls.

The player's **Lyrics** button opens a scrollable window for the playing song. Rust asks YouTube Music for available lyrics and keeps its source credit and line breaks. The window follows song changes, ignores old replies, and closes when playback clears. It does not repeatedly fetch text as the playback position changes. Missing lyrics and loading failures are separate states; this is plain reading text, not synchronized karaoke.

Lyrics load only when requested and can be reused from a small five-minute memory cache. They are not written to a local collection or generated by Vessel. The window traps keyboard focus while open; Escape, the close button, or an outside click dismisses it and returns focus. The inert interface is blurred while the window is open so its text does not overlap the lyrics/form; opaque mode respects reduced transparency.

**Why this design:** a familiar desktop feel with one shared interface. **Trade-off:** native effects, image loading, accessibility, and graphics cost still need platform and hardware testing; attractive glass alone is not evidence of low power use.

## 11. The important problems we solved

### Songs took too long to start

The first packaged extractor could take roughly 7–8 seconds just to start and print its version. Some early full playback runs took around 17–20 seconds to reach decoded audio.

We changed the macOS extractor to a reusable directory layout, added bounded source reuse, prepared the engine in the background, and added a direct guest lookup in Rust. More difficult requests still use the extractor fallback.

Some earlier warmed direct-path tests reached decoded output in under a second. The beta.6 guest catalog test took **3.060 seconds**, with **0.889 seconds** for a cached repeat. These are different individual test runs, not a guaranteed speed or a measurement of audible sound reaching headphones.

The trade-off is more than one source-lookup path to maintain, plus some memory kept ready to reduce waiting.

### The volume slider moved, but the sound changed late

The interface was sending changes promptly, but one macOS audio path had already queued about 1.8 seconds of audio with the old volume applied.

Sending the same command faster could not change that queued audio. We added per-player volume control at the output stage, where it can affect audio already waiting to play. The change preserves mute and the chosen volume when the output is recreated. It does not change the Mac's system volume.

Native tests removed the reproduced queue-related delay. This is not a measurement of Bluetooth or physical-device delay. Windows uses its standard mpv/WASAPI volume path rather than that macOS-specific extension.

### Audio-device changes exposed a native cleanup problem

We investigated crash reports, added device-change checks, and brought in a focused fix from newer audio-engine code. It cleans up audio-device notifications when starting an output fails, rather than leaving them attached after failure.

The native regression passes. Switching the owner's actual headphones/AirPods and other physical-device combinations still needs hands-on confirmation.

### A signed-looking app still failed package checks

The first macOS archive had incomplete signatures, and copying the bundled Python framework damaged its expected layout.

The packager now preserves that layout, signs the included executable code from the inside out, and checks the final ZIP after extraction and the DMG after mounting. Beta.6 verifies 107 code signatures.

A valid development signature is still different from Apple's trusted distribution signing. The private macOS beta has not gone through Apple's additional check for downloadable apps, called notarization, so macOS may require Open Anyway. The Windows beta is unsigned too.

### Account data could arrive after the user had moved on

Network replies do not always arrive in the order requests were made. A reply from an old account must not fill the new account's sidebar or change its hearts.

A number for the current account session and a number for the latest update are checked before accepting results in Rust and the interface. Tests deliberately delay replies, sign out, and then deliver the old results. Local and account collections also use separate storage paths.

### Windows glass and sign-in needed their own native checks

Transparent page colors cannot reveal the desktop if the native window or embedded web display is still opaque. The old Windows setup left that layer opaque, and its material-change command did nothing. Beta.9 clears both backgrounds and explicitly applies or removes Acrylic.

The sign-in controller also waited only for the Music host and rejected a YouTube consent page. It now accepts trusted YouTube completion paths, reads suitable authentication-cookie candidates without requiring an unrelated extra cookie, and gives partial verification a bounded retry. The provider must still confirm the account before it is connected or saved. A dummy native-cookie check exercises the Windows handoff without entering a real Google password; the owner's actual account still needs a retry.

See [startup](startup-and-materials.md), [volume](volume-response.md), [device investigation](audio-device-investigation.md), and [package signing](macos-distribution.md) for the detailed evidence.

## 12. How we test the app

We use different tests for different questions. One type cannot stand in for all the others.

| Check                           | What it tells us                                                                                                                      | What it does not tell us                                                           |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Type checks, formatting, Clippy | The source has consistent formatting and passes useful language checks.                                                               | That a real song plays or a real account accepts a request.                        |
| Rust behavior tests             | Queue rules, stored data, cancellation, account changes, and error handling behave as expected.                                       | That every provider response or physical device works.                             |
| Controlled HTTP tests           | The app sends the intended requests and handles known provider responses, including late replies and failures.                        | That Google accepted real credentials or a live library write.                     |
| WebKit interface tests          | The actual rendered page responds correctly to clicks, layout changes, and simulated native replies.                                  | That real Tauri playback, WebView2, or account services work.                      |
| Live guest tests                | Real provider search, browsing, source resolution, and native decoding work in the tested run.                                        | Premium behavior, every region/song, or subjective sound quality.                  |
| Native/package tests            | Real windows, protected storage fixtures, installed dependencies, package integrity, and launch/quit work in the checked environment. | Every OS version, real media-key delivery, or physical speaker/headphone behavior. |

For beta.10, the recorded checks include:

- **59 WebKit interface tests and 35 focused Chromium tests.** Both explicit modes are checked over bright/dark/patterned backgrounds at six tint values. Tests also cover proportional reveals, outside clicks, delayed-mask flashes, player edges, Appearance scrolling, account-only likes, the 80-playlist sidebar, playlist writes, and lyrics dialogs.
- **57 ordinary Rust library tests on macOS** and **59 on Windows**, plus two binary tests and the non-network integration check. New coverage includes authenticated playlist creation/addition, owned/read-only destinations, rejected writes, stale accounts, empty playlists, and lyrics parsing/cache boundaries.
- Successful frontend checks/builds, Rust formatting, and Clippy checks.
- Live guest search, pagination, recommended collection browsing, and muted native playback were checked in beta.6. The beta.10 native-window check loaded three live Home shelves; a live guest lyrics lookup returned 45 lines with source credit in the observed run.
- macOS ZIP/DMG integrity, 107 signatures, and an eight-second extracted-app launch followed by normal quit.
- Windows native WebView2 transparency and Acrylic removal/reapply, Rust command calls, media-control availability, installed-runtime decoding/volume, install, normal launch/quit, and uninstall. A native private-browser dummy-cookie check also captures the session and closes the handoff without falsely connecting an account.

Some native/network tests run only when explicitly requested, so ordinary test totals do not include every experiment. Muted or silent-output decoding tests are not listening tests. The [verification log](verification.md) keeps those distinctions and the measured results.

## 13. How the app is built and delivered

### On the development machine

Node.js/npm builds the interface. Rust/Cargo builds the desktop program. Platform tools build or prepare the included audio and extraction programs.

For macOS, that includes Apple's developer tools and tools for the pinned native engine/extractor builds. Windows uses Rust's Microsoft toolchain and the Windows SDK. The exact setup is described in the [README](../README.md#development) and the platform guides.

The macOS engine build uses the MPVKit build recipe with CMake, Ninja, Meson, and pkg-config. These tools turn the selected native source into the library we ship. Python and PyInstaller prepare the self-contained yt-dlp directory. Windows uses checksum-verified prebuilt playback/extractor files. This makes the developer setup more involved so the listener's setup can stay simple.

Typical development commands are:

```sh
npm ci
npm run tauri dev
```

The main checks are:

```sh
npm run check
npm run test:ui
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

The interface tests need Playwright's WebKit installation. Explicit live/native checks have additional requirements and are documented separately.

### Preparing a download

1. Read the pinned dependency lists and verify downloaded inputs with checksums.
2. Prepare the platform's libmpv, yt-dlp, and Deno files.
3. Build the interface and Rust application.
4. Bundle the required playback/extraction files with the app.
5. Create and verify the platform download.
6. Upload the verified files and checksum/build reports to a private beta release.

On macOS, `npm run package:macos` produces a ZIP and a DMG with an Applications shortcut. On Windows, `npm run package:windows` produces an NSIS setup executable. The Windows installer also handles WebView2 setup when needed.

The installed app includes the tools it needs. **The listener does not install Python, Node.js, mpv, yt-dlp, Deno, or developer tools manually.**

GitHub Actions builds and checks Windows on a real Windows runner. macOS builds and native checks have been run on the available Apple Silicon host. That is different from proving support on every declared minimum operating-system version.

The [beta.10 release](https://github.com/YuriiPtyts/vessel-app/releases/tag/v0.1.0-beta.10) has a roughly **75 MB ZIP**, **77 MB DMG**, and **85 MB Windows installer**. Those are compressed download sizes, rounded in decimal megabytes, not installed size or memory use.

The repository contains the app source and build instructions. Large downloaded tools, built apps, caches, and test artifacts are generated separately and kept out of source commits.

Vessel is private/proprietary, but the bundled tools retain their own licenses. Using them separately does not remove their notice/source obligations. Public distribution still needs the documented dependency materials, trusted signing, and platform verification.

## 14. What is finished, and what is next

### Implemented in the current beta

The shared desktop interface, guest search/playback, source quality selection, real recommendations, radio, editable saved queue, local history, account likes/playlists, playlist creation/song additions, available lyrics, protected account remembering, media controls, independent appearance controls, and both platform packages are implemented.

“Implemented” and “live-verified” are different. Account-library code has controlled request/UI coverage; it still needs real-account verification. Native media-control registration has been checked; physical media keys still need hands-on checking.

### Work that remains

- Real Google sign-in/second-factor, remembered real-session recovery, likes/playlists, and Premium/account-required playback checks.
- Recovery when a stream address expires or the connection drops during playback.
- Broader physical audio-device, sleep/wake, media-key, and long-session testing.
- Full memory, CPU, battery, startup, and responsiveness measurements across the complete app and its helper processes.
- Wider accessibility, minimum-OS, and clean-machine coverage.
- Full album/artist browsing, gapless playback, and an updater.
- Trusted macOS/Windows distribution signing and remaining public-release dependency materials.
- More complete themes; Duskfox is the only complete palette shipped now.
- SoundCloud support, which is on the backlog but is not implemented.
- Linux and Intel Mac support, which are not validated release targets.

Ideas such as saved listening spaces and sharing small listening collections are separate experiments. They are not hidden features already present in this beta. See the [opportunity list](opportunities.md).

## 15. Where to find the code and deeper explanations

| If you want to understand…               | Start here                                                                             |
| ---------------------------------------- | -------------------------------------------------------------------------------------- |
| The interface and user actions           | `src/App.svelte` and `src/lib/components/`                                             |
| How interface commands reach Rust        | `src/lib/native.ts` and `src-tauri/src/main.rs`                                        |
| How the music parts work together        | `src-tauri/src/application.rs`                                                         |
| Search, recommendations, and collections | `catalog.rs` and `home.rs` under `src-tauri/src/`                                      |
| Finding a playable source                | `provider.rs` and `direct.rs`                                                          |
| Audio playback and volume                | `playback.rs`, `mpv.rs`, and `native/patches/`                                         |
| Queue order and radio                    | `queue.rs` and `radio.rs`                                                              |
| Recent history and older local data      | `library.rs`                                                                           |
| Sign-in and account protection           | `account.rs`, `sign_in.rs`, and `account/windows_vault.rs`                             |
| Account likes and playlists              | `account_library.rs` and its tests                                                     |
| Creating playlists and adding songs      | `playlist_writes.rs` and `src/lib/components/PlaylistDialog.svelte`                     |
| Lyrics lookup and its window             | `lyrics.rs`, `catalog.rs`, and `src/lib/components/LyricsWindow.svelte`                  |
| Appearance and readable glass            | `src/lib/appearance.ts`, `src/lib/material.ts`, `src/lib/themes/`, and the CSS files   |
| Building and packaging                   | `scripts/`, the native dependency manifests, and `.github/workflows/windows-build.yml` |

Menu closing is handled in `src/lib/popover.ts`; proportional opening is in `src/lib/motion.ts`.

For the complete file tree, read the [project map](project-map.md). For the detailed design, read [architecture](architecture.md). For why the product took this direction, read [decisions](decisions.md). For test evidence and limitations, read [verification](verification.md). For installing the current beta, read [beta notes](beta-release.md).

**In one sentence:** Vessel uses a shared Svelte interface to control a Rust music app, which gets music from YouTube Music and plays it through libmpv, with local data and platform-specific desktop features around it.
