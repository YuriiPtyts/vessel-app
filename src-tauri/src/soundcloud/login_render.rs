//! Opt-in render/handoff diagnostics through the production sign-in controller.
//! Browser data and application/vault state are isolated from normal listening.
use super::*;
use crate::{
    account::AccountMode,
    services::{MusicProvider, MusicServices, ProviderSnapshot},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    sync::OnceLock,
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
struct FrameReport {
    origin: String,
    ready: bool,
    visible_login_fields: u64,
    resource_errors: Vec<String>,
    csp_blocked: Vec<String>,
    consent_dismissed: bool,
}

#[derive(Deserialize)]
struct FrameMessage {
    id: String,
    #[serde(flatten)]
    frame: FrameReport,
}

#[derive(Default)]
struct Probe {
    frames: Mutex<BTreeMap<String, (FrameReport, Instant)>>,
    denied: Mutex<BTreeSet<String>>,
    navigation: Mutex<Vec<Value>>,
    phases: Mutex<Vec<Value>>,
}

static PROBE: OnceLock<Probe> = OnceLock::new();
fn probe() -> &'static Probe {
    PROBE.get_or_init(Probe::default)
}

fn interactive() -> bool {
    std::env::var_os("VESSEL_SOUNDCLOUD_LOGIN_DIAGNOSTICS").is_some()
        && std::env::var_os("VESSEL_SOUNDCLOUD_LOGIN_INTERACTIVE").is_some()
}

fn push_event(events: &Mutex<Vec<Value>>, event: Value) {
    if let Ok(mut events) = events.lock()
        && events.last() != Some(&event)
        && events.len() < 100
    {
        events.push(event);
    }
}

pub(super) fn phase(stage: &'static str, error: Option<&'static str>) {
    if interactive() {
        push_event(&probe().phases, json!({"stage":stage,"errorCode":error}));
    }
}

fn navigation(url: &url::Url, accepted: bool, popup: bool) {
    let page = match url.path() {
        "/signin" | "/web-auth" => "signIn",
        "/signin/callback" => "signInCallback",
        _ => "other",
    };
    push_event(
        &probe().navigation,
        json!({"origin":safe_origin(url.as_str()),"allowed":accepted,"popupRequested":popup,"page":page}),
    );
}

pub(super) fn instrument<'a>(
    builder: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    // Interactive mode never inspects fields, changes consent, or snapshots a
    // page on which the owner may enter real credentials.
    if interactive() {
        return builder
            .on_navigation(|url| {
                let accepted = allowed(url);
                navigation(url, accepted, false);
                accepted
            })
            .on_new_window(|url, _| {
                let accepted = allowed(&url);
                navigation(&url, accepted, true);
                if accepted {
                    tauri::webview::NewWindowResponse::Allow
                } else {
                    tauri::webview::NewWindowResponse::Deny
                }
            });
    }
    builder
        .initialization_script_for_all_frames(include_str!("login_render.js"))
        .on_navigation(|url| {
            if url.scheme() == "vessel-login-render" {
                if url.host_str() == Some("report")
                    && let Some(message) = url
                        .query_pairs()
                        .find(|(key, _)| key == "data")
                        .map(|(_, value)| value.into_owned())
                        .filter(|value| value.len() < 4096)
                        .and_then(|value| parse_frame(&value))
                    && let Ok(mut frames) = probe().frames.lock()
                    && (frames.len() < 32 || frames.contains_key(&message.id))
                {
                    frames.insert(message.id, (message.frame, Instant::now()));
                }
                // A hidden transport frame never replaces the measured page
                // and receives no privileged IPC or custom-scheme response.
                return false;
            }
            let accepted = allowed(url);
            if !accepted
                && let Ok(mut denied) = probe().denied.lock()
                && denied.len() < 32
            {
                denied.insert(safe_origin(url.as_str()));
            }
            accepted
        })
}

