# Radio and optional Google connection

## Listening behavior

- Selecting a search result or discovery song starts radio from that song. The next songs come from YouTube Music's `/youtubei/v1/next` radio response, not a local artist search or shuffled search results.
- Selecting a loaded album/playlist or local list retains its order. Radio adds related music after the loaded queue.
- Rust claims a natural EOF once for the current native playback instance and queue generation. Stop, stale source results, old EOF notifications, and obsolete radio responses cannot advance a replacement queue.
- The queue holds at most 100 public track records. Moving far ahead retains up to 20 played tracks. Its metadata, current index, and autoplay choice are saved atomically in `queue.json` beside the local collection. It is restored **stopped**, without persisting stream URLs, request headers, or continuation tokens.
- Autoplay radio controls new recommendations. Turning it off still plays the tracks already queued, then stops at the end. Clearing the queue stops playback and removes its tracks; sign-out stops playback but keeps those tracks.
- Refills run separately from browsing, with a low-water mark of eight future songs and a non-forced 20-second retry cooldown. One next source is resolved/probed within 30 seconds of the current song's end. Selecting/skipping/stopping cancels obsolete preparation. This is bounded source preparation, not bulk audio downloading or gapless output.
- Previous restarts after three seconds (or at the first item); otherwise it goes back one song. A transition skips at most three unavailable sources and then presents a retryable queue error.
- Snapshot revisions protect the renderer from out-of-order queue replies. Native selection events update artwork and available quality when radio advances.

## Account lifecycle

1. **Connect Google** uses an isolated native WebView with its own stable website-data-store identifier on macOS 14+. On older macOS the browser remains ephemeral, preserving isolation from the main window's preferences. It initially opens a blank document, restores any saved browser cookies through Rust, then navigates to Google's own HTTPS page. It has no Vessel command access; subsequent navigation is limited to the permitted Google/YouTube account origins (plus its internal blank document). Vessel has no password form and does not inject credential-extraction JavaScript.
2. After the window returns to YouTube Music, Rust reads its cookie store off the main thread. Validated YouTube cookies form the provider session; validated Google cookies are retained separately for browser restoration. Anonymous or incomplete sign-ins are not saved. Google-domain browser cookies never enter provider headers or extractor cookie files.
3. Rust verifies the session against YouTube's account-menu endpoint. A real account header is required before the public state becomes Connected. Only public name/email/artwork and status cross IPC.
4. A verified session and its scoped browser cookies are saved as a generic password in macOS Keychain under service `app.vessel.desktop.google-session`, entry `youtube-music`. Startup shows the remembered profile while checking, then re-verifies before enabling authenticated requests or announcing Connected. Existing vault records without Google browser cookies still restore. Cancelled vault mutations finish/roll back in a serialized task; browser recovery waits for pending vault mutations too. Remembering does not depend on WebKit flushing its session data before an app crash or exit.
5. Session revisions partition catalog/context/source caches. An expired authenticated request removes its active request context and offers reconnect. Still-valid saved Google browser cookies can be reused for reconnect even when YouTube cookies have expired. Account-backed playback is stopped on expiry. **Sign out / Forget saved session** removes the complete Keychain entry, cancels session work, closes the sign-in window, clears the dedicated browser store, waits for WebKit's deletion completion, and stops playback. Local likes, history, queue, and main-window appearance storage remain. Google may revoke/expire a session; remembering is not a promise of indefinite authentication.

Catalog requests use verified account context for Home/search/radio. Playback keeps the tested fast guest path first, with ordinary guest extraction as fallback. Authenticated extraction is attempted when guest extraction fails, using an owner-only temporary Netscape cookie file that is removed afterwards. This does not establish access to Premium-only formats; quality choices describe only the actual resolved source.

Google can reject embedded sign-in. A rejection is surfaced as an unavailable account connection; guest playback remains usable. No shared OAuth client credentials, browser-profile scraping, cookie-paste UI, or password handling is introduced.

## Observed verification (2026-10-26)

- Real guest radio returned **50 initial tracks**, then **49 new continuation tracks**. Cursor values stayed in Rust.
- A real muted CoreAudio run using the packaged runtime populated a 50-track queue, prepared the next source in **1.63 seconds**, and moved from native EOF to decoded next-song output in **357 ms**, with no renderer present. Stop left the queue saved and playback idle. These are individual observations, not gapless/p95 claims.
- WebKit contract tests exercise queue controls, native-driven selection/quality updates, stale queue events, optional connect/cancel/expiry/sign-out UI, preservation of local likes, and narrow layouts. They use mocked IPC, not actual Google credentials.
- A real Tauri/AppKit sign-in smoke test opened Google's origin, read cookies through Rust, confirmed the actual WKWebsiteDataStore is nonpersistent, cancelled/closed the window, and preserved account revision/local collection. **No account credentials were entered.**
- Real Google password/2FA acceptance, account-menu verification, account-session Keychain round trip, personalized feed/radio, and account-required/Premium playback are **not yet live-verified**.

### Reproduce native sign-in window verification

