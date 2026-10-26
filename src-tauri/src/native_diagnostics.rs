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
