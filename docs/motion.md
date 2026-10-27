# Genie-style surface reveals

Appearance, account, queue, audio-quality, and now-playing panels use the shared `genie` transition in `src/lib/motion.ts` when they appear. The 360 ms reveal unfurls from an anchor corner using curved clipping, perspective, separate horizontal/vertical expansion, and a short fade. It consumes the existing theme materials and adds no blur layer.

The effect runs once on mounting a surface. Playback snapshots and track metadata updates do not repeatedly animate an open panel. Closing is immediate, without leaving focusable controls in an outro. Only transient surfaces animate, rather than the entire window or every song row.

`prefers-reduced-motion: reduce` makes the reveal immediate. This follows the operating-system preference and introduces no new stored appearance setting.

WebKit contract checks inspect actual Web Animations keyframes, a paused curved reveal, the settled layout, immediate closing, and reduced-motion behavior. The mid-reveal screenshot is a controlled renderer/IPC fixture, not a native compositor or power measurement.
