# Playback spike dependencies

These are pinned, checksum-verified development artifacts, bundled with the local feasibility app. End users must not need to install tools. The preparation script runs on the developer/build machine only. Downloaded binaries and archives are ignored by Git.

## libmpv 0.41.0 (zapper build v0.41.0-1)

- Artifact and corresponding source: https://github.com/studiolxd/zapper-libmpv/releases/tag/v0.41.0-1
- Build recipes and dependency pins: https://github.com/studiolxd/zapper-libmpv/tree/v0.41.0-1
- Recipe inspected: macOS builds use mpv `-Dgpl=false` and FFmpeg without `--enable-gpl` or `--enable-nonfree`; dependencies are linked into a replaceable dynamic library. The combined library uses LGPL-3.0-or-later.
- Upstream licensing: https://github.com/mpv-player/mpv/blob/v0.41.0/Copyright
- Rejected candidate: kirbylink/mpv-natives v0.41.0 declares self-contained binaries, but `otool -L` and a real `dlopen` showed Homebrew libxcb/jpeg dependencies in its arm64 slice. Do not reuse it without resolving the closure.

## yt-dlp 2026.10.21

- Artifact/source: https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.21
- Standalone macOS release contains GPL-licensed components and is distributed under GPLv3+. It is invoked as a separate process, not linked into Vessel.
- License and dependency notices: https://github.com/yt-dlp/yt-dlp/blob/2026.10.21/THIRD_PARTY_LICENSES.txt
- Full YouTube support uses the bundled EJS solver plus a JavaScript runtime. No permanent extraction daemon is introduced.

## Deno 2.9.8

- Artifact/source: https://github.com/denoland/deno/releases/tag/v2.9.8
- License: https://github.com/denoland/deno/blob/v2.9.8/LICENSE.md
- Used only while resolving streams/challenges. It is not an application server or background service.

## Before public distribution

These references record provenance; they are not a complete redistribution package. Include full license texts, transitive attribution, corresponding source/build materials or the required source offer, and verify all applicable artifact-specific obligations before publishing downloads. Exercise signing/notarization and user-replaceability of the LGPL library. The current local spike is not a distribution-ready release.
