# Account playlists and song lyrics

## Make a playlist

Choose **Create playlist** in the sidebar. Enter a name, optionally a description, and choose Private, Unlisted, or Public. **Private is the default.** Vessel creates the playlist in the connected YouTube Music account, then shows it in the sidebar and opens it. This is not a separate local playlist that needs exporting.

Choose **Add to playlist** beside a song in search, a collection, or recent listening, or use the player's list-plus button. Pick one of your editable playlists, or choose **New playlist** to create one containing that song in a single request. Saved playlists belonging to other people remain browsable but are not offered as write destinations.

The provider's library cards identify owned playlists through their playlist-editor endpoints. Raw browse parameters and authentication details remain in Rust. Refreshing the library or returning to Vessel reads account changes back from YouTube Music; reopening a playlist reads its current contents. A successful addition to the currently displayed playlist reloads it.

### Write behavior

- A verified connected account is required. Guests get a Connect Google prompt.
- Names are 1–100 characters; descriptions are at most 5,000 characters. Unsupported name characters and malformed playlist/song identifiers are rejected.
- Creation requires an acknowledged valid playlist ID. Additions require `STATUS_SUCCEEDED`; confirmation/sign-in gates and failed or missing statuses are errors.
- Acknowledged playlists appear immediately. Buttons stay busy until the response; failed writes keep the dialog open with an error.
- Writes are never automatically retried, including after an uncertain network outcome. Check the playlist before retrying; a request may have reached YouTube Music even when its response did not arrive. Duplicate-song rejection is left to the provider rather than silently inserting another copy.
- Account revisions, cancellation, and serialized library writes prevent late replies from filling a different or signed-out account. Existing local files and preferences are preserved.

## Read lyrics

Play a song, then choose **Lyrics** in the player. A scrollable window displays available song text, line breaks, and the provider's source credit. It follows the current playing/paused song, ignores late replies from previous songs, and does not reload on position updates. Escape, the close button, and a click outside dismiss it; keyboard focus returns to its opener.

Lyrics load on demand through Rust from the song's YouTube Music `next` lyrics tab and its `browse` response. Disabled/missing lyrics have an explicit unavailable message; transport/changed-response failures offer retry. Plain lyrics and timed-response text are supported; this is a reading window, not synchronized karaoke highlighting. No external lyric-search service or generated text is used.

Provider browse identifiers remain private. The in-memory cache holds at most 16 song/account-revision results for five minutes, with a 64 KiB text bound. There is no idle lyrics poll or lyrics file on disk. Account changes invalidate the cache; opening the window again can reuse a current cached result. Dialogs share appearance tokens, blur the inert interface while open, and respect opaque/reduced-transparency settings.

## Verification scope

Rust loopback fixtures exercise real authenticated request construction, owned versus read-only destinations, creation with an initial song, addition acknowledgements/rejections, no automatic retry, validation, stale-account cancellation, and guest rejection. Lyrics fixtures cover text/credit extraction, unavailable/malformed responses, bounded text, caching, and private browse data staying out of public results.

Browser checks cover creating/adding songs, visibility choices, guest/error/busy states, late account replies, song changes during lyrics loading, retry/unavailable states, focus restoration, outside dismissal, scrolling, and 420px layouts. These use mocked IPC. A separate live guest test retrieved provider lyrics with attribution; it printed only a line count, not the song text.

**Real-account creation/addition and cross-app reconciliation need owner verification.** No real Google credentials or account writes were used for the automated checks. See [verification](verification.md) for measured results and native package coverage.

Protocol reference: the upstream [ytmusicapi playlist methods](https://github.com/sigma67/ytmusicapi/blob/main/ytmusicapi/mixins/playlists.py), [owned-card parsing](https://github.com/sigma67/ytmusicapi/blob/main/ytmusicapi/parsers/browsing.py), and [lyrics lookup](https://github.com/sigma67/ytmusicapi/blob/main/ytmusicapi/mixins/browsing.py). ytmusicapi is a reference, not a bundled service or new runtime dependency.
