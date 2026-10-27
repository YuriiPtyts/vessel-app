//! Opt-in developer verification of the actual AppKit/WKWebView material stack.
//! Compiled only with native-diagnostics; no provider identifiers or user text is collected.
use crate::vessel_diagnostic_error;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSGlassEffectView, NSView, NSVisualEffectView, NSWindow};
use serde_json::{Value, json};
use tauri::Manager;

pub fn report(window: tauri::WebviewWindow, web: Value) -> vessel_app::error::Result<()> {
    let destination =
        std::env::var_os("VESSEL_NATIVE_DIAGNOSTICS").ok_or_else(vessel_diagnostic_error)?;
    if serde_json::to_vec(&web)
        .map_err(|_| vessel_diagnostic_error())?
        .len()
        > 4096
    {
        return Err(vessel_diagnostic_error());
    }
    if MainThreadMarker::new().is_some() {
        write_report(&window, web, destination.into())
    } else {
        let copy = window.clone();
        window
            .run_on_main_thread(move || {
                let _ = write_report(&copy, web, destination.into());
            })
            .map_err(|_| vessel_diagnostic_error())
    }
}

fn write_report(
    window: &tauri::WebviewWindow,
    web: Value,
    destination: std::path::PathBuf,
) -> vessel_app::error::Result<()> {
    let _main = MainThreadMarker::new().ok_or_else(vessel_diagnostic_error)?;
    let pointer = window.ns_window().map_err(|_| vessel_diagnostic_error())?;
    // SAFETY: Tauri supplies its live NSWindow and the retained window is inspected on
    // the AppKit main thread. The pointer is not retained or used after this call.
    let native = unsafe { &*pointer.cast::<NSWindow>() };
    let mut views = Vec::new();
    if let Some(root) = native.contentView() {
        inspect_view(&root, &mut views);
    }
    let data = json!({
        "windowOpaque": native.isOpaque(),
        "windowBackgroundAlpha": native.backgroundColor().alphaComponent(),
        "views": views,
        "web": web,
    });
    std::fs::write(
        destination,
        serde_json::to_vec_pretty(&data).map_err(|_| vessel_diagnostic_error())?,
    )
    .map_err(|_| vessel_diagnostic_error())?;
    if std::env::var_os("VESSEL_NATIVE_DIAGNOSTICS_EXIT").is_some() {
        window.app_handle().exit(0);
    }
    Ok(())
}

fn inspect_view(view: &NSView, views: &mut Vec<Value>) {
    if views.len() >= 100 {
        return;
    }
    let name = view.class().name().to_string_lossy().into_owned();
    let mut data = json!({"class":name,"opaque":view.isOpaque()});
    if (name.contains("NSGlassEffectViewTagged") || name == "NSGlassEffectView")
        && let Some(glass) = view.downcast_ref::<NSGlassEffectView>()
    {
        data["glassStyle"] = json!(glass.style().0);
    } else if (name.contains("NSVisualEffectViewTagged") || name == "NSVisualEffectView")
        && let Some(visual) = view.downcast_ref::<NSVisualEffectView>()
    {
        data["material"] = json!(visual.material().0);
    }
    views.push(data);
    for child in view.subviews() {
        inspect_view(&child, views);
    }
}

pub const PROBE_SCRIPT: &str = r#"(() => {
  if (window.__vesselNativeProbeScheduled) return;
  window.__vesselNativeProbeScheduled = true;
  setTimeout(() => {
    const root = document.querySelector('.app-frame');
    const stage = document.querySelector('.main-stage');
    window.__TAURI_INTERNALS__.invoke('native_material_probe', { probe: {
      rootPresent: Boolean(root), playerPresent: Boolean(document.querySelector('.floating-player')),
      bodyLength: document.body?.textContent?.length ?? 0,
      background: root ? getComputedStyle(root).backgroundColor : null,
      tint: root ? getComputedStyle(root).getPropertyValue('--glass-opacity') : null,
      stageBackground: stage ? getComputedStyle(stage).backgroundColor : null,
      homeShelves: document.querySelectorAll('.provider-shelf').length,
      alertPresent: Boolean(document.querySelector('[role=alert]'))
    }}).catch(() => {});
  }, 3500);
})()"#;

/// Opens and cancels the real Google window without entering credentials or exporting cookies.
pub async fn sign_in_smoke(handle: tauri::AppHandle) {
    use std::{sync::Arc, time::Duration};
    use vessel_app::{application::Application, sign_in::SignIn};
    let Some(destination) = std::env::var_os("VESSEL_SIGN_IN_DIAGNOSTICS") else {
        return;
    };
    tokio::time::sleep(Duration::from_secs(4)).await;
    let app = handle.state::<Arc<Application>>().inner().clone();
    let controller = handle.state::<Arc<SignIn>>().inner().clone();
    let before = app.account();
    let library = app.library().ok();
    let mut report = json!({"opened": false, "persistentStore": null, "persistentStoreSupported": vessel_app::sign_in::persistent_browser_available(), "googleOrigin": false, "cookiesReadable": false});
    if controller.start(handle.clone(), app.clone()).await.is_ok() {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if let Some(window) = handle.get_webview_window("google-sign-in") {
            report["opened"] = json!(true);
            report["googleOrigin"] = json!(window.url().is_ok_and(
                |url| url.scheme() == "https" && url.host_str() == Some("accounts.google.com")
            ));
            let (send, receive) = tokio::sync::oneshot::channel();
            let _ = window.with_webview(move |platform| {
                // SAFETY: Tauri runs this closure on the main thread with a live WKWebView.
                // Only its configuration's persistence flag is read; no page content is read.
                let persistent = unsafe {
                    let view = platform.inner().cast::<objc2::runtime::AnyObject>();
                    let config: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![view, configuration];
                    let store: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![config, websiteDataStore];
                    let value: objc2::runtime::Bool = objc2::msg_send![store, isPersistent];
                    value.as_bool()
                };
                let _ = send.send(persistent);
            });
            report["persistentStore"] = json!(receive.await.ok());
            report["cookiesReadable"] = json!(
                tokio::task::spawn_blocking(move || window.cookies().is_ok())
                    .await
                    .unwrap_or(false)
            );
        }
    }
    let _ = controller.cancel(&handle, &app).await;
    for _ in 0..40 {
        if handle.get_webview_window("google-sign-in").is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    report["windowClosed"] = json!(handle.get_webview_window("google-sign-in").is_none());
    report["accountRestored"] =
        json!(app.account().mode == before.mode && app.account().revision == before.revision);
    report["localLibraryUnchanged"] = json!(
        app.library()
            .ok()
            .and_then(|value| serde_json::to_value(value).ok())
            == library.and_then(|value| serde_json::to_value(value).ok())
    );
    report["browserForgotten"] = json!(controller.forget(&handle, &app).await.is_ok());
    // This is an opt-in diagnostic result, containing no profile or cookie values.
    if let Ok(bytes) = serde_json::to_vec_pretty(&report) {
        let _ = std::fs::write(destination, bytes);
    }
    handle.exit(0);
}
