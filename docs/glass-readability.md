# Shared glass, independent modes, and readable text

## Current controls

- Light/Dark is an explicit saved preference, available in Appearance and through the top-bar sun/moon button.
- Glass tint changes the shared material alpha from 50–100%. It never changes the selected mode, surface RGB, or icon colors. Opaque mode makes that same material solid.
- Dark-mode letters keep their normal system-font weights, semantic colors, and antialiasing through the entire tint range. There is no stroke, halo, forced-white fill, or extra low-tint bolding. Primary/secondary labels retain their hierarchy and filled buttons retain dark text. Light mode keeps its existing modest low-tint weight treatment.
- Increased contrast uses semantic text roles for the selected mode. An older hard-coded pale artist-caption override was removed because it failed in light mode.
- Existing v2 preferences gain `colorMode` without resetting other choices. Older beta.7 records infer their previous mode once; changing tint afterward does not infer a new mode.

## Material-supported typography

[Apple's material guidance](https://developer.apple.com/design/human-interface-guidelines/materials) recommends **Regular Liquid Glass** for text-heavy sidebars, alerts, and popovers. This variant blurs and adjusts background luminosity to support foreground legibility. Clear glass is intended for richer media backgrounds and can require separate dimming.

Vessel now selects native `LiquidGlassRegular` at startup and when restoring native effects on macOS 26+, retaining the existing HUD fallback. The UI uses semantic label roles over that material rather than decorating glyph edges. This is native material support for web-rendered system-font text, not a claim that WebView labels gain AppKit's native vibrancy behavior.

Sharp lower content must not compete with floating labels. A WebKit regression exposed readable underlying letters through popovers even when backdrop blur was requested. The live vector cutouts now also occlude underlying content regions and lower overlay content groups. Nested quality popovers remain visible, geometry updates remain event/animation bounded, and shared material alpha stays equal to the tint setting. There are no extra opaque reading patches or desktop-pixel sampling.

## Transparency flash and player-edge fix

The previous implementation rebuilt SVG mask images as data URLs. Until a replacement mask decoded, a material could disappear. A controlled 120ms resource delay reproduced the report exactly: the window and both player edges dropped from `[128,128,128]` to `[0,0,0]` over a black backdrop.

`material.ts` now keeps inline SVG **clip paths**, updated in place. Each clip is the complement of one rounded cutout; intersecting these complements handles overlapping panels without adding the window material back into their overlap. There are no mask-image resources to load. Coordinates use the actual painted padding box, not the outer border, and account for the reveal scale.

The window, player, artwork area, content, and popovers still receive one material layer per point. Appearance has a separate inner scroller so its background does not move away when settings scroll. Geometry updates are bounded to layout changes and active reveals; no desktop pixels are read and no idle sampling loop is used.

## Menu behavior

Appearance, queue, quality, and account popovers share `popover.ts`: outside pointer clicks and Escape close them; inside clicks stay inside; trigger clicks toggle normally. Dismissal does not prevent the underlying control's click. Opening another menu also closes the previous one for keyboard users.

## Checks and interpretation

```sh
npm run test:ui
VESSEL_UI_BROWSER=chromium npm run test:ui -- tests/ui/appearance-controls.spec.js tests/ui/popover-behavior.spec.js tests/ui/sidebar-and-material.spec.js tests/ui/glass-contrast.spec.js tests/ui/popover-transparency.spec.js
```

- Explicit modes are checked at 50%, 69%, 70%, 75%, 90%, and 100% over white, dark, and patterned backgrounds. Mode, tint, palette, and migration/persistence have independent assertions.
- The flash regression delays old mask-image loads, samples opening/settled frames and both player edges, and scrolls Appearance. It fails with the old image masks and passes with inline vector clips.
- Sidebar coverage retains 80 playlists, actual wheel scrolling, and reachable controls at 1100×700 and 420×800.
- Text checks require 4.5:1 for ordinary text and 3:1 for large headings. Beta.11 dark 50% Home/search samples measure at least **5.03:1 in WebKit and Chromium** against the controlled material-backed fixtures.

`tests/ui/contrast.js` compares declared ink with rendered backgrounds for plain text. The typography regression requires zero shadow/stroke, stable weights/fills between 85% and 50%, and fixed surface/icon/border colors. It failed against the previous one-pixel black halo. Removing the fixture's supporting backdrop is also red-capable; plain white text over raw clear 50% glass on white is not asserted to be readable.

Browser previews cannot render the actual AppKit/DWM backing. `material-backdrop.js` supplies a controlled 35% dark underlay for dark-mode contrast cases, using Apple's clear-glass dimming example as a test condition. It is not an exact model of Regular glass, an added production layer, or a native compositor measurement. The native macOS diagnostic independently verifies Regular style 0 and real WebView typography. Native window/package results and remaining hardware/account limits are recorded in [verification](verification.md).