```sh
npm run tauri -- build --no-bundle --features native-diagnostics
VESSEL_SIGN_IN_DIAGNOSTICS="$TMPDIR/vessel-sign-in.json" VESSEL_RUNTIME_DIR="$PWD/src-tauri/runtime" src-tauri/target/release/Vessel -ApplePersistenceIgnoreState YES -NSQuitAlwaysKeepsWindows NO
node scripts/check-native-sign-in.mjs "$TMPDIR/vessel-sign-in.json"
```

The opt-in diagnostic opens and cancels the real Google window without entering credentials. Its report contains booleans only, without account profiles, cookie values, or page text. Normal builds do not run it.

## Remembered sign-in follow-up (2026-10-27)

- A real native cookie test went red with the original ephemeral window: a fixture cookie was present before close but absent on reopening. Merely selecting a persistent WebKit store did not reliably pass repeated process-restart checks, so browser disk/session survival is not the durability contract.
- `scripts/check-native-remember.mjs` verifies the complete recovery path using **real macOS Keychain** and native WKWebViews. It uses fresh, test-only browser/Keychain namespaces, writes a non-account fixture cookie, deliberately erases browser data, quits/relaunches the app, restores via the same native cookie setter, and verifies explicit forgetting. No real Google credentials or the production Keychain entry are read. Reports are booleans in ignored `.cache/native-remember/`.
- Rust tests cover account-vault restart, a remembered profile remaining Checking until verification, legacy records, Google-cookie separation from provider headers/files, browser reconnect after YouTube-cookie expiry, and explicit sign-out.
- The real Google-window smoke test now checks the supported persistence mode, native cookie access, cancel/forget cleanup, and preserved fixture account/library state. These diagnostic modes use memory-only application services and their own browser store; they do not warm playback or load the owner's Keychain entry.
- **Real Google password/2FA acceptance and restoration of a real signed-in account still require the owner flow below.** Native browser/vault fixtures do not prove server acceptance.

```sh
npm run tauri -- build --debug --bundles app --features native-diagnostics
node scripts/check-native-remember.mjs src-tauri/target/debug/bundle/macos/Vessel.app/Contents/MacOS/Vessel
```

### Owner account verification

In the packaged app, open Account and connect using Google's own window. Confirm that Connected appears only after verification, Home changes to Your feed, restart restores the verified session, and sign-out returns to guest recommendations while preserving local music/appearance. Exercise the account's normal second factor and an account-required song, if available. If Google refuses the embedded window, record the displayed reason; a successful window smoke test is not evidence that Google accepts a real login.

References: [ytmusicapi browser authentication](https://ytmusicapi.readthedocs.io/en/stable/setup/browser.html), [OAuth setup/client requirements](https://ytmusicapi.readthedocs.io/en/stable/setup/oauth.html), and Tauri 2.12.5's native cookie/incognito APIs.

## Account library sync (2026-10-28)

- Once the account is verified, **Liked songs** shows YouTube Music account likes. Hearts in search, playlists, and the player use that account; their tooltip identifies the destination. Writes use explicit like/unlike endpoints and update hearts only after a successful provider response. Failed writes preserve the previous heart/list and show a retryable error.
- **Liked songs is account-only from beta.8.** Device likes navigation and guest-local heart actions were removed at the owner's request. Guests can still search, play, and use recent history; Liked songs asks them to connect. Older local collection files remain preserved, and nothing is automatically uploaded or merged into account likes.
- Saved account playlists appear under **YouTube Music playlists** in the sidebar. Open one to browse/play it; **Show more songs** retains order and removes duplicates. Playback seeds the bounded existing queue from displayed songs.
- Library refresh runs after verified connection/restoration and on app focus/visibility return, with one-minute reuse of a successful refresh. Sidebar refresh and **Refresh likes** bypass that reuse. Likes and playlists have independent error/readiness states; ordinary refresh errors preserve the last successful section instead of displaying an empty library. Reconnect is required for an expired session.
- Rust owns in-memory account metadata, request serialization, revision/cancellation checks, continuation tokens, and playlist browse parameters. Sign-out/account switching clears this cache; late replies cannot populate another account's state. Frontend snapshots/events have both account and library revisions.
- Bounds are 10,000 liked IDs, 1,000 saved playlists, 200 continuation pages, and a two-minute timeout per library section. Opened playlists load incrementally, up to 10,000 tracks/200 pages. Exceeding bounds or a malformed/repeated cursor is an error, not a silently complete library. Unavailable liked IDs are retained even if their rows cannot be played.

Protocol references: upstream [library reads/rating](https://github.com/sigma67/ytmusicapi/blob/main/ytmusicapi/mixins/library.py) and [continuations](https://github.com/sigma67/ytmusicapi/blob/main/ytmusicapi/continuations.py). Authenticated HTTP tests use dummy cookies and loopback responses; WebKit tests use mocked IPC. Neither establishes live account-library acceptance. With a real verified account, compare likes and playlists to YouTube Music, like/unlike in both apps and refresh/return, then sign out and confirm account metadata clears while guest listening/history remain available. Real Google/Premium limits above still apply.
