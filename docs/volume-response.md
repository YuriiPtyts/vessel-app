# Output-stage volume control

## Reproduced delay

The previous slider sent prompt Rust/libmpv acknowledgements, but used mpv's software `volume` filter. On AVFoundation, that gain was already baked into roughly **1.8 seconds of queued PCM**. Faster IPC and a responsive thumb could not change those queued samples. The same local fixture did not show that delay on CoreAudio.

`node scripts/check-native-volume.mjs` exercises the real Rust player with each backend, without provider/network work. The original AVFoundation path took **1.804–1.848 seconds** across four mute/unmute transitions. The regression failed its 50 ms bound before the output-stage change.

## Replacement

- Playback and volume remain Rust-owned. The frontend's bounded latest-value lane and revision guards are retained.
- Software PCM gain remains at unity. Rust maps the existing perceptual curve, `(volume / 100)^3`, onto the per-player **AudioUnit gain** or **AVSampleBufferAudioRenderer volume**. This changes already-queued audio without seeking, restarting the song, or changing the Mac's system volume.
- The pinned `0.41.0-vessel.2` engine exposes `vessel-output-volume`, retaining the target per handle even before an output exists. It applies that gain before a newly initialized output can start. Muting or a low slider value therefore survives device/output recreation without a full-volume startup frame.
- Source/patch hashes and self-contained system linking remain enforced. See `native/patches/mpv-output-gain.patch` and [dependency provenance](native-dependencies.md).

## Verification and interpretation

Against the final beta `.app` runtime, four transitions each observed:

| Backend      | Previous path                | Output-stage path |
| ------------ | ---------------------------- | ----------------- |
| CoreAudio    | 7–12 ms in the initial probe | 4.0–11.4 ms       |
| AVFoundation | 1.804–1.848 s                | 0.047–2.369 ms    |

The audit observes CoreAudio callback PCM combined with its actual per-unit gain. For AVFoundation it observes real queued PCM, presentation timestamps, the running render clock, and the renderer's live gain. The fixture silences physical output independently. These checks establish elimination of the reproduced software/queue delay; **they are not an acoustic recording or a measurement of Bluetooth/device transport latency**.

Separate tests force output recreation at 0%, 27%, 73%, and 0% and inspect the first output gain. Ordinary mute restoration, curve retention, rapid-write/event bounds, real Opus/AAC transport, and radio EOF advancement remain covered. The observer runs one player per test process so a previous fixture's renderer timeline cannot contaminate the next measurement.

Test the updated app on the originally affected output and record any remaining audible delay, device, and selected source quality. Do not use acknowledgement timings alone as proof of audible response.
