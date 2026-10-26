# Confirmed product decisions

Recorded 2026-10-26 from the owner's answers.

## Search-first product correction

The owner explicitly rejected the link-inspection lab as the main experience. Normal listening must begin with music search and direct playback, presented as a finished music-app layout. Keep extraction and format inspection behind the interface. Current navigation is Listen now, Search, local Liked songs, and Recently played, with a persistent player. Account support remains optional and an early priority; local saved music must be clearly distinguished from the account library.

## Confirmed

1. Polish macOS first. Add other operating systems in future milestones.
2. Prioritize account support early, while preserving an explicit guest listening path. Account failure or sign-out must not erase the local queue or disable otherwise available guest capabilities.
3. Reliable audio-only playback, seeking, editable queues, and media keys form the playback baseline. libmpv is the provisional engine, not a completed feasibility decision.
4. Include quality selection. Actual formats, codecs, and account/source availability determine the options. Lossless is conditional on a verified source, not a launch promise.
5. Keep the code and repository private. Do not publish or change visibility unless the owner explicitly asks.
6. Distribute self-contained downloads. End users must not install Python, Homebrew, yt-dlp, mpv, or other developer tools themselves.
7. Lead with low resource use and reliable everyday listening, supported by premium native design and preservation of listening context.

## Provisional defaults requiring technical validation

- First development/release architecture: Apple Silicon, matching the available development machine. Intel support and minimum macOS version are not yet product commitments.
- The initial build declares macOS 13.3 / Safari 16.4 as its provisional floor because Tailwind 4 requires Safari 16.4. This has not been tested on that OS version; minimum-version testing remains a release gate. Source: https://tailwindcss.com/docs/compatibility.
- Repository and working product name: Vessel / vessel-app, derived from the workspace. Repository owner will follow the authenticated GitHub account unless specified otherwise.
- Direct downloads first. Signing/notarization and an updater are release requirements, not completed setup.
- Account milestone begins with library reads; mutations follow separately.

## Audio quality evidence

Google's current help page describes low/normal/high streaming using AAC and Opus, with high listed up to 256 kbps. It does not establish lossless availability. These are descriptions of Google's application, not proof of the formats our integration can access.

Source: https://support.google.com/youtubemusic/answer/9076559?hl=en (consulted 2026-10-26).

Policy:

- Offer Auto and only supported quality choices after format discovery.
- Show the active codec and available bitrate information; unknown values remain unknown.
- Do not compare codecs solely by nominal bitrate.
- Explain a fallback when the preferred format becomes unavailable.
- Decoding lossy audio to PCM or transcoding to FLAC does not make the source lossless.
- A future lossless badge requires verified source codec/provenance, not a filename, container, or output-device sample rate.
