# Bundled playback dependencies

Pinned, checksum-verified artifacts/source are bundled in the app. Preparation runs on the developer/build machine only. End users do not install tools. Native binaries, source archives, build virtual environments, and caches are ignored by Git.

## libmpv 0.41.0-vessel.2 (CoreAudio cleanup and live output gain)

- Preparation now builds the arm64 engine from the exact MPVKit/mpv/FFmpeg source snapshots and static dependency archives pinned in `native/mpv-build.json`. Every downloaded input, including the private build environment's Meson wheel, is SHA-256 checked before use.
- `native/patches/mpv-coreaudio-cleanup.patch` backports upstream mpv commit `c5d391adba7bd024954d0df1e0405f5749f4d4ca` (PR #18383). Device listeners are registered only after successful AudioUnit initialization; partial initialization is cleaned up. The v0.41.0 helper's disposed AudioUnit pointer is cleared to avoid double disposal during the backported cleanup.
- `native/patches/mpv-output-gain.patch` adds a per-handle `vessel-output-volume` property. Rust maps the established cubic loudness curve onto CoreAudio's per-AudioUnit gain or AVFoundation's per-renderer volume, leaving software PCM gain at unity. The target can be set before an output exists and is restored before every new output starts, including mute and device/format recreation. No system-wide device volume is changed. This removes the reproduced roughly 1.8-second AVFoundation queue delay.
- `scripts/build-mpv.mjs` and `native/patches/mpvkit-library-build.patch` keep GPL/nonfree flags off, omit unused SMB/Blu-ray/OpenSSL inputs and CLI players, and link the static dependencies into a replaceable LGPL-3.0-or-later dynamic library. Only system dynamic dependencies are accepted. Runtime replacement is atomic; build provenance is bundled in `runtime/libmpv-build.json`.
- Developer/build requirements: Xcode, Python 3, pkg-config, Ninja, and CMake. Meson is installed from its pinned wheel into the ignored build cache. The recipe does not install global packages. These tools are **not** needed by end users.
- Source and compiled caches live in `.cache/native/`; the compiled-file checksum and recipe hash are checked on reuse. The normal macOS bundle contains the resulting engine, without a source build at launch.
- The previous baseline was https://github.com/studiolxd/zapper-libmpv/releases/tag/v0.41.0-1. Its LGPL build strategy is retained, with the missing CoreAudio fix now tracked in this repository.
- Upstream licensing: https://github.com/mpv-player/mpv/blob/v0.41.0/Copyright
- Rejected candidate: kirbylink/mpv-natives v0.41.0 declares self-contained binaries, but `otool -L` and a real `dlopen` showed Homebrew libxcb/jpeg dependencies in its arm64 slice. Do not reuse it without resolving the closure.

The host remains macOS 27.0.1 arm64. Minimum-OS/clean-machine testing remains a release gate; the build linker reported macOS-14 deployment metadata on some prebuilt AVS3 assembly objects, despite the general engine target. No minimum-OS certification is inferred from the host checks.

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

## Windows x64 runtime

- Windows preparation uses the separate SHA-256-pinned `native-dependencies.windows.json` manifest. It selects zhongfly/mpv-winbuild's general x86-64 **LGPL** development library from `2026-10-27-ad5877edab` (not the AVX2/v3 or GPL build), upstream yt-dlp `2026.10.21`, and Deno `2.9.8` for Windows x64.
- The engine is loaded as `libmpv-2.dll`; the upstream Windows DLL does not contain Vessel's macOS output-gain extension. Windows uses the standard mpv volume/WASAPI path. The build checks native mpv configuration and installed-library decoding in addition to archive hashes.
- The extractor executable is a separate process and bundles its Python runtime. The Windows installer also handles the Microsoft WebView2 runtime. Build provenance and upstream licensing references are retained with the bundled runtime. See [Windows builds](windows-build.md).

## Before public distribution

These references record provenance; they are not a complete redistribution package. Include full license texts, transitive attribution, corresponding source/build materials or the required source offer, and verify all applicable artifact-specific obligations before publishing downloads. Exercise signing/notarization and user-replaceability of the LGPL library. The current local spike is not a distribution-ready release.
