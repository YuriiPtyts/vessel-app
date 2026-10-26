#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
mod native_diagnostics;

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

fn vessel_diagnostic_error() -> vessel_app::error::AppError {
    vessel_app::error::AppError::new(
        "diagnostics_unavailable",
        "Native diagnostics are not enabled for this run.",
        false,
    )
}

#[tauri::command]
fn native_material_probe(window: tauri::WebviewWindow, probe: serde_json::Value) -> Result<()> {
    #[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
    {
        native_diagnostics::report(window, probe)
    }
    #[cfg(not(all(target_os = "macos", feature = "native-diagnostics")))]
    {
        let _ = (window, probe);
        Err(vessel_diagnostic_error())
    }
}

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
async fn home_feed(
    state: tauri::State<'_, Application>,
    refresh: bool,
) -> Result<vessel_app::home::HomeFeed> {
    state.home(refresh).await
}

#[tauri::command]
async fn browse_collection(
    state: tauri::State<'_, Application>,
    id: String,
    more: bool,
) -> Result<vessel_app::home::CollectionPage> {
    state.collection(&id, more).await
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
                    .effect(Effect::LiquidGlassClear)
                    .effect(Effect::HudWindow)
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
    let builder = tauri::Builder::default().setup(|app| {
        use tauri::Emitter;
        let root = if cfg!(debug_assertions) {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime")
        } else {
            app.path().resource_dir()?.join("runtime")
        };
        #[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
        let root = std::env::var_os("VESSEL_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or(root);
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
        let warm_handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            warm_handle.state::<Application>().warm_up().await;
        });
        Ok(())
    });
    #[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
    let builder = builder.on_page_load(|webview, payload| {
        if std::env::var_os("VESSEL_NATIVE_DIAGNOSTICS").is_some()
            && payload.event() == tauri::webview::PageLoadEvent::Finished
        {
            let _ = webview.eval(native_diagnostics::PROBE_SCRIPT);
        }
    });
    let app = builder
        .invoke_handler(tauri::generate_handler![
            native_material_probe,
            runtime_report,
            search_music,
            home_feed,
            browse_collection,
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
