#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
mod media_controls;
#[cfg(target_os = "windows")]
#[path = "media_controls_windows.rs"]
mod media_controls;
#[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
mod native_diagnostics;
#[cfg(all(target_os = "windows", feature = "native-diagnostics"))]
mod windows_diagnostics;

use serde::Serialize;
use std::sync::Arc;
use tauri::Manager;
use vessel_app::{
    account::AccountSnapshot,
    account_library::AccountLibrarySnapshot,
    application::{AppEvent, Application, PlaybackSelection},
    catalog::{SearchKind, SearchResults},
    error::Result,
    library::{LibrarySnapshot, LocalLibrary},
    playback::PlaybackSnapshot,
    provider::TrackInspection,
    queue::QueueSnapshot,
    runtime::RuntimePaths,
    sign_in::SignIn,
};

type AppState<'a> = tauri::State<'a, Arc<Application>>;
#[cfg(any(target_os = "macos", target_os = "windows"))]
type MediaDispatch = Arc<dyn Fn(media_controls::Action) + Send + Sync>;

fn trusted_caller(label: &str, url: &url::Url) -> bool {
    label == "main"
        && url.username().is_empty()
        && url.password().is_none()
        && ((url.scheme() == "tauri"
            && url.host_str() == Some("localhost")
            && url.port().is_none())
            || (cfg!(debug_assertions)
                && url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port() == Some(1420))
            || (cfg!(target_os = "windows")
                && matches!(url.scheme(), "http" | "https")
                && url.host_str() == Some("tauri.localhost")
                && url.port().is_none()))
}

fn vessel_diagnostic_error() -> vessel_app::error::AppError {
    vessel_app::error::AppError::new(
        "diagnostics_unavailable",
        "Native diagnostics are not enabled for this run.",
        false,
    )
}

#[tauri::command]
async fn windows_smoke_probe(
    window: tauri::WebviewWindow,
    state: AppState<'_>,
    probe: serde_json::Value,
) -> Result<()> {
    #[cfg(all(target_os = "windows", feature = "native-diagnostics"))]
    {
        windows_diagnostics::report(window, state.inner().clone(), probe).await
    }
    #[cfg(not(all(target_os = "windows", feature = "native-diagnostics")))]
    {
        let _ = (window, state, probe);
        Err(vessel_diagnostic_error())
    }
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
    state: AppState<'_>,
    query: String,
    kind: SearchKind,
    more: bool,
) -> Result<SearchResults> {
    state.search(&query, kind, more).await
}

#[tauri::command]
async fn play_song(
    state: AppState<'_>,
    video_id: String,
    context: Vec<String>,
) -> Result<PlaybackSelection> {
    state.play_song_with_context(&video_id, context).await
}

#[tauri::command]
async fn home_feed(state: AppState<'_>, refresh: bool) -> Result<vessel_app::home::HomeFeed> {
    state.home(refresh).await
}

#[tauri::command]
async fn browse_collection(
    state: AppState<'_>,
    id: String,
    more: bool,
) -> Result<vessel_app::home::CollectionPage> {
    state.collection(&id, more).await
}

#[tauri::command]
async fn local_library(state: AppState<'_>) -> Result<LibrarySnapshot> {
    state.library()
}

#[tauri::command]
async fn toggle_like(state: AppState<'_>, video_id: String) -> Result<LibrarySnapshot> {
    state.toggle_like(&video_id)
}

#[tauri::command]
async fn account_library(state: AppState<'_>) -> Result<AccountLibrarySnapshot> {
    state.account_library()
}

#[tauri::command]
async fn refresh_account_library(
    state: AppState<'_>,
    force: bool,
    account_revision: u64,
) -> Result<AccountLibrarySnapshot> {
    state.refresh_account_library(force, account_revision).await
}

#[tauri::command]
async fn set_account_like(
    state: AppState<'_>,
    video_id: String,
    liked: bool,
    account_revision: u64,
) -> Result<AccountLibrarySnapshot> {
    state
        .set_account_like(&video_id, liked, account_revision)
        .await
}

