# Windows x64 beta

The first Windows package targets Windows 10/11 x64 and uses a native MSVC build, WebView2, and a per-user NSIS installer. The installer includes the playback/extractor runtimes and the Microsoft WebView2 bootstrapper. A missing WebView2 runtime is installed automatically; no manual Python, mpv, or Deno setup is required.

## Build

On Windows with x64 Node.js, Rust/MSVC, the Windows SDK, and the built-in `tar.exe`:

```sh
npm ci
npm run package:windows
```

`native-dependencies.windows.json` pins the exact Windows LGPL libmpv, yt-dlp, and Deno downloads and their SHA-256 hashes. Preparation verifies Windows x64 PE headers, the GPL-disabled libmpv configuration, and absence of explicit GPL/nonfree FFmpeg enable flags. The selected upstream LGPL build's provenance is recorded; this is not a complete independent transitive-license audit.

The output is `.cache/releases/v0.1.0-beta.14/Vessel-0.1.0-beta.14-windows-x64-setup.exe`, accompanied by `SHA256SUMS-Windows` and `WINDOWS-BUILD.json`. These are generated artifacts and are not committed.

## Platform behavior

- Rust loads `runtime/libmpv-2.dll`, `yt-dlp.exe`, and `deno.exe` from the installed application, including paths with spaces. The Windows backend uses standard mpv/WASAPI software volume with the bounded audio buffer; the macOS-specific output-gain extension is not required by the Windows DLL.
- Windows System Media Transport Controls route previous/next and play/pause to the Rust queue/player. Support can be unavailable on Windows editions without the required media components; the on-screen player remains usable.
- The main window and WebView2 background are transparent. Native material is selected from the actual OS build: **Blur on Windows 10**, Mica on Windows 11 builds 22000–22620, and modern DWM Acrylic on build 22621+. Selection runs at startup and after runtime transparency changes; the config no longer forces legacy Acrylic first. Windows' own transparency-effects preference still governs the OS effect. CSS tint, light/dark choice, glyphs, and saved appearance values retain their existing rules.
- Remembered account records are encrypted for the current user with Windows DPAPI and atomically stored under Vessel's application-data directory. They are never saved as plaintext session JSON. Sign-out removes the protected record. The isolated Google browser window remains subject to Google's embedded-login policies.
- Extractor processes start without flashing console windows. A per-request Windows job object terminates the extractor and its transient runtime children on cancellation/timeout.
- Installed data and appearance preferences are separate from the installer directory. The installer is per-user and the beta is not Authenticode-signed.

## Native verification

`.github/workflows/windows-build.yml` builds on a Windows runner. It checks the frontend and Rust code, runs real DPAPI/cancellation filesystem/kernel tests, opens an actual WebView2 window and invokes Rust from the mounted frontend in isolated diagnostic mode, then builds the normal installer. The installer test performs a silent install to a path with spaces, validates the installed runtimes, exercises local muted/null-output decoding and volume through the installed DLL, launches/closes the normal installed application, and uninstalls it.

The strengthened WebView2 diagnostic reads the real controller background alpha and the available DWM/legacy accent material attributes, checks build-specific selection and remove/reapply behavior, and checks clean dark-mode text declarations. Unsupported readbacks remain null and cannot pass as a verified zero. A separate InPrivate browser fixture sets a dummy YouTube authentication cookie through Rust, reads it through the production cookie APIs, and exercises candidate eligibility while the top-level Google origin is substituted. It closes through the sign-in controller after a substituted provider acknowledgement. It does not use real Google credentials or mark the fixture account Connected.

### Windows 10 follow-up (beta.14)

The owner reports Windows 10 beta.13 dragging delay, YouTube Music stuck on Connecting, and a Connected SoundCloud account whose likes/playlists do not load. Windows 10's old Acrylic path matches the [upstream documented dragging/resizing performance issue](https://github.com/tauri-apps/window-vibrancy#available-functions); `windows_material.rs` now chooses Blur there. There is no measured physical Windows 10 drag latency from this macOS development host, and the Windows Server 2025 CI runner exercises the modern compositor path. The owner must retest real dragging on Windows 10.

Native YouTube candidates can now be checked while Google remains visible, with an authentication-cookie baseline so unchanged remembered credentials/tracking changes do not interrupt account choice. Every candidate still needs YouTube Music's account-menu verification. Once candidate verification begins, its 45-second deadline survives temporary cookie disappearance. The overall time allowed for entering credentials/2FA remains separate.

