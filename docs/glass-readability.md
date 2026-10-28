# Legible text and one shared glass material

## Why the previous version looked uneven

The beta.4/5 contrast treatment added extra dark reading layers over the sidebar, bars, lists, and popover text. That made pale text readable, but left the main gutters much more transparent than the other regions. At 50% tint over white, the reproduced sidebar pixel was `[49,47,61]` while the main gutter was `[148,145,154]`.

The owner asked for one transparency level throughout the app, with text changing instead of covering the glass with dark patches. Beta.7 replaces that earlier treatment.

## Current treatment

1. **One alpha.** Sidebar, content, top bar, artwork area, player, and popovers use the same selected material opacity. Structural containers have no separate dark fills. Opaque mode uses alpha 1 everywhere.
2. **Readable ink/material pair.** At 50–69% tint, white glass uses very dark text. At 70–100%, Duskfox dark glass uses near-white text. These pairs retain contrast over the controlled darkest/brightest backdrops without reading the desktop. The accent preference is unchanged; important accent-colored text uses a readable ink role, while accent fills/marks retain the selected color.
3. **No doubled opacity under floating panels.** `material.ts` observes element bounds and creates SVG coverage masks. The window material has a cutout beneath floating surfaces; a lower floating material has cutouts beneath higher ones. Each visible point receives one material tint, including the floating player. Floating surfaces can blur the content underneath without adding an opaque reading field.
4. **Contrast preferences.** OS increased contrast strengthens text/borders without adding opaque patches or rewriting preferences. Reduced transparency/100% tint removes material blur and stays opaque. Queue history uses image hierarchy instead of faded text.
5. **Bounded sidebar.** The application grid has a bounded row. Saved playlists get their own flex/scroll area, separate from navigation and the fixed sidebar footer. At short heights the decorative tagline is hidden to leave more room for actual controls.

The coverage observer reacts to layout, resize, opening/closing, and animation completion; it does not run a continuous background-sampling loop or request screen-capture access. Duskfox remains the palette, and saved appearance values stay in version 2.

## Reproduce the checks

```sh
npm run test:ui -- tests/ui/sidebar-and-material.spec.js tests/ui/glass-contrast.spec.js tests/ui/popover-transparency.spec.js
# With Playwright Chromium installed:
VESSEL_UI_BROWSER=chromium npm run test:ui -- tests/ui/sidebar-and-material.spec.js tests/ui/glass-contrast.spec.js tests/ui/popover-transparency.spec.js
```

- The sidebar regression uses **80 saved playlists**, actual wheel scrolling, a 1100×700 window, and a 420×800 layout. The last playlist can be reached while navigation and Appearance remain in the viewport, and Appearance opens normally.
- Rendered blank-material samples compare sidebar, content, top bar, player, and Appearance over white/black backdrops at 50%, 70%, and 100%. They remain within six RGB levels per channel; reading-region opacity is not used as a substitute for matching material.
- Text checks cover Home/search over white, dark, and patterned backdrops at **50%, 69%, 70%, 75%, 90%, and 100%**. They include the automatic light/dark boundary, accents, account errors, queue/quality captions, and increased contrast.
- Normal text requires **4.5:1**; large page headings require **3:1**, following [WCAG contrast minimum](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html). The observed minimum across the latest Home/search samples is **5.05:1**. Muted/secondary text is not faded to achieve hierarchy.

`tests/ui/contrast.js` compares declared text colors with actual screenshot backgrounds, temporarily hiding the sampled text. It excludes clipped/covered text and does not treat anti-aliased glyph edges as the foreground color.

These are rendered browser checks with controlled account/playback replies, not native desktop screenshots or measurements of compositor power use. Current native packaging/window checks and remaining hardware limits are recorded in [verification](verification.md).
