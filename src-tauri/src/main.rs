#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use tauri::Manager;
use vessel_app::{
    application::Application, error::Result, playback::PlaybackSnapshot, provider::TrackInspection,
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
            app.manage(Application::new(RuntimePaths::new(root), move |snapshot| {
                let _ = handle.emit("playback-state", snapshot);
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            runtime_report,
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