SoundCloud publishes each fully fetched library section as soon as it completes, so a slow playlist request does not withhold ready likes (and vice versa). Failed sections retain their previous good data and show a redacted HTTP status when available. Late results cannot repopulate a forgotten account. These fixes have transport/behavior regressions; successful live Windows account acceptance/sync still needs the owner's beta.14 retest.

[Windows CI 40961383780](https://github.com/YuriiPtyts/vessel-app/actions/runs/40961383780), built from `3046d78`, passes the strengthened native/installer checks, 75 ordinary library tests, three ordinary binary tests, three diagnostic library tests, and five feature-enabled binary tests. Its **Server 2025 / build 26100** main window uses modern Acrylic with actual DWM type 3; the native handoff uses real scoped cookies with a substituted Google origin/provider acknowledgement. The **85,387,666-byte** installer matches its SHA-256 sidecar. This is not a Windows 10 physical drag or real-account acceptance measurement. The checked installer is attached to the private [beta.14 release](https://github.com/YuriiPtyts/vessel-app/releases/tag/v0.1.0-beta.14).

The null-output decode check does not certify WASAPI on physical speakers, audible volume latency, media-key delivery, real Google credentials, or personalized/Premium playback. Those require owner testing on Windows hardware. See [verification](verification.md) for actual run results and [native dependencies](native-dependencies.md) for provenance and distribution scope.

### Verified beta.3 build

[Native Windows CI run 40607451788](https://github.com/YuriiPtyts/vessel-app/actions/runs/40607451788) completed successfully on Windows Server 2025 x64. It passed 35 library tests plus the IPC-origin and non-network integration checks, including real DPAPI round-trip/corruption preservation and job-object cleanup. Actual WebView2/native IPC and SMTC availability were confirmed. Silent install, installed extractor/Deno execution, installed DLL null-output decoding/volume, an eight-second normal app launch, normal close, and silent uninstall passed.

The downloaded installer is **85,032,286 bytes**, and its SHA-256 matches `SHA256SUMS-Windows`. It is unsigned. The run used fixture/local audio and no real account credentials; the hardware/provider limits above still apply.

### Beta.4 readability update

[Native Windows CI run 40619153912](https://github.com/YuriiPtyts/vessel-app/actions/runs/40619153912) passes the same native verification for the glass-readability build, including actual WebView2/IPC/SMTC, installed-runtime checks, normal app close, and uninstall. The Windows setup is **85,026,440 bytes** with a verified `SHA256SUMS-Windows` sidecar. The installer fixture now synchronizes with NSIS's asynchronous cleanup; the physical-audio/account scope is unchanged.

### Beta.5 panel transparency update

[Native Windows CI run 40623508110](https://github.com/YuriiPtyts/vessel-app/actions/runs/40623508110) is green for the shared Appearance/Queue tint update, including the normal installed application and uninstall checks. The Windows setup is **85,043,526 bytes**, with a verified SHA-256 sidecar. The panel alpha/contrast regression is part of the 24 mocked-WebKit UI checks; hardware/compositor and real-account scopes retain the limits above.

### Beta.7 sidebar and shared material update

[Native Windows CI run 40696357557](https://github.com/YuriiPtyts/vessel-app/actions/runs/40696357557), from `aa6c7f8`, passes frontend/Rust checks, 46 library tests, actual WebView2/native commands/media-control availability, and the production installer/runtime/launch/quit/uninstall checks. The downloaded installer is **85,137,065 bytes** and matches its SHA-256 sidecar. Eleven focused Chromium fixture tests also check the shared sidebar/material/contrast behavior; these ran on the macOS development host, separately from the native Windows installer checks.

### Beta.8 independent appearance and stable menus

[Native Windows CI run 40735439125](https://github.com/YuriiPtyts/vessel-app/actions/runs/40735439125), from `23c33e0`, passes frontend/Rust checks, 46 library tests, actual WebView2/native IPC/SMTC, production install, installed-runtime decoding/volume, normal launch/quit, and uninstall. The downloaded setup is **85,117,159 bytes** and matches its SHA-256 sidecar. The 28 focused Chromium fixture tests ran on the macOS development host and are separate from native Windows pixel/compositor testing.
