//! Opt-in actual WebView2/main-window IPC check; reports no account/provider data.
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::Manager;
use vessel_app::{application::Application, error::Result};
use webview2_com::Microsoft::Web::WebView2::Win32::{COREWEBVIEW2_COLOR, ICoreWebView2Controller2};
use windows::core::Interface;

static OPAQUE_BACKDROP_REMOVED: AtomicBool = AtomicBool::new(false);

async fn material_state(window: &tauri::WebviewWindow) -> Option<(u8, u32)> {
    let copy = window.clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    window
        .with_webview(move |platform| {
            // SAFETY: the COM controller is used only in Tauri's owning UI-thread
            // callback; the DWM call reads this window's attribute into a sized u32.
            let state = unsafe {
                let controller = platform
                    .controller()
                    .cast::<ICoreWebView2Controller2>()
                    .ok();
                controller.and_then(|controller| {
                    let mut color = COREWEBVIEW2_COLOR::default();
                    controller.DefaultBackgroundColor(&mut color).ok()?;
                    let hwnd = copy.hwnd().ok()?;
                    let mut backdrop = 0_u32;
                    let result = windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(
                        hwnd.0,
                        windows_sys::Win32::Graphics::Dwm::DWMWA_SYSTEMBACKDROP_TYPE as u32,
                        (&mut backdrop as *mut u32).cast(),
                        std::mem::size_of::<u32>() as u32,
                    );
                    (result >= 0).then_some((color.A, backdrop))
                })
            };
            let _ = send.send(state);
        })
        .ok()?;
    tokio::time::timeout(std::time::Duration::from_secs(5), receive)
        .await
        .ok()?
        .ok()?
}

pub async fn report(
    window: tauri::WebviewWindow,
    app: Arc<Application>,
    probe: Value,
) -> Result<()> {
    let destination = std::env::var_os("VESSEL_WINDOWS_SMOKE_REPORT")
        .ok_or_else(crate::vessel_diagnostic_error)?;
    let mounted = probe.get("rootPresent").and_then(Value::as_bool) == Some(true);
    let player = probe.get("playerPresent").and_then(Value::as_bool) == Some(true);
    let available = app.runtime_available();
    let material = material_state(&window).await;
    if probe.get("phase").and_then(Value::as_str) == Some("opaque") {
        OPAQUE_BACKDROP_REMOVED.store(material == Some((0, 1)), Ordering::SeqCst);
        return Ok(());
    }
    let sign_in = vessel_app::sign_in::windows_handoff_smoke(window.app_handle().clone(), app)
        .await
        .unwrap_or_else(|error| json!({"passed":false,"error":error.code}));
    let hwnd = window.hwnd().is_ok();
    let transparent = material.is_some_and(|(alpha, _)| alpha == 0);
    let backdrop = material.is_some_and(|(_, kind)| kind == 3);
    let opaque = OPAQUE_BACKDROP_REMOVED.load(Ordering::SeqCst);
    let passed = mounted
        && player
        && available
        && hwnd
        && transparent
        && backdrop
        && opaque
        && sign_in.get("passed").and_then(Value::as_bool) == Some(true)
        && probe.get("cleanDarkText").and_then(Value::as_bool) == Some(true)
        && probe.get("materialCommandFailed").and_then(Value::as_bool) != Some(true);
    let report = json!({
        "passed": passed, "webview2Mounted": mounted, "playerPresent": player,
        "nativeIpc": true, "runtimeAvailable": available, "windowHandle": hwnd,
        "mediaControlsAvailable": crate::media_controls::available(),
        "transparentWebview": transparent, "nativeAcrylicBackdrop": backdrop,
        "opaqueBackdropRemoved": opaque, "signInHandoff": sign_in,
        "cleanDarkText": probe.get("cleanDarkText").and_then(Value::as_bool) == Some(true),
        "realAccountTested": false, "audibleOutputTested": false
    });
    let written = serde_json::to_vec_pretty(&report)
        .ok()
        .is_some_and(|bytes| std::fs::write(destination, bytes).is_ok());
    window
        .app_handle()
        .exit(if passed && written { 0 } else { 1 });
    Ok(())
}

pub const PROBE_SCRIPT: &str = r#"(() => {
  if (window.__vesselWindowsProbe) return;
  window.__vesselWindowsProbe = true;
  const probe = async (attempt = 0) => {
    const root = document.querySelector('.app-frame');
    const rootPresent = Boolean(root);
    const playerPresent = Boolean(document.querySelector('.floating-player'));
    if ((!rootPresent || !playerPresent || root.getAttribute('aria-busy') !== 'false') && attempt < 100) {
      setTimeout(() => probe(attempt + 1), 100); return;
    }
    const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
    let cleanDarkText = false;
    if (root) {
      const light = root.classList.contains('light-glass');
      const strong = root.classList.contains('contrast-ink');
      root.classList.remove('light-glass'); root.classList.add('contrast-ink');
      const caption = document.querySelector('.sidebar nav > button > span');
      if (caption) {
        const ink = getComputedStyle(caption);
        cleanDarkText = parseFloat(ink.webkitTextStrokeWidth) === 0 && parseInt(ink.fontWeight) >= 500 && ink.textShadow !== 'none';
      }
      root.classList.toggle('light-glass', light); root.classList.toggle('contrast-ink', strong);
    }
    try {
      await invoke('set_material_mode', { reduced: true });
      await invoke('windows_smoke_probe', { probe: { phase: 'opaque', rootPresent, playerPresent } });
      await invoke('set_material_mode', { reduced: false });
      await invoke('windows_smoke_probe', { probe: { phase: 'clear', rootPresent, playerPresent, cleanDarkText } });
    } catch {
      invoke('windows_smoke_probe', { probe: { rootPresent, playerPresent, materialCommandFailed: true } }).catch(() => {});
    }
  };
  setTimeout(probe, 500);
})()"#;
