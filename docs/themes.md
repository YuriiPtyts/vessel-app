# Appearance and future themes

## Current default: Duskfox palette, clear-glass material

The owner requested the colors of their active Zed theme. The configured dark theme was **Duskfox - blurred**, supplied by **Nightfox for Zed 0.10.5**. Vessel snapshots the relevant palette; it does not read or change Zed settings at runtime and does not require Zed to be installed.

Attribution/reference: https://github.com/cange/nightfox.zed (a port of the Nightfox Neovim palettes).

| Role | Palette color |
| --- | --- |
| Main background | `#232136` |
| Low surface | `#191726` |
| Elevated surface | `#2d2a45` |
| Selection | `#63577d` |
| Primary foreground | `#eae8ff` |
| Text | `#e0def4` |
| Secondary text | `#b1acde` |
| Rose / default interactive accent | `#ea9a97` |
| Iris | `#c4a7e7` |
| Foam | `#9ccfd8` |
| Gold | `#f6c177` |

Palette tokens live in `src/lib/themes/duskfox.css`. Shared material/layout styling consumes those tokens in `src/lib/glass.css`. The palette follows Zed's colors; Vessel's opacity, elevation, and controls are adapted for music listening.

## Customization available now

- Four Duskfox accent choices.
- Live glass-tint adjustment from 8–90%, default 18%.
- Reduced-transparency mode that removes native vibrancy and CSS backdrop filters.
- Comfortable/compact song rows.
- Expanded/compact navigation.
- A now-playing artwork panel, enabled by default on wide windows. The player cover opens an overlay on smaller windows.
- Reset to the defaults.

Preferences are bounded/validated and stored locally as `vessel:appearance:v2`. The previous default 56% tint migrates to 18%; non-default tint, accent, density, navigation, artwork, and reduced-transparency choices are retained. Existing `vessel:opaque` settings remain supported. Reduced motion remains active. Native clear Liquid Glass is used on macOS 26+, with HUD vibrancy on older systems and bounded WebKit-compatible player/popover blur.

Visual material reference: https://github.com/Jonzu00/GlassPlayer. That project uses Windows Acrylic and translucent rounded panes. Vessel independently applies the layering/highlight ideas with AppKit and its Duskfox palette; no WPF source is incorporated. Desktop transparency comes from the native window/effect stack, while web tint and surface fills stay low enough to reveal it.

## Owner-requested future goal: additional themes

Add a small theme library and picker while retaining Duskfox as the default. Introduce stable theme IDs and previews, persist the selected theme, preserve independent layout choices, and test contrast and reduced-transparency behavior for each palette. Start with a few coherent curated palettes; a safe palette-data import/export format can follow. Do not require editor configuration, arbitrary executable theme plugins, or a permanent service.

This goal is planned, not a claim that multiple complete themes are already implemented.
