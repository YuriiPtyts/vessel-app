# Vessel 0.1.0-beta.14

Private Apple Silicon macOS and Windows x64 development beta. This is a prerelease, not a stable release.

## Included changes

- Windows 10 uses native Blur instead of the legacy Acrylic path associated with dragging/resizing stalls. Build-specific material selection is shared by startup and runtime changes; early Windows 11 uses Mica and newer builds retain modern Acrylic. Physical Windows 10 dragging still needs the owner's retest.
- YouTube Music can verify a new native YouTube session while Google remains visible. Remembered-credential baselines avoid interrupting account choice, and verification's deadline no longer resets when a cookie candidate temporarily disappears. The real Windows Google login still needs the owner's retest.
- SoundCloud publishes ready likes and owned playlists independently, preserves good sections after failures, fences late sign-out results, and displays redacted HTTP status errors. Connected-account fixtures and public provider reads verify these code paths; the owner's Windows library synchronization remains a hands-on check.
- Retains beta.13's opt-in native SoundCloud form-rendering and post-submit handoff diagnostics, with isolated browser/application/vault state and redacted reports. See [login diagnostics](soundcloud.md#optional-native-login-diagnostics-beta13). The provider switch, playback, and account features below are retained from beta.12.
- SoundCloud mode with real guest search/discovery, playlist browsing, related tracks, native audio, source attribution, and account likes/owned playlist actions.
- A single smooth sliding YouTube Music/SoundCloud switch at the exact horizontal window centre, with search to its right on desktop widths. Switching pauses the outgoing service and retains independent queues/history.
- Independent SoundCloud sign-in browser and protected session storage. Both services can stay connected; signing out of one does not forget the other. See [SoundCloud scope and verification](soundcloud.md).
- Crisp dark-theme lettering at low tint: removed the dark contour/halo, forced-white fill, and extra bolding. Text retains normal system-font weights and semantic primary/secondary colors.
- Apple's native Regular Liquid Glass supports the text-heavy macOS interface. Floating surfaces also occlude sharp underlying content so popover labels do not compete with letters behind them. Material alpha still follows the selected tint. See [material-supported typography](glass-readability.md#material-supported-typography).
- Create playlists in your verified YouTube Music account, with Private as the default visibility. Add songs from track rows or the player to your own playlists, or create a playlist containing the chosen song. Provider-confirmed changes appear in the sidebar; failures do not claim success or retry automatically.
- A scrollable Lyrics window follows the playing song and displays available YouTube Music text and source credit, with clear unavailable/retry states. See [playlists and lyrics](playlists-and-lyrics.md).
- Windows now requests a genuinely transparent native window/WebView2 surface and Acrylic backdrop. Reduced-transparency mode clears the native effect while the selected CSS material stays solid.
- Google handoff accepts trusted YouTube completion hosts and the YouTube consent redirect, rather than waiting only for Music. Scoped authentication-cookie candidates no longer require `LOGIN_INFO`; the server must still verify the account before activation/storage. Pending network verification retries with a deadline instead of leaving an indefinite handoff.
- Separate Light/Dark buttons and a top-bar sun/moon shortcut. Glass tint changes transparency only; high transparency strengthens glyphs without recoloring surfaces or icons. Existing appearance choices are migrated without resetting other settings.
- Queue, quality, account, and appearance menus close on an outside click or Escape. Inside clicks stay inside, trigger toggles work normally, and the outside control still receives its click.
- Fixed the opening flash and transparent player edges by replacing loaded mask images with live inline vector clips. Appearance's background stays in place while settings scroll.
- Liked songs now uses YouTube Music only. Device likes navigation and guest-local heart actions were removed; guest playback and recent listening remain available.
- Saved playlists now scroll in their own sidebar area; Listen now, Search, Appearance, and Account stay reachable with long libraries.
- One material tint across the sidebar, main content, top bar, player, artwork area, and popovers. Floating materials replace the background beneath them instead of doubling its opacity.
- Account likes read from YouTube Music and hearts write back to the verified account. Saved account playlists appear in the sidebar with pagination. Refresh from the sidebar/likes view or return to the app (one-minute refresh reuse). See [account library behavior](radio-and-accounts.md#account-library-sync-2026-10-28).
- Glass readability uses semantic text/error contrast, visible selection borders, readable queue history, and OS increased-contrast support. Extra opaque reading patches have been removed; 100% tint and reduced-transparency mode stay opaque. See [glass readability](glass-readability.md).
- Windows x64 NSIS installer with pinned Windows playback/extraction runtimes, WebView2 setup, DPAPI-protected remembered sessions, native Windows media controls, and bounded extractor process cleanup. See [Windows builds](windows-build.md).
- New Rust-owned per-player output gain applies after queued PCM, removing the reproduced AVFoundation volume delay. The established loudness curve, mute restoration, and gain across output recreation are preserved.
- Native macOS Now Playing previous/next and play/pause handlers, independent of renderer keyboard focus. The installed process is named **Vessel** for Neohtop/Activity Monitor; older builds and Cargo development runs use `vessel-app`.
- Balanced 220 ms reveals for appearance, account, queue, audio-quality, and now-playing panels. Uniform scaling replaces the stretched genie; the material remains steady while content fades. Closing is immediate and reduced motion is respected. See [motion behavior](motion.md).
- A rebuilt arm64 libmpv engine with the targeted CoreAudio initialization-failure cleanup fix. The deterministic regression verifies that failed initialization leaves no device callbacks behind. See [the crash investigation](audio-device-investigation.md).
- Remembered Google sign-in data restored through macOS Keychain, with server verification required before authenticated requests resume and explicit forgetting on sign-out.
- Native radio/queue management, faster guest playback, responsive/coalesced volume controls, and readable 50–100% glass tint. Existing likes, history, queue, and appearance data are retained.

## Install the candidate

### macOS

Download `Vessel-0.1.0-beta.14-macos-arm64.dmg` from the private release, open it, quit any older Vessel instance, and drag **Vessel.app** onto the **Applications** shortcut. Eject the Vessel disk after copying, then launch the app from Applications. The image is read-only and includes the playback and extractor runtimes; end users do not install Python, mpv, or Deno.

### Windows

Run `Vessel-0.1.0-beta.14-windows-x64-setup.exe` and follow the per-user installer, then open Vessel from the Start menu. The installer includes the playback/extraction runtimes and handles WebView2 setup if required. Windows controls whether native transparency effects are enabled. The beta is not Authenticode-signed. `SHA256SUMS-Windows` and `WINDOWS-BUILD.json` accompany the installer. See [Windows verification scope](windows-build.md).

### First launch if macOS blocks Vessel

This beta is not notarized, so macOS may require you to approve its first launch:

1. Try opening **Vessel** from **Applications** once, then dismiss the warning.
2. Open the **Apple menu → System Settings → Privacy & Security**.
3. Scroll down to **Security**, find the message about Vessel being blocked, and click **Open Anyway** if offered.
4. Authenticate with Touch ID or your Mac password if requested, then click **Open** in the confirmation dialog.

If the button is not visible, try opening Vessel again and return to Privacy & Security. After approval, you can open Vessel normally from Applications. See [Apple's Open Anyway instructions](https://support.apple.com/en-us/102445#openanyway).

### Other downloads and signing status

The ZIP download remains available. `SHA256SUMS` covers both files; download both alongside it to run `shasum -a 256 -c SHA256SUMS`, or compare the single downloaded file with its matching line. `SIGNING.json` records the package checks and signing status.

The repacked download repairs the invalid bundle/framework signatures in the original ZIP and verifies all bundled code after extraction. It is **ad-hoc signed**, not Developer-ID signed or notarized, and Gatekeeper still rejects it as untrusted. Warning-free launch on other Macs requires the credentials and notarization workflow in [macOS distribution](macos-distribution.md). It was tested on Apple Silicon macOS 27; older macOS versions and Intel Macs have not been validated. See [native dependency provenance and distribution limits](native-dependencies.md).

## Pending physical-audio checks

The owner still needs to confirm both original symptoms against this beta:

1. While playing and while paused, switch between speakers, the reported 3.5 mm headphones, and AirPods; remove/reconnect the devices and confirm there is no crash and playback recovers. The automated initialization audit and aggregate-device stress exercise related code paths but do not certify these physical transitions.
2. During actual playback, drag volume and use mute/unmute on the affected output; the new implementation passes the native AVFoundation queue/gain regression, but physical-device/acoustic confirmation still belongs to this check. Record the output device and any remaining delay.
3. While Vessel is playing, use the keyboard's media previous/next and play/pause keys, including while another app is focused. Native registration, metadata/state, and handler routing are diagnostic-verified; actual physical key delivery is not synthesized by that check. If the keyboard sends F-keys instead, use its media/Fn mode.

If a crash recurs, use `scripts/inspect-crash.mjs` as described in the investigation; keep raw reports and private media URLs out of the repository and release assets. A distinct crash signature needs a separate diagnosis.

## Verification and remaining scope

Current commands, observed results, and the distinction between mocked UI, real native checks, and live provider checks are recorded in [the verification log](verification.md). Real Google password/2FA acceptance, real-account recovery, authenticated/Premium playback, minimum-OS coverage, and long-session resource behavior retain the documented limitations.

Release assets contain the drag-and-drop DMG, ZIP, their SHA-256 checksums, and a machine-readable signing report. Source, patch, and dependency provenance are tracked in `native-dependencies.json`, `native/mpv-build.json`, `native/patches/`, and [native dependencies](native-dependencies.md). Keep this repository and its beta private.
