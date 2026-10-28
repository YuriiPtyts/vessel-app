# Appearance and future themes

## Current default: Duskfox palette, adaptive glass material

The owner requested the colors of their active Zed theme. The configured dark theme was **Duskfox - blurred**, supplied by **Nightfox for Zed 0.10.5**. Vessel snapshots the relevant palette; it does not read or change Zed settings at runtime and does not require Zed to be installed.

Attribution/reference: https://github.com/cange/nightfox.zed (a port of the Nightfox Neovim palettes).

| Role                              | Palette color |
| --------------------------------- | ------------- |
| Main background                   | `#232136`     |
| Low surface                       | `#191726`     |
| Elevated surface                  | `#2d2a45`     |
| Selection                         | `#63577d`     |
| Primary foreground                | `#eae8ff`     |
| Text                              | `#e0def4`     |
| Secondary text                    | `#b1acde`     |
| Rose / default interactive accent | `#ea9a97`     |
| Iris                              | `#c4a7e7`     |
| Foam                              | `#9ccfd8`     |
| Gold                              | `#f6c177`     |

Palette tokens live in `src/lib/themes/duskfox.css`. Shared material/layout styling consumes those tokens in `src/lib/glass.css`. The palette follows Zed's colors; Vessel's opacity, elevation, and controls are adapted for music listening.

## Customization available now

- Four Duskfox accent choices.
- Independent Light/Dark buttons in Appearance and a sun/moon shortcut in the top bar.
- Live glass-tint adjustment from 50–100%, default 50%. It changes transparency only, not the selected light/dark material.
- Crisp dark-mode system-font text with stable weights, semantic label colors, and no contour, halo, or forced-white fill. Native Regular glass supports legibility; sharp content beneath floating surfaces is occluded. Surface/icon tokens and tint alpha stay fixed.
- Reduced-transparency mode that removes native vibrancy and CSS backdrop filters.
- Comfortable/compact song rows.
- Expanded/compact navigation.
- A now-playing artwork panel, enabled by default on wide windows. The player cover opens an overlay on smaller windows.
- Reset to the defaults.

Preferences stay validated in `vessel:appearance:v2`, now with an explicit `colorMode`. An older beta.7 record without this field keeps its previous visible light/dark appearance once, then stores the independent choice. The old default 56% tint migrates to 50%; low values are clamped to the supported range. Accent, spacing, navigation, artwork, and transparency preferences are preserved. Legacy `vessel:opaque` remains supported. Native Regular Liquid Glass is used on macOS 26+, with HUD vibrancy on older systems and bounded player/popover blur. See [the material-supported typography approach](glass-readability.md#material-supported-typography).

Visual material reference: https://github.com/Jonzu00/GlassPlayer. That project uses Windows Acrylic and translucent rounded panes. Vessel independently applies the layering/highlight ideas with AppKit and its Duskfox palette; no WPF source is incorporated. Desktop transparency comes from the native window/effect stack, while web tint and surface fills stay low enough to reveal it.

## Owner-requested future goal: additional themes

Add a small theme library and picker while retaining Duskfox as the default. Introduce stable theme IDs and previews, persist the selected theme, preserve independent layout choices, and test contrast and reduced-transparency behavior for each palette. Start with a few coherent curated palettes; a safe palette-data import/export format can follow. Do not require editor configuration, arbitrary executable theme plugins, or a permanent service.

The planned picker adds new palettes beyond the current Duskfox light/dark modes.
