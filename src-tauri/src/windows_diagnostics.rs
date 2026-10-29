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

#[derive(Clone, Copy)]
struct MaterialState {
    alpha: u8,
    backdrop: Option<u32>,
    accent: Option<u32>,
    mica: Option<u32>,
    media_controls: bool,
}

impl MaterialState {
    fn matches(self, effect: tauri::window::Effect) -> bool {
        match effect {
            tauri::window::Effect::Blur => self.accent == Some(3),
            tauri::window::Effect::Mica => self.backdrop == Some(2) || self.mica == Some(1),
            tauri::window::Effect::Acrylic => self.backdrop == Some(3) || self.accent == Some(4),
            _ => false,
        }
    }
    fn removed(self) -> bool {
        (self.backdrop.is_some() || self.accent.is_some() || self.mica.is_some())
            && self.backdrop.is_none_or(|value| value <= 1)
            && self.accent.is_none_or(|value| value == 0)
            && self.mica.is_none_or(|value| value == 0)
    }
}

unsafe fn read_dwm(hwnd: *mut std::ffi::c_void, attribute: u32) -> Option<u32> {
    let mut value = 0_u32;
    // SAFETY: the caller supplies the live main-window HWND on its owning UI
    // thread; this buffer has the size required by these DWORD attributes.
    let result = unsafe {
        windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            hwnd,
            attribute,
            (&mut value as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    (result >= 0).then_some(value)
}

unsafe fn read_accent(hwnd: *mut std::ffi::c_void) -> Option<u32> {
    #[repr(C)]
    #[derive(Default)]
    struct Policy {
        state: u32,
        flags: u32,
        color: u32,
        animation: u32,
    }
    #[repr(C)]
    struct Attribute {
        kind: u32,
        data: *mut std::ffi::c_void,
        size: usize,
    }
    type GetAttribute = unsafe extern "system" fn(*mut std::ffi::c_void, *mut Attribute) -> i32;
    // SAFETY: query the same sized ACCENT_POLICY/WCA_ACCENT_POLICY structure
    // used by window-vibrancy. Missing/unsupported readback remains None.
    unsafe {
        let library = libloading::Library::new("user32.dll").ok()?;
        let get = library
            .get::<GetAttribute>(b"GetWindowCompositionAttribute\0")
            .ok()?;
        let mut policy = Policy::default();
        let mut attribute = Attribute {
            kind: 0x13,
            data: (&mut policy as *mut Policy).cast(),
            size: std::mem::size_of::<Policy>(),
        };
        (get(hwnd, &mut attribute) != 0).then_some(policy.state)
    }
}

async fn material_state(window: &tauri::WebviewWindow) -> Option<MaterialState> {
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
                    // SMTC ownership is UI-thread-local, just like the controller.
                    Some(MaterialState {
                        alpha: color.A,
                        backdrop: read_dwm(
                            hwnd.0,
                            windows_sys::Win32::Graphics::Dwm::DWMWA_SYSTEMBACKDROP_TYPE as u32,
                        ),
                        accent: read_accent(hwnd.0),
                        mica: read_dwm(hwnd.0, 1029),
                        media_controls: crate::media_controls::available(),
                    })
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
        OPAQUE_BACKDROP_REMOVED.store(
            material.is_some_and(|state| state.alpha == 0 && state.removed()),
            Ordering::SeqCst,
        );
        return Ok(());
    }
    let sign_in = vessel_app::sign_in::windows_handoff_smoke(window.app_handle().clone(), app)
        .await
        .unwrap_or_else(|error| json!({"passed":false,"error":error.code}));
    let provider_isolation =
        vessel_app::soundcloud::sign_in::isolation_smoke(window.app_handle().clone())
            .await
            .unwrap_or_else(|error| json!({"passed":false,"error":error.code}));
    let hwnd = window.hwnd().is_ok();
    let build = windows_version::OsVersion::current().build;
    let selected = crate::windows_material::effect_for_build(build);
    let selected_name = match selected {
        tauri::window::Effect::Blur => "blur",
        tauri::window::Effect::Mica => "mica",
        _ => "acrylic",
    };
    let transparent = material.is_some_and(|state| state.alpha == 0);
    let backdrop = material.is_some_and(|state| state.matches(selected));
    let acrylic = material.is_some_and(|state| state.matches(tauri::window::Effect::Acrylic));
    let media_controls = material.is_some_and(|state| state.media_controls);
    let opaque = OPAQUE_BACKDROP_REMOVED.load(Ordering::SeqCst);
    let passed = mounted
        && player
        && available
        && media_controls
        && hwnd
        && transparent
        && backdrop
        && opaque
        && sign_in.get("passed").and_then(Value::as_bool) == Some(true)
        && provider_isolation.get("passed").and_then(Value::as_bool) == Some(true)
        && probe.get("cleanDarkText").and_then(Value::as_bool) == Some(true)
        && probe.get("materialCommandFailed").and_then(Value::as_bool) != Some(true);
    let report = json!({
        "passed": passed, "webview2Mounted": mounted, "playerPresent": player,
        "nativeIpc": true, "runtimeAvailable": available, "windowHandle": hwnd,
        "mediaControlsAvailable": media_controls,
        "transparentWebview": transparent, "nativeAcrylicBackdrop": acrylic,
        "windowsBuild":build,"selectedMaterial":selected_name,"nativeBackdropVerified":backdrop,
        "materialReadback":{"dwmBackdrop":material.and_then(|state|state.backdrop),"accentPolicy":material.and_then(|state|state.accent),"legacyMica":material.and_then(|state|state.mica)},
        "opaqueBackdropRemoved": opaque, "signInHandoff": sign_in,
        "providerIsolation":provider_isolation,
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
        cleanDarkText = parseFloat(ink.webkitTextStrokeWidth) === 0 && parseInt(ink.fontWeight) <= 500 && ink.textShadow === 'none' && ink.webkitTextFillColor === ink.color;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::window::Effect;

    #[test]
    fn windows_material_missing_readback_is_not_treated_as_zero_or_success() {
        let state = MaterialState {
            alpha: 0,
            backdrop: None,
            accent: None,
            mica: None,
            media_controls: true,
        };
        assert!(!state.removed());
        for effect in [Effect::Blur, Effect::Mica, Effect::Acrylic] {
            assert!(!state.matches(effect));
        }
    }

    #[test]
    fn windows_material_readback_distinguishes_legacy_blur_and_modern_acrylic() {
        let mut state = MaterialState {
            alpha: 0,
            backdrop: None,
            accent: Some(3),
            mica: None,
            media_controls: true,
        };
        assert!(state.matches(Effect::Blur));
        assert!(!state.matches(Effect::Acrylic) && !state.removed());
        state.accent = Some(0);
        assert!(state.removed());
        state.backdrop = Some(3);
        assert!(state.matches(Effect::Acrylic) && !state.removed());
    }
}
