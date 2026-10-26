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

1. **Connect Google** opens Google's own HTTPS page in an ephemeral native WebView. It has no Vessel command access and can navigate only to the permitted Google/YouTube account origins. Vessel has no password form and does not inject credential-extraction JavaScript.
2. After the window returns to YouTube Music, Rust reads its cookie store off the main thread and retains only validated YouTube-domain cookies. Anonymous or incomplete cookies are not saved.
3. Rust verifies the session against YouTube's account-menu endpoint. A real account header is required before the public state becomes Connected. Only public name/email/artwork and status cross IPC.
4. A verified session is saved as a generic password in macOS Keychain under service `app.vessel.desktop.google-session`, entry `youtube-music`. Startup re-verifies the stored session. Cancelled vault mutations finish/roll back in a serialized task instead of abandoning an OS write.
5. Session revisions partition catalog/context/source caches. An expired authenticated request removes its active request context and offers reconnect. Account-backed playback is stopped on expiry. **Sign out / Forget saved session** removes the Keychain entry, cancels session work, closes the private window, and stops playback. Local likes, history, queue, and appearance remain.

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
VESSEL_SIGN_IN_DIAGNOSTICS="$TMPDIR/vessel-sign-in.json" VESSEL_RUNTIME_DIR="$PWD/src-tauri/runtime" src-tauri/target/release/vessel-app -ApplePersistenceIgnoreState YES -NSQuitAlwaysKeepsWindows NO
node scripts/check-native-sign-in.mjs "$TMPDIR/vessel-sign-in.json"
```

The opt-in diagnostic opens and cancels the real Google window without entering credentials. Its report contains booleans only, without account profiles, cookie values, or page text. Normal builds do not run it.

### Owner account verification

In the packaged app, open Account and connect using Google's own window. Confirm that Connected appears only after verification, Home changes to Your feed, restart restores the verified session, and sign-out returns to guest recommendations while preserving local music/appearance. Exercise the account's normal second factor and an account-required song, if available. If Google refuses the embedded window, record the displayed reason; a successful window smoke test is not evidence that Google accepts a real login.

References: [ytmusicapi browser authentication](https://ytmusicapi.readthedocs.io/en/stable/setup/browser.html), [OAuth setup/client requirements](https://ytmusicapi.readthedocs.io/en/stable/setup/oauth.html), and Tauri 2.12.5's native cookie/incognito APIs.
