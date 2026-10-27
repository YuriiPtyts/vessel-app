//! Opt-in actual WebView2/main-window IPC check; reports no account/provider data.
use serde_json::{Value, json};
use tauri::Manager;
use vessel_app::{application::Application, error::Result};

pub fn report(window: tauri::WebviewWindow, app: &Application, probe: Value) -> Result<()> {
    let destination = std::env::var_os("VESSEL_WINDOWS_SMOKE_REPORT")
        .ok_or_else(crate::vessel_diagnostic_error)?;
    let mounted = probe.get("rootPresent").and_then(Value::as_bool) == Some(true);
    let player = probe.get("playerPresent").and_then(Value::as_bool) == Some(true);
    let available = app.runtime_available();
    let copy = window.clone();
    window
        .run_on_main_thread(move || {
            let hwnd = copy.hwnd().is_ok();
            let passed = mounted && player && available && hwnd;
            let report = json!({
                "passed": passed, "webview2Mounted": mounted, "playerPresent": player,
                "nativeIpc": true, "runtimeAvailable": available, "windowHandle": hwnd,
                "mediaControlsAvailable": crate::media_controls::available(),
                "realAccountTested": false, "audibleOutputTested": false
            });
            let written = serde_json::to_vec_pretty(&report)
                .ok()
                .is_some_and(|bytes| std::fs::write(destination, bytes).is_ok());
            copy.app_handle()
                .exit(if passed && written { 0 } else { 1 });
        })
        .map_err(|_| crate::vessel_diagnostic_error())
}

pub const PROBE_SCRIPT: &str = r#"(() => {
  if (window.__vesselWindowsProbe) return;
  window.__vesselWindowsProbe = true;
  const probe = (attempt = 0) => {
    const rootPresent = Boolean(document.querySelector('.app-frame'));
    const playerPresent = Boolean(document.querySelector('.floating-player'));
    if ((!rootPresent || !playerPresent) && attempt < 100) {
      setTimeout(() => probe(attempt + 1), 100); return;
    }
    window.__TAURI_INTERNALS__.invoke('windows_smoke_probe', {
      probe: { rootPresent, playerPresent }
    }).catch(() => {});
  };
  setTimeout(probe, 500);
})()"#;