#[tauri::command]
async fn account_playlist(
    state: AppState<'_>,
    id: String,
    more: bool,
    account_revision: u64,
) -> Result<vessel_app::home::CollectionPage> {
    state.account_playlist(&id, more, account_revision).await
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
    #[cfg(target_os = "windows")]
    {
        use tauri::window::{Color, Effect, EffectsBuilder};
        window
            .set_background_color(Some(Color(0, 0, 0, 0)))
            .map_err(|_| {
                vessel_app::error::AppError::new(
                    "material_unavailable",
                    "The Windows webview background could not be made transparent.",
                    true,
                )
            })?;
        let effects = if reduced {
            None
        } else {
            Some(
                EffectsBuilder::new()
                    .effect(Effect::Acrylic)
                    .color(Color(0, 0, 0, 1))
                    .build(),
            )
        };
        window.set_effects(effects).map_err(|_| {
            vessel_app::error::AppError::new("material_unavailable", "The Windows backdrop could not be changed. Check Windows transparency effects and try again.", true)
        })?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (window, reduced);
    Ok(())
}

#[tauri::command]
fn runtime_report(state: AppState<'_>) -> RuntimeReport {
    RuntimeReport {
        app_version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        debug_build: cfg!(debug_assertions),
        playback_dependencies_available: state.runtime_available(),
    }
}

#[tauri::command]
async fn inspect_track(state: AppState<'_>, url: String) -> Result<TrackInspection> {
    state.inspect(&url).await
}

#[tauri::command]
fn cancel_inspection(state: AppState<'_>) {
    state.cancel_inspection();
}

#[tauri::command]
async fn play_track(
    state: AppState<'_>,
    track_id: String,
    format_id: String,
) -> Result<PlaybackSnapshot> {
    state.play(&track_id, &format_id).await
}

#[tauri::command]
fn playback_snapshot(state: AppState<'_>) -> Result<PlaybackSnapshot> {
    state.snapshot()
}

#[tauri::command]
async fn set_paused(state: AppState<'_>, paused: bool) -> Result<PlaybackSnapshot> {
    state.pause(paused).await
}

#[tauri::command]
async fn seek_track(state: AppState<'_>, position: f64) -> Result<PlaybackSnapshot> {
    state.seek(position).await
}

#[tauri::command]
async fn set_volume(state: AppState<'_>, volume: f64) -> Result<PlaybackSnapshot> {
    state.volume(volume).await
}

#[tauri::command]
async fn toggle_mute(state: AppState<'_>) -> Result<PlaybackSnapshot> {
    state.toggle_mute().await
}

#[tauri::command]
async fn stop_playback(state: AppState<'_>) -> Result<PlaybackSnapshot> {
    state.stop().await
}

#[tauri::command]
fn queue_snapshot(state: AppState<'_>) -> Result<QueueSnapshot> {
    state.queue()
}
#[tauri::command]
fn selection_snapshot(state: AppState<'_>) -> Result<Option<TrackInspection>> {
    state.inspection()
}
#[tauri::command]
fn account_snapshot(state: AppState<'_>) -> AccountSnapshot {
    state.account()
}
#[tauri::command]
async fn next_song(state: AppState<'_>) -> Result<Option<PlaybackSelection>> {
    state.inner().next().await
}
#[tauri::command]
async fn previous_song(state: AppState<'_>) -> Result<Option<PlaybackSelection>> {
    state.inner().previous().await
}
#[tauri::command]
async fn jump_queue(
    state: AppState<'_>,
    index: usize,
    video_id: String,
) -> Result<PlaybackSelection> {
    state.inner().jump_queue(index, Some(&video_id)).await
}
#[tauri::command]
fn remove_queue(state: AppState<'_>, index: usize, video_id: String) -> Result<QueueSnapshot> {
    state.remove_queue(index, &video_id)
}
#[tauri::command]
async fn clear_queue(state: AppState<'_>) -> Result<QueueSnapshot> {
    state.clear_queue().await
}
#[tauri::command]
async fn set_autoplay(state: AppState<'_>, enabled: bool) -> Result<QueueSnapshot> {
    state.inner().autoplay(enabled).await
}
#[tauri::command]
async fn retry_radio(state: AppState<'_>) -> Result<QueueSnapshot> {
    state.inner().retry_radio().await
}
#[tauri::command]
async fn connect_google(
    handle: tauri::AppHandle,
    state: AppState<'_>,
    sign_in: tauri::State<'_, Arc<SignIn>>,
) -> Result<AccountSnapshot> {
    sign_in.inner().start(handle, state.inner().clone()).await
}
#[tauri::command]
async fn cancel_google(
    handle: tauri::AppHandle,
    state: AppState<'_>,
    sign_in: tauri::State<'_, Arc<SignIn>>,
) -> Result<AccountSnapshot> {
    sign_in.cancel(&handle, &state).await
}
#[tauri::command]
async fn sign_out(
    handle: tauri::AppHandle,
    state: AppState<'_>,
    sign_in: tauri::State<'_, Arc<SignIn>>,
) -> Result<AccountSnapshot> {
    sign_in.forget(&handle, &state).await
}

fn main() {
    let builder = tauri::Builder::default().setup(|app| {
        use tauri::Emitter;
        let root = if cfg!(debug_assertions) {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime")
        } else {
            app.path().resource_dir()?.join("runtime")
        };
        #[cfg(feature = "native-diagnostics")]
        let root = std::env::var_os("VESSEL_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or(root);
        let handle = app.handle().clone();
        let data = app.path().app_data_dir()?;
        let remember_probe = cfg!(all(target_os = "macos", feature = "native-diagnostics"))
            && std::env::var_os("VESSEL_REMEMBER_DIAGNOSTICS").is_some();
        let sign_in_probe = cfg!(all(target_os = "macos", feature = "native-diagnostics"))
            && std::env::var_os("VESSEL_SIGN_IN_DIAGNOSTICS").is_some();
        let media_probe = cfg!(all(target_os = "macos", feature = "native-diagnostics"))
            && std::env::var_os("VESSEL_MEDIA_DIAGNOSTICS").is_some();
        let windows_smoke = cfg!(all(target_os = "windows", feature = "native-diagnostics"))
            && std::env::var_os("VESSEL_WINDOWS_SMOKE_REPORT").is_some();
        let isolated_services = remember_probe || sign_in_probe || media_probe || windows_smoke;
        let library = Arc::new(if isolated_services {
            LocalLibrary::memory()
        } else {
            LocalLibrary::open(data.join("collection.json"))
        });
        app.manage(Arc::new(SignIn::default()));
        let emit = move |event| {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            if let AppEvent::Playback(snapshot) = &event {
                let snapshot = snapshot.clone();
                let _ = handle.run_on_main_thread(move || media_controls::update(&snapshot));
            }
            let _ = match event {
                AppEvent::Playback(snapshot) => handle.emit_to("main", "playback-state", snapshot),
                AppEvent::Queue(snapshot) => handle.emit_to("main", "queue-state", snapshot),
                AppEvent::Selection(inspection) => {
                    handle.emit_to("main", "selection-state", inspection)
                }
                AppEvent::Account(snapshot) => handle.emit_to("main", "account-state", snapshot),
                AppEvent::AccountLibrary(snapshot) => {
                    handle.emit_to("main", "account-library-state", snapshot)
                }
            };
        };
        app.manage(if isolated_services {
            Application::with_library(RuntimePaths::new(root), library, emit)?
        } else {
            Application::desktop(
                RuntimePaths::new(root),
                library,
                data.join("queue.json"),
                emit,
            )?
        });
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if !isolated_services || windows_smoke {
            let application = Arc::downgrade(&app.state::<Arc<Application>>());
            let dispatch: MediaDispatch = Arc::new(move |action| {
                if let Some(application) = application.upgrade() {
                    tauri::async_runtime::spawn(async move {
                        use media_controls::Action;
                        match action {
                            Action::Next => {
                                let _ = application.next().await;
                            }
                            Action::Previous => {
                                let _ = application.previous().await;
                            }
                            Action::Play => {
                                let _ = application.pause(false).await;
                            }
                            Action::Pause => {
                                let _ = application.pause(true).await;
                            }
                            #[cfg(target_os = "macos")]
                            Action::Toggle => {
                                if let Ok(snapshot) = application.snapshot() {
                                    let _ = application
                                        .pause(
                                            snapshot.status
                                                != vessel_app::playback::PlaybackStatus::Paused,
                                        )
                                        .await;
                                }
                            }
                        }
                    });
                }
            });
            #[cfg(target_os = "macos")]
            media_controls::install(dispatch);
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main")
                && media_controls::install(window.hwnd()?, dispatch).is_err()
            {
                eprintln!("Windows media controls are unavailable on this system.");
            }
        }
        #[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
        if let Some(destination) = std::env::var_os("VESSEL_MEDIA_DIAGNOSTICS") {
            media_controls::diagnose(std::path::Path::new(&destination))?;
        }
        if !isolated_services {
            let warm_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                warm_handle.state::<Arc<Application>>().warm_up().await;
            });
        }
        #[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
        if std::env::var_os("VESSEL_SIGN_IN_DIAGNOSTICS").is_some() {
            tauri::async_runtime::spawn(native_diagnostics::sign_in_smoke(app.handle().clone()));
        }
        #[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
        if remember_probe {
            tauri::async_runtime::spawn(vessel_app::sign_in::remember_smoke(app.handle().clone()));
        }
        Ok(())
    });
    #[cfg(all(target_os = "windows", feature = "native-diagnostics"))]
    let builder = builder.on_page_load(|webview, payload| {
        if webview.label() == "main"
            && std::env::var_os("VESSEL_WINDOWS_SMOKE_REPORT").is_some()
            && payload.event() == tauri::webview::PageLoadEvent::Finished
        {
            let _ = webview.eval(windows_diagnostics::PROBE_SCRIPT);
        }
    });
    #[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
    let builder = builder.on_page_load(|webview, payload| {
        if webview.label() == "main"
            && std::env::var_os("VESSEL_NATIVE_DIAGNOSTICS").is_some()
            && payload.event() == tauri::webview::PageLoadEvent::Finished
        {
            let _ = webview.eval(native_diagnostics::PROBE_SCRIPT);
        }
    });
    let app = builder
        .invoke_handler({
            let commands: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
                native_material_probe,
                windows_smoke_probe,
                runtime_report,
                search_music,
                home_feed,
                browse_collection,
                play_song,
                local_library,
                toggle_like,
                account_library,
                refresh_account_library,
                set_account_like,
                account_playlist,
                set_material_mode,
                inspect_track,
                cancel_inspection,
                play_track,
                playback_snapshot,
                set_paused,
                seek_track,
                set_volume,
                toggle_mute,
                stop_playback,
                queue_snapshot,
                selection_snapshot,
                account_snapshot,
                next_song,
                previous_song,
                jump_queue,
                remove_queue,
                clear_queue,
                set_autoplay,
                retry_radio,
                connect_google,
                cancel_google,
                sign_out
            ];
            move |invoke: tauri::ipc::Invoke| {
                let webview = invoke.message.webview_ref();
                if !webview
                    .url()
                    .is_ok_and(|url| trusted_caller(webview.label(), &url))
                {
                    invoke.resolver.reject(vessel_app::error::AppError::new(
                        "ipc_denied",
                        "This window cannot use Vessel commands.",
                        false,
                    ));
                    return true;
                }
                commands(invoke)
            }
        })
        .build(tauri::generate_context!())
        .expect("could not start the Vessel native shell");
    app.run(|handle, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            media_controls::shutdown();
            handle.state::<Arc<SignIn>>().shutdown(handle, &handle.state::<Arc<Application>>());
            handle.state::<Arc<Application>>().shutdown();
        } else if matches!(event, tauri::RunEvent::WindowEvent { ref label, event: tauri::WindowEvent::Destroyed, .. } if label == "main") {
            handle.exit(0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_main_window_requests_a_transparent_webview_and_native_backdrop() {
        let base: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let windows: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.windows.conf.json")).unwrap();
        let window = windows
            .pointer("/app/windows/0")
            .unwrap_or(&base["app"]["windows"][0]);
        let mut merged = base.clone();
        merged["app"]["windows"] = windows["app"]["windows"].clone();
        let _: tauri::Config = serde_json::from_value(merged)
            .expect("Windows override must deserialize as real Tauri configuration");
        assert_eq!(
            window
                .get("transparent")
                .and_then(serde_json::Value::as_bool),
            Some(true),
            "CSS alpha cannot reveal the desktop through an opaque native window"
        );
        assert_eq!(window["backgroundColor"], "#00000000");
        assert!(
            window
                .pointer("/windowEffects/effects")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|effects| effects.iter().any(|effect| effect == "acrylic"))
        );
    }
    #[test]
    fn remote_windows_and_remote_navigation_cannot_invoke_app_commands() {
        assert!(trusted_caller(
            "main",
            &"tauri://localhost/index.html".parse().unwrap()
        ));
        assert!(!trusted_caller(
            "google-sign-in",
            &"https://accounts.google.com/".parse().unwrap()
        ));
        assert!(!trusted_caller(
            "main",
            &"https://accounts.google.com/".parse().unwrap()
        ));
        assert!(!trusted_caller(
            "google-sign-in",
            &"tauri://localhost/".parse().unwrap()
        ));
    }
}
