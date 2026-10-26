# Bundled playback dependencies

Pinned, checksum-verified artifacts/source are bundled in the app. Preparation runs on the developer/build machine only. End users do not install tools. Native binaries, source archives, build virtual environments, and caches are ignored by Git.

## libmpv 0.41.0 (zapper build v0.41.0-1)

- Artifact and corresponding source: https://github.com/studiolxd/zapper-libmpv/releases/tag/v0.41.0-1
- Build recipes and dependency pins: https://github.com/studiolxd/zapper-libmpv/tree/v0.41.0-1
- Recipe inspected: macOS builds use mpv `-Dgpl=false` and FFmpeg without `--enable-gpl` or `--enable-nonfree`; dependencies are linked into a replaceable dynamic library. The combined library uses LGPL-3.0-or-later.
- Upstream licensing: https://github.com/mpv-player/mpv/blob/v0.41.0/Copyright
- Rejected candidate: kirbylink/mpv-natives v0.41.0 declares self-contained binaries, but `otool -L` and a real `dlopen` showed Homebrew libxcb/jpeg dependencies in its arm64 slice. Do not reuse it without resolving the closure.

## yt-dlp 2026.10.21

- Source tag: https://github.com/yt-dlp/yt-dlp/tree/2026.10.21. The complete tag archive (including build recipes) is pinned by SHA-256 in native-dependencies.json.
- Build: Python 3.14, the upstream `bundle.pyinstaller --onedir --target-architecture arm64` recipe, generated lazy extractors, and upstream hash-pinned PyInstaller/default dependency lockfiles. No global packages are installed; the build environment is cached under `.cache/native/`.
- The frozen directory runtime includes Python, standard modules, certificates, and required dependencies. It lives under `Resources/runtime/extractor/`; it is invoked as a separate process, not linked into Vessel. GPL-licensed components remain subject to their own redistribution terms.
- License and dependency notices: https://github.com/yt-dlp/yt-dlp/blob/2026.10.21/THIRD_PARTY_LICENSES.txt
- Full YouTube support uses the bundled EJS solver plus a JavaScript runtime. No permanent extraction daemon is introduced.
- The previous single-file executable took roughly 7–8 seconds merely to print its version on the host. The directory runtime measured about 0.27–0.38 seconds after first-use loading. The published macOS ZIP inspected during the spike contained dependency resources without a usable entry executable, so the app builds the upstream directory layout rather than relying on that ZIP.

## Deno 2.9.8

- Artifact/source: https://github.com/denoland/deno/releases/tag/v2.9.8
- License: https://github.com/denoland/deno/blob/v2.9.8/LICENSE.md
- Used only while resolving streams/challenges. It is not an application server or background service.

## Before public distribution

These references record provenance; they are not a complete redistribution package. Include full license texts, transitive attribution, corresponding source/build materials or the required source offer, and verify all applicable artifact-specific obligations before publishing downloads. Exercise signing/notarization and user-replaceability of the LGPL library. The current local spike is not a distribution-ready release.
