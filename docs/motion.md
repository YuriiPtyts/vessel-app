# Balanced surface reveals

Appearance, account, queue, audio-quality, and now-playing panels use `panelReveal` in `src/lib/motion.ts`. The old genie stretched width and height at different rates and bent the clipping shape. At one reproduced quarter-progress point it was 85.6% wide but only 56.6% tall.

The replacement is a **220 ms** anchored reveal with uniform **96.5–100% scale**, at most **8px** of travel, and a shared content fade. There is no perspective, rotation, funnel clipping, or separate timing for sections. The background keeps its selected alpha while the content fades, so the material does not become clear or lose its blur mid-reveal.

Opening runs once per mounted panel. Playback updates do not reanimate it. Closing is immediate. Reduced motion makes opening immediate too.

Live inline vector clips follow the short animation and stop updating when it finishes; there is no idle animation loop. The renderer checks compare actual width/height ratios at 25%, 50%, and 75%, assert stable material opacity, inspect all shared popup reveals, and check settled layout, closing, and reduced motion. Screenshots are controlled UI fixtures, not native compositor/power measurements.