fn parse_frame(value: &str) -> Option<FrameMessage> {
    let mut message: FrameMessage = serde_json::from_str(value).ok()?;
    if message.id.is_empty()
        || message.id.len() >= 32
        || !message.id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    message.frame.origin = safe_origin(&message.frame.origin);
    message.frame.visible_login_fields = message.frame.visible_login_fields.min(100);
    for origins in [
        &mut message.frame.resource_errors,
        &mut message.frame.csp_blocked,
    ] {
        *origins = origins
            .iter()
            .take(12)
            .map(|value| safe_origin(value))
            .collect();
    }
    Some(message)
}

fn visible_frame(frame: &FrameReport, received: &Instant) -> bool {
    received.elapsed() < Duration::from_secs(2)
        && frame.origin == "https://secure.soundcloud.com"
        && frame.ready
        && frame.visible_login_fields > 0
}

fn visible() -> bool {
    probe()
        .frames
        .lock()
        .unwrap()
        .values()
        .any(|(frame, received)| visible_frame(frame, received))
}

async fn clear_fixture(handle: &AppHandle, label: &str) -> Result<()> {
    let cleanup = browser(
        handle,
        label,
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .visible(false)
    .on_navigation(|url| url.as_str() == "about:blank")
    .build()
    .map_err(|_| login_error())?;
    let result = crate::sign_in::clear_browser_data(cleanup.clone()).await;
    let _ = cleanup.close();
    result
}

pub async fn render_smoke(handle: AppHandle) -> Result<Value> {
    probe().frames.lock().unwrap().clear();
    probe().denied.lock().unwrap().clear();
    probe().navigation.lock().unwrap().clear();
    probe().phases.lock().unwrap().clear();
    let services = handle.state::<Arc<MusicServices>>().inner().clone();
    let app = services.app(MusicProvider::SoundCloud);
    let before = services.snapshot();
    // The browser factory selects a dedicated fixture store for this flag.
    clear_fixture(&handle, "soundcloud-login-render-cleanup").await?;
    let controller = handle.state::<Arc<SoundCloudSignIn>>().inner().clone();
    if let Err(error) = controller.start(handle.clone(), app.clone()).await {
        let _ = clear_fixture(&handle, "soundcloud-login-failed-cleanup").await;
        return Err(error);
    }
    let result = if interactive() {
        if let Some(window) = handle.get_webview_window(LABEL) {
            let _ = window.set_title("SoundCloud sign-in diagnostic · Vessel");
        }
        if let Some(window) = handle.get_webview_window("main") {
            let _ = window.hide();
        }
        handoff(&handle, &services, &before).await.map(|()| false)
    } else {
        let mut rendered = false;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if visible() {
                tokio::time::sleep(Duration::from_secs(3)).await;
                if visible() {
                    rendered = true;
                    break;
                }
            }
        }
        Ok(rendered)
    };
    let cancelled = controller.cancel(&handle, &app).await;
    // Both paths clear only the fixture browser, even if writing a progress
    // report failed. Interactive sessions use an in-memory vault, not Keychain.
    let cleared = clear_fixture(&handle, "soundcloud-login-final-cleanup")
        .await
        .is_ok();
    let rendered = result?;
    cancelled?;
    let mut report = if interactive() {
        handoff_report(&services, &before, true)
    } else {
        let after = services.snapshot();
        let unchanged = before.youtube == after.youtube;
        let unverified = after.soundcloud.mode != AccountMode::Connected;
        json!({
            "mode":"credentialFreeRender",
            "passed":rendered&&unchanged&&unverified,
            "visibleLoginForm":rendered,
            "youtubeAccountUnchanged":unchanged,
            "noUnverifiedConnection":unverified,
            "frames":probe().frames.lock().unwrap().values().map(|(frame,_)|frame.clone()).collect::<Vec<_>>(),
            "blockedNavigationOrigins":*probe().denied.lock().unwrap(),
            "realCredentialsUsed":false,
            "screenshotsCaptured":false,
        })
    };
    report["diagnosticBrowserCleared"] = json!(cleared);
    if !cleared {
        report["passed"] = json!(false);
    }
    Ok(report)
}

