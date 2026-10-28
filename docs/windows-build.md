# Windows x64 beta

The first Windows package targets Windows 10/11 x64 and uses a native MSVC build, WebView2, and a per-user NSIS installer. The installer includes the playback/extractor runtimes and the Microsoft WebView2 bootstrapper. A missing WebView2 runtime is installed automatically; no manual Python, mpv, or Deno setup is required.

## Build

On Windows with x64 Node.js, Rust/MSVC, the Windows SDK, and the built-in `tar.exe`:

```sh
npm ci
npm run package:windows
```

`native-dependencies.windows.json` pins the exact Windows LGPL libmpv, yt-dlp, and Deno downloads and their SHA-256 hashes. Preparation verifies Windows x64 PE headers, the GPL-disabled libmpv configuration, and absence of explicit GPL/nonfree FFmpeg enable flags. The selected upstream LGPL build's provenance is recorded; this is not a complete independent transitive-license audit.

The output is `.cache/releases/v0.1.0-beta.7/Vessel-0.1.0-beta.7-windows-x64-setup.exe`, accompanied by `SHA256SUMS-Windows` and `WINDOWS-BUILD.json`. These are generated artifacts and are not committed.

## Platform behavior

- Rust loads `runtime/libmpv-2.dll`, `yt-dlp.exe`, and `deno.exe` from the installed application, including paths with spaces. The Windows backend uses standard mpv/WASAPI software volume with the bounded audio buffer; the macOS-specific output-gain extension is not required by the Windows DLL.
- Windows System Media Transport Controls route previous/next and play/pause to the Rust queue/player. Support can be unavailable on Windows editions without the required media components; the on-screen player remains usable.
- Remembered account records are encrypted for the current user with Windows DPAPI and atomically stored under Vessel's application-data directory. They are never saved as plaintext session JSON. Sign-out removes the protected record. The isolated Google browser window remains subject to Google's embedded-login policies.
- Extractor processes start without flashing console windows. A per-request Windows job object terminates the extractor and its transient runtime children on cancellation/timeout.
- Installed data and appearance preferences are separate from the installer directory. The installer is per-user and the beta is not Authenticode-signed.

## Native verification

`.github/workflows/windows-build.yml` builds on a Windows runner. It checks the frontend and Rust code, runs real DPAPI/cancellation filesystem/kernel tests, opens an actual WebView2 window and invokes Rust from the mounted frontend in isolated diagnostic mode, then builds the normal installer. The installer test performs a silent install to a path with spaces, validates the installed runtimes, exercises local muted/null-output decoding and volume through the installed DLL, launches/closes the normal installed application, and uninstalls it.

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
