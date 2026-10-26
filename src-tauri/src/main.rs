#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use tauri::Manager;
use vessel_app::{
    application::{Application, PlaybackSelection},
    catalog::{SearchKind, SearchResults},
    error::Result,
    library::{LibrarySnapshot, LocalLibrary},
    playback::PlaybackSnapshot,
    provider::TrackInspection,
    runtime::RuntimePaths,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeReport {
    app_version: &'static str,
    platform: &'static str,
    architecture: &'static str,
    debug_build: bool,
    playback_dependencies_available: bool,
}

#[tauri::command]
async fn search_music(
    state: tauri::State<'_, Application>,
    query: String,
    kind: SearchKind,
    more: bool,
) -> Result<SearchResults> {
    state.search(&query, kind, more).await
}

#[tauri::command]
async fn play_song(
    state: tauri::State<'_, Application>,
    video_id: String,
) -> Result<PlaybackSelection> {
    state.play_song(&video_id).await
}

#[tauri::command]
async fn local_library(state: tauri::State<'_, Application>) -> Result<LibrarySnapshot> {
    state.library()
}

#[tauri::command]
async fn toggle_like(
    state: tauri::State<'_, Application>,
    video_id: String,
) -> Result<LibrarySnapshot> {
    state.toggle_like(&video_id)
}

#[tauri::command]
async fn set_material_mode(window: tauri::WebviewWindow, reduced: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use tauri::window::{Effect, EffectState, EffectsBuilder};
        let effects = if reduced {
            None
        } else {
            Some(
                EffectsBuilder::new()
                    .effect(Effect::UnderWindowBackground)
                    .state(EffectState::FollowsWindowActiveState)
                    .build(),
            )
        };
        window.set_effects(effects).map_err(|_| {
            vessel_app::error::AppError::new(
                "material_unavailable",
                "The native window material could not be changed.",
                true,
            )
        })?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, reduced);
    Ok(())
}

#[tauri::command]
fn runtime_report(state: tauri::State<'_, Application>) -> RuntimeReport {
    RuntimeReport {
        app_version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        debug_build: cfg!(debug_assertions),
        playback_dependencies_available: state.runtime_available(),
    }
}

#[tauri::command]
async fn inspect_track(
    state: tauri::State<'_, Application>,
    url: String,
) -> Result<TrackInspection> {
    state.inspect(&url).await
}

#[tauri::command]
fn cancel_inspection(state: tauri::State<'_, Application>) {
    state.cancel_inspection();
}

#[tauri::command]
async fn play_track(
    state: tauri::State<'_, Application>,
    track_id: String,
    format_id: String,
) -> Result<PlaybackSnapshot> {
    state.play(&track_id, &format_id).await
}

#[tauri::command]
fn playback_snapshot(state: tauri::State<'_, Application>) -> Result<PlaybackSnapshot> {
    state.snapshot()
}

#[tauri::command]
async fn set_paused(
    state: tauri::State<'_, Application>,
    paused: bool,
) -> Result<PlaybackSnapshot> {
    state.pause(paused).await
}

#[tauri::command]
async fn seek_track(
    state: tauri::State<'_, Application>,
    position: f64,
) -> Result<PlaybackSnapshot> {
    state.seek(position).await
}

#[tauri::command]
async fn set_volume(state: tauri::State<'_, Application>, volume: f64) -> Result<PlaybackSnapshot> {
    state.volume(volume).await
}

#[tauri::command]
async fn stop_playback(state: tauri::State<'_, Application>) -> Result<PlaybackSnapshot> {
    state.stop().await
}

fn main() {
    let app = tauri::Builder::default()
        .setup(|app| {
            use tauri::Emitter;
            let root = if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime")
            } else {
                app.path().resource_dir()?.join("runtime")
            };
            let handle = app.handle().clone();
            let library = std::sync::Arc::new(LocalLibrary::open(
                app.path().app_data_dir()?.join("collection.json"),
            ));
            app.manage(Application::with_library(
                RuntimePaths::new(root),
                library,
                move |snapshot| {
                    let _ = handle.emit("playback-state", snapshot);
                },
            )?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            runtime_report,
            search_music,
            play_song,
            local_library,
            toggle_like,
            set_material_mode,
            inspect_track,
            cancel_inspection,
            play_track,
            playback_snapshot,
            set_paused,
            seek_track,
            set_volume,
            stop_playback
        ])
        .build(tauri::generate_context!())
        .expect("could not start the Vessel native shell");
    app.run(|handle, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            handle.state::<Application>().shutdown();
        }
    });
}
