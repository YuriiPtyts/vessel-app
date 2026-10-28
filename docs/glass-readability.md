# Legible text on clear glass

## Reproduced problem

At the clearest supported tint (50%), a bright backdrop washed out the original pale text. A real WebKit render with controlled white behind the app measured text contrast as low as **1.05:1**; 33 sampled text fragments failed the minimum. The baseline screenshot reproduced the washed-out player, card metadata, navigation, and artwork captions.

The regression is `npm run test:ui -- --grep 'glass text contrast|glass controls'`. It uses the actual UI/styles with fixture IPC and controlled desktop-like backgrounds; it is not native desktop/compositor automation.

## Chosen treatment

1. **Local reading materials.** The window keeps the user's selected tint. Text-bearing surfaces receive an additional Duskfox reading tint, while gutters and native glass remain translucent. For tint `t`, the extra alpha is `max(0, (0.9 - t) / (1 - t))`; opaque mode returns zero before division. This bounds the underlying lightness without sampling the desktop or adding a full-window blur.
2. **Semantic text roles.** Glass secondary/error colors derive from the existing palette. Meaningful muted captions use a readable text role instead of the original faint decorative color. The palette/accent choices stay intact.
3. **Tonal interaction states.** Nested card washes, selected search pills, and current/pending rows retain dark reading fields. Past queue items use artwork/color hierarchy instead of fading their text along with the whole row.
4. **OS increased contrast.** `prefers-contrast: more` strengthens the reading material, text roles, and borders without rewriting stored appearance preferences. Existing reduced-transparency and 100% opaque modes continue to work.

Stable material behind text is the contrast guarantee. Text shadows or inversion alone cannot guarantee readable color pairs over arbitrary imagery, and runtime desktop capture is unnecessary. The implementation uses static CSS paints and a preference-derived alpha; it adds no background-sampling loop or new blur layer.

## Source map

- `src/lib/themes/duskfox.css`: semantic reading material and derived glass ink tokens.
- `src/lib/readability.css`: reading surfaces, tonal states, queue hierarchy, increased contrast.
- `src/lib/appearance.ts` / `src/App.svelte`: derived scrim alpha, with existing saved preferences unchanged.
- `tests/ui/contrast.js`: computed foreground colors plus screenshot samples of the actual painted background with text temporarily hidden. DOM text ranges, overflow clipping, and hit-testing exclude adjacent icons, clipped cards, and occluded content.
- `tests/ui/glass-contrast.spec.js`: Home/search at 50%, 75%, 90%, and 100% tint over white, dark, and patterned backdrops; accent/selection, account error, queue/quality captions, and OS increased-contrast checks.

## Verification scope

Normal text is checked at **4.5:1**; the large 32–40px page headings use **3:1**, following [WCAG contrast minimum](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html). Disabled controls, clipped content, and covered text are not treated as visible reading samples. Anti-aliased glyph edges are not compared as if they were the declared ink color.

At 50% tint, the updated Home/search samples measure at least **5.49:1** over the controlled bright/patterned backgrounds. The four tint settings pass their applicable thresholds, all four accents pass selected-state checks, and account/queue/quality text passes. Increased contrast is checked against the actual CSS media query, including unchanged serialized appearance preferences. Screenshots are reviewed separately from the numeric checks. Native material/window verification retains the limits recorded in [verification](verification.md).
