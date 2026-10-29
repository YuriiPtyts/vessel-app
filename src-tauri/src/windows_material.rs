//! Select a compositor path appropriate to the actual Windows build.
//! Legacy Acrylic stalls dragging on Windows 10 1903+; Blur avoids that path.
//! See window-vibrancy's upstream platform/performance notes.
use tauri::window::Effect;

pub fn effect_for_build(build: u32) -> Effect {
    if build < 22_000 {
        Effect::Blur
    } else if build < 22_621 {
        Effect::Mica
    } else {
        Effect::Acrylic
    }
}

#[cfg(target_os = "windows")]
pub fn apply(window: &tauri::WebviewWindow, reduced: bool) -> vessel_app::error::Result<()> {
    use tauri::window::{Color, EffectsBuilder};
    use vessel_app::error::AppError;
    window
        .set_background_color(Some(Color(0, 0, 0, 0)))
        .map_err(|_| {
            AppError::new(
                "material_unavailable",
                "The Windows webview background could not be made transparent.",
                true,
            )
        })?;
    let effects = (!reduced).then(|| {
        EffectsBuilder::new()
            .effect(effect_for_build(
                windows_version::OsVersion::current().build,
            ))
            .color(Color(0, 0, 0, 1))
            .build()
    });
    window.set_effects(effects).map_err(|_| {
        AppError::new("material_unavailable", "The Windows backdrop could not be changed. Check Windows transparency effects and try again.", true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_10_avoids_the_legacy_acrylic_dragging_path() {
        for build in [18_362, 19_041, 19_045] {
            assert!(matches!(effect_for_build(build), Effect::Blur));
        }
        assert!(matches!(effect_for_build(22_000), Effect::Mica));
        for build in [22_621, 22_631, 26_100] {
            assert!(matches!(effect_for_build(build), Effect::Acrylic));
        }
    }
}
