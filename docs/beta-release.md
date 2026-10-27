# Vessel 0.1.0-beta.1

Private Apple Silicon macOS beta candidate. This is a prerelease, not a stable release.

## Included changes

- Shared genie-style reveals for appearance, account, queue, audio-quality, and now-playing panels, with immediate closing and OS reduced-motion support. See [motion behavior](motion.md).
- A rebuilt arm64 libmpv engine with the targeted CoreAudio initialization-failure cleanup fix. The deterministic regression verifies that failed initialization leaves no device callbacks behind. See [the crash investigation](audio-device-investigation.md).
- Remembered Google sign-in data restored through macOS Keychain, with server verification required before authenticated requests resume and explicit forgetting on sign-out.
- Native radio/queue management, faster guest playback, responsive/coalesced volume controls, and readable 50–100% glass tint. Existing likes, history, queue, and appearance data are retained.

## Install the candidate

Download `Vessel-0.1.0-beta.1-macos-arm64.zip` and `SHA256SUMS` from the private draft release. Verify the archive with `shasum -a 256 -c SHA256SUMS`, unzip it, quit any older Vessel instance, and move `Vessel.app` to Applications. The download includes the playback and extractor runtimes; end users do not install Python, mpv, or Deno.

This build is not Developer-ID signed or notarized. It was tested on Apple Silicon macOS 27; older macOS versions and Intel Macs have not been validated. See [native dependency provenance and distribution limits](native-dependencies.md).

## Publication gate: original physical-audio reports

Keep the GitHub prerelease **in draft** until the owner confirms both original symptoms against this candidate:

1. While playing and while paused, switch between speakers, the reported 3.5 mm headphones, and AirPods; remove/reconnect the devices and confirm there is no crash and playback recovers. The automated initialization audit and aggregate-device stress exercise related code paths but do not certify these physical transitions.
2. During actual playback, drag volume and use mute/unmute; confirm that the audible gain follows promptly, not just the thumb or native command acknowledgement. Record the output device and any remaining delay.

If a crash recurs, use `scripts/inspect-crash.mjs` as described in the investigation; keep raw reports and private media URLs out of the repository and release assets. A distinct crash signature needs a separate diagnosis.

## Verification and remaining scope

Current commands, observed results, and the distinction between mocked UI, real native checks, and live provider checks are recorded in [the verification log](verification.md). Real Google password/2FA acceptance, real-account recovery, authenticated/Premium playback, minimum-OS coverage, and long-session resource behavior retain the documented limitations.

Release assets contain the app archive and its SHA-256 checksum. Source, patch, and dependency provenance are tracked in `native-dependencies.json`, `native/mpv-build.json`, `native/patches/`, and [native dependencies](native-dependencies.md). Keep this repository and its beta private.
