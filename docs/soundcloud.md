# SoundCloud mode and independent accounts

## Switching services

The single two-position button at the top is anchored to the **horizontal centre of the entire window**, with search immediately to its right on desktop widths. Click once to move between YouTube Music and SoundCloud. The selection slides for 280ms; reduced motion makes it immediate. At narrow preview widths, search moves to a second row while the switch remains centred.

Switching pauses the outgoing service and shows the incoming service's own catalog, account, history, queue, and selection. Returning keeps the previous song paused at its position so it can be resumed. Only the selected provider can start playback; the player actor has a pause barrier for a load already in progress. Both accounts remain connected.

The selection is Rust-owned and saved in `music-service.json`. Existing YouTube `collection.json` and `queue.json` are preserved. SoundCloud uses `soundcloud-collection.json` and `soundcloud-queue.json`. SoundCloud track IDs use `sc:<numeric id>` so IDs cannot collide with YouTube video IDs. Provider-tagged events and frontend request generations reject results from the other service.

Each service has an independent application/player context. An inactive player's engine may remain paused after it has been used; at most two contexts exist. This favors retaining position/queue state, and is not a claim of a completed whole-process memory/power budget.

## Connecting SoundCloud

1. Switch to **SoundCloud**.
2. Open Account and choose **Connect SoundCloud**.
3. Sign in on SoundCloud's own page in the separate native window.
4. Rust reads SoundCloud-scoped session cookies and verifies the account with SoundCloud before displaying Connected or saving it.

YouTube Music does not sign out. The header can show connected indicators for both services; Account identifies when the other service is still connected. Signing out/forgetting affects the currently selected service only.

SoundCloud has its own named WebKit store on supported Macs and its own InPrivate WebView2 data-directory namespace on Windows. It does not share Google's browser profile or the main settings store. Remembered sessions use the separate `app.vessel.desktop.soundcloud` / `verified-session` Keychain entry on macOS, or the current-user DPAPI-protected `soundcloud-session.dpapi` record on Windows. Its vault transactions are serialized and cancelled writes roll back. The SoundCloud window and authentication popups cannot use privileged Vessel commands.

This beta uses **SoundCloud's website compatibility API** (`api-v2.soundcloud.com`), its public browser client identifier, and the user's verified website session. It does not ship an application secret or claim an officially registered SoundCloud OAuth application. SoundCloud's [official API flow](https://developers.soundcloud.com/docs/api/guide) is a separate client-registration/OAuth integration; [yt-dlp's SoundCloud extractor](https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/extractor/soundcloud.py) is a compatibility reference. Website formats and embedded sign-in acceptance can change.

## Catalog and playback

- Real guest audio-track search, pagination, provider-selected discovery shelves, public playlist browsing, related tracks, and native libmpv playback are implemented in Rust.
- Account likes, owned playlists, explicit like/unlike, playlist creation, and additions use the connected SoundCloud account. Private/Public are supported; YouTube's Unlisted option is not shown for SoundCloud.
- Playlist additions read the complete current ordered ID list before writing, retain existing/unavailable IDs, reject incomplete reads, and do not automatically retry a write. The API replaces a track list; concurrent edits made in another client are not an atomic append guarantee.
- Full-playback formats are taken from real returned transcodings. Blocked/preview policies, snipped entries, and DRM-style protocols are rejected rather than presented as full playback. Available AAC/Opus/MP3 information is shown without invented sample-rate/channel values or lossless claims.
- The player credits SoundCloud and the creator, with an action to open the validated original/creator page. Signed stream URLs, API client data, continuation URLs, and authentication headers stay in Rust. OAuth headers are never passed to audio CDN sources.
- SoundCloud lyrics are not supplied by this integration; its Lyrics button is disabled with an explanation. YouTube Music's existing lyrics behavior remains available in its mode.

Bounds include 25 search results per request / 100 displayed, 10,000 likes/playlist tracks, 1,000 playlists, 200 library pages, two-minute section loading, bounded response bodies and script discovery, a 15-minute browser-client cache, and up to eight one-minute stream-cache entries. There is no new permanent Python/Node server or extra playback runtime.

## Verification and owner checks

Transport fixtures cover scoped request construction, authentication, stream/header privacy, playlist read/merge/write behavior, incomplete-read/failed-write rejection, token/cookie scope, independent vaults, cancellation rollback, and persisted service choice. Browser fixtures cover exact centring, search placement, animation/reduced motion, both logins, scoped sign-out, separate histories/queues, late replies/events, and SoundCloud playlist visibility.

The opt-in native check creates two isolated fixture browser windows, uses dummy cookies through the real Tauri APIs, confirms separate stores and unchanged accounts, and closes both without connecting an unverified account:

```sh
npm run tauri -- build --no-bundle --features native-diagnostics
VESSEL_PROVIDER_DIAGNOSTICS="$TMPDIR/vessel-providers.json" VESSEL_RUNTIME_DIR="$PWD/src-tauri/runtime" src-tauri/target/release/Vessel
node scripts/check-native-providers.mjs "$TMPDIR/vessel-providers.json"
cargo test --manifest-path src-tauri/Cargo.toml --lib live_soundcloud_guest_search_and_native_playback -- --ignored --nocapture
```

**Real SoundCloud password/2FA/federated-login acceptance, remembered real-session recovery, account likes/playlists/writes, Go+ access, and cross-app reconciliation still need owner verification.** Check that both accounts stay connected after switching/restarting and that signing out of one leaves the other intact. Automated native checks use isolated profiles and dummy cookies, never real credentials. See [verification](verification.md) for actual results.

SoundCloud and YouTube Music marks use [Simple Icons](https://simpleicons.org/) paths (CC0); the brands remain their owners' trademarks. No Simple Icons runtime dependency is added.
