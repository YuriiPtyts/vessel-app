# Headphone-disconnect crash: unresolved investigation

Updated 2026-10-27. **The physical headphone crash is not fixed or reproduced by the synthetic test.** The packaged changes improve tint, source startup, and volume; no audio-backend workaround is shipped.

The owner has now confirmed **ordinary analog headphones through the 3.5 mm jack**. A subsequent report from 2026-10-27 17:07 repeats the same faulting thread and all three libmpv offsets; its application UUID matches the previously packaged `56479dd` build. This is not an old-copy explanation. The audible slider delay was also reported again: previous acknowledgement timings are not proof of audible gain latency.

## Captured evidence

The original `vessel-app-2026-10-26-222034.ips` report shows:

- `EXC_BAD_ACCESS` / `SIGSEGV`, null address.
- Faulting dispatch queue: `HALC_ProxyNotification Call Listener Queue`.
- Three leading frames in bundled `libmpv.dylib`, image offsets **326636**, **332124**, and **1247596** (unsymbolicated in the report).
- Following frames: CoreAudio `PropertiesChanged`, `ObjectsPublishedAndDied`, and its proxy notification listener.

The report is currently in `~/Library/Logs/DiagnosticReports/Retired/`; macOS may move reports. Keep original reports and any signed media URLs out of Git. `scripts/inspect-crash.mjs` prints crash mechanics without report identities or home paths, and gives a path-free error if the file cannot be read.

```sh
node scripts/inspect-crash.mjs "$HOME/Library/Logs/DiagnosticReports/Retired/vessel-app-2026-10-26-222034.ips"
```

## What the unattended test establishes

```sh
node scripts/probe-audio-device.mjs
```

This runs real muted libmpv playback of local 44.1 kHz **stereo** silence. `VESSEL_AUDIO_MONO=1` selects the earlier mono fixture. The Swift helper creates a temporary aggregate device, selects it, destroys it while selected, restores the original default output, and verifies restoration. The wrapper selects 20 cycles and bounds the process group to 45 seconds, including compilation, so a native teardown hang cannot leave the command running indefinitely. For a clean build, increase `VESSEL_AUDIO_PROBE_TIMEOUT_MS` (maximum 600000). It installs no driver. The underlying ignored Rust test performs three cycles without `VESSEL_AUDIO_STRESS=1`; the helper finishes cleanup if its parent exits and reports actual completed cycles.

The test checks native playback/error state and continued position advancement after restoration, allowing a bounded three-second recovery window. An initial 200 ms advancement assertion failed and one failure hung in teardown; that deadline was too short. With bounded polling, the next pass observed advancement in **326 ms** and normal teardown. Keep the outer watchdog when testing unknown native-engine behavior.

Observed output was **AVFoundation, S16**, before and after the 20-cycle run. This is **not** proof of CoreAudio-driver recovery or the owner's physical headphone path. Earlier aggregate-device/samplerate stress also passed without reproducing the reported crash. No regression test currently catches the actual null-pointer failure reliably.

### Further observations after the 3.5 mm clarification

- The stereo fixture selected actual **CoreAudio/S16** and passed 20 cycles with `MallocScribble=1` (observed post-route advancement 110 ms). A temporary float-planar stereo variant selected **CoreAudio/floatp** and passed 20 cycles (55 ms). A mono float variant under Guard Malloc selected AVFoundation and passed. None reproduced the SIGSEGV.
- A temporary real-GUI/muted guest-source-switching probe also passed 20 cycles. Rapid concurrent samplerate reinitialization instead hung until the watchdog; that was a different symptom and the throwaway reinit/probe code was removed.
- Built-in output inventory at inspection time exposed speakers only, with no settable data-source selector. The physical headphone transition cannot currently be driven through that property.
- These device notifications are **system-wide**, not process-isolated. The new report's notification voucher names `swift-frontend`, consistent with the Swift device-change probe, which may explain that incident's apparently random timing in another running Vessel instance. Avoid treating a passing test process as proof that a simultaneously running app survived.
- Disassembly of the matching library identifies the callback at file address `0x130944`, including the literal `Handling potential hotplug event...`. It loads `ao->log` before the logging call. The captured crash is the null dereference at `0x4fbec` inside that call; the image/offset match is established, but a reliable same-failure loop is still missing.
- The pinned MPVKit patch directory contains only the MoltenVK/static-build patches, not the upstream CoreAudio cleanup patch. A replacement engine still needs reproducible build/provenance and real regression verification; none is shipped by the remembered-account follow-up.

## Upstream lead and rejected workaround

- [mpv issue #18274](https://github.com/mpv-player/mpv/issues/18274) describes a matching macOS 27 hotplug crash signature. It was closed by [PR #18383](https://github.com/mpv-player/mpv/pull/18383), “ao_coreaudio: cleanup ao on init failure,” merged as `c5d391adba7bd024954d0df1e0405f5749f4d4ca` on 2026-09-01.
- This is a strong investigation lead, not a confirmed diagnosis of Vessel's failure. The local library is stripped; symbolication/source matching is still needed.
- The pinned zapper release is `v0.41.0-1`, published 2026-10-03. Its `sources.lock` pins mpv `v0.41.0` at `41f6a645068483470267271e1d09966ca3b9f413` and its build also applies MPVKit patches. Inspect those patches before concluding which fixes are present.
- An `audio-format=float` experiment still selected AVFoundation on this host. An assertion requiring CoreAudio failed both before and after that change. The option and incorrect CoreAudio-only assertion were removed. This experiment did not reproduce or repair the SIGSEGV.

## Resume with the real reproducer

1. The connection is confirmed **3.5 mm**. Establish a repeatable physical headphone-to-speaker transition in the normal app, including its track/quality and playing/paused state. For volume, clarify whether the thumb itself lags or only the audible change.
2. Launch the rebuilt `src-tauri/target/release/bundle/macos/Vessel.app`, play the same track/quality if known, and record the disconnect time and whether playback was active or paused. Disconnect/reconnect the actual device. Keep playback at a comfortable volume; this is a human hardware test.
3. If it crashes, locate the new report in DiagnosticReports (including Retired) and run the inspector. Compare the thread, exception, image UUID/offsets against the original before treating it as the same failure. Do not commit the raw report.
4. Establish a repeatable failing loop on that device. Inspect the pinned corresponding source and MPVKit patch set, symbolicate the relevant frames, and verify whether upstream #18383 addresses the observed lifetime failure. Do not replace this with more assertions that merely require a particular backend.
5. If an engine patch is justified, rebuild/pin a self-contained compatible dynamic library and corresponding source using the policy in `native-dependencies.md`; update asset hashes/provenance. The shipped app must not depend on Homebrew paths. Compare old/new engines against the same physical reproducer and the synthetic test.
6. Run frontend checks, Rust fmt/Clippy/tests, real playback/quality/radio checks, package with `npm run tauri -- build --bundles app`, and repeat physical removal during playback/paused playback. Verify the actual output backend and CPU behavior across repeated device changes. Commit/push only the verified change, with remaining limits recorded.

`diagnosing-bugs` is the appropriate next-session skill. See `verification.md` and `startup-and-materials.md` for completed performance/interface work and commands.
