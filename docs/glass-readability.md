# Shared glass, independent modes, and readable text

## Current controls

- Light/Dark is an explicit saved preference, available in Appearance and through the top-bar sun/moon button.
- Glass tint changes the shared material alpha from 50–100%. It never changes the selected mode, surface RGB, or icon colors. Opaque mode makes that same material solid.
- Below 70% tint, only glyphs receive extra contrast. Light mode adds a small same-color stroke. Dark mode uses white text fill with a 0.65px black edge, painted behind the fill. Primary filled buttons retain their normal dark text. SVG icons are excluded from this glyph treatment.
- Increased contrast uses semantic text roles for the selected mode. An older hard-coded pale artist-caption override was removed because it failed in light mode.
- Existing v2 preferences gain `colorMode` without resetting other choices. Older beta.7 records infer their previous mode once; changing tint afterward does not infer a new mode.

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
- Text checks require 4.5:1 for ordinary text and 3:1 for large headings. The final Home/search samples measure at least **5.05:1**, using the outline-aware model where the glyph edge is active.

`tests/ui/contrast.js` samples actual rendered backgrounds with glyph fill/stroke hidden. It reads the actual text-fill color and admits an outline only when it is at least 0.5px, near-opaque, painted behind the fill, and itself contrasts with the fill. The comparison then uses the stronger visible glyph edge/fill against the background. A regression removes the stroke and verifies that the unprotected dark-mode text fails on the bright fixture; it does not merely assume that having a shadow is enough.

These are declared-glyph/background comparisons, not an audit of every anti-aliased display pixel or native desktop compositor. Native window/package results and remaining hardware/account limits are recorded in [verification](verification.md).