fn handoff_report(services: &MusicServices, before: &ProviderSnapshot, finished: bool) -> Value {
    let after = services.snapshot();
    json!({
            "mode":"interactiveHandoff",
            "finished":finished,
            "passed":after.soundcloud.mode==AccountMode::Connected&&before.youtube==after.youtube,
            "accountMode":after.soundcloud.mode,
            "accountErrorCode":after.soundcloud.error.as_ref().map(|error|error.code),
            "youtubeAccountUnchanged":before.youtube==after.youtube,
            "navigation":*probe().navigation.lock().unwrap(),
            "phases":*probe().phases.lock().unwrap(),
            "inputValuesRecorded":false,
            "cookieValuesRecorded":false,
            "screenshotsCaptured":false,
    })
}

async fn handoff(
    handle: &AppHandle,
    services: &MusicServices,
    before: &ProviderSnapshot,
) -> Result<()> {
    for _ in 0..600 {
        if let Some(path) = std::env::var_os("VESSEL_SOUNDCLOUD_LOGIN_DIAGNOSTICS") {
            write_progress(
                std::path::Path::new(&path),
                &handoff_report(services, before, false),
            )?;
        }
        if handle.get_webview_window(LABEL).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(())
}

fn write_progress(path: &std::path::Path, report: &Value) -> Result<()> {
    let write = || -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(".".as_ref()))?;
        serde_json::to_writer_pretty(&mut file, report)?;
        file.flush()?;
        file.persist(path)?;
        Ok(())
    };
    write().map_err(|_| {
        AppError::new(
            "soundcloud_diagnostic",
            "The sign-in diagnostic report could not be written.",
            false,
        )
    })
}

fn safe_origin(value: &str) -> String {
    url::Url::parse(value)
        .ok()
        .map(|url| {
            if url.scheme() == "about" && matches!(url.path(), "blank" | "srcdoc") {
                format!("about:{}", url.path())
            } else if url.scheme() == "https" {
                url.origin().ascii_serialization()
            } else {
                format!("{}:", url.scheme())
            }
        })
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_report_redacts_credentials_paths_queries_and_fragments() {
        assert_eq!(
            safe_origin(
                "https://name:password@secure.soundcloud.com/signin/callback?code=private-code#private-token"
            ),
            "https://secure.soundcloud.com"
        );
        assert_eq!(safe_origin("about:blank#private-token"), "about:blank");
        assert_eq!(
            safe_origin("vessel-test://callback?token=private-token"),
            "vessel-test:"
        );
    }

    #[test]
    fn frame_transport_discards_unknown_values_and_sanitizes_resource_urls() {
        let message = parse_frame(r#"{"id":"frame1","origin":"https://secure.soundcloud.com/path?token=secret","ready":true,"visibleLoginFields":999,"resourceErrors":["https://name:secret@example.test/path?token=secret"],"cspBlocked":[],"password":"secret","cookies":"secret"}"#).unwrap();
        let report = serde_json::to_string(&message.frame).unwrap();
        assert!(!report.contains("secret"));
        assert!(!report.contains("password"));
        assert!(!report.contains("cookies"));
        assert_eq!(message.frame.visible_login_fields, 100);
        assert_eq!(message.frame.resource_errors, ["https://example.test"]);
        assert!(parse_frame(r#"{"id":"invalid/id"}"#).is_none());
    }

    #[test]
    fn only_fresh_ready_soundcloud_auth_fields_count_as_visible() {
        let mut frame = FrameReport {
            origin: "https://unrelated.test".into(),
            ready: true,
            visible_login_fields: 1,
            ..FrameReport::default()
        };
        let now = Instant::now();
        assert!(!visible_frame(&frame, &now));
        frame.origin = "https://secure.soundcloud.com".into();
        assert!(visible_frame(&frame, &now));
        assert!(!visible_frame(&frame, &(now - Duration::from_secs(3))));
        frame.ready = false;
        assert!(!visible_frame(&frame, &now));
    }
}
