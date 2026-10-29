//! Independent native SoundCloud browser. It never has privileged Vessel IPC.
use crate::{
    account::{AccountSnapshot, SessionCookie},
    application::Application,
    error::{AppError, Result},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tokio::sync::{Mutex as AsyncMutex, watch};

const LABEL: &str = "soundcloud-sign-in";
#[cfg(target_os = "macos")]
const PROFILE: [u8; 16] = 0xc37edb746c3b4b929ca4d193166c5861_u128.to_be_bytes();
struct Attempt {
    id: u64,
    closed: watch::Sender<()>,
}
#[derive(Default)]
pub struct SoundCloudSignIn {
    active: Mutex<Option<Attempt>>,
    operation: AsyncMutex<()>,
}

fn browser<'a>(
    handle: &'a AppHandle,
    label: &str,
    url: WebviewUrl,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let builder = WebviewWindowBuilder::new(handle, label, url);
    #[cfg(target_os = "macos")]
    if crate::sign_in::persistent_browser_available() {
        let render_probe = cfg!(feature = "native-diagnostics")
            && std::env::var_os("VESSEL_SOUNDCLOUD_LOGIN_DIAGNOSTICS").is_some();
        let diagnostic = cfg!(feature = "native-diagnostics")
            && (std::env::var_os("VESSEL_PROVIDER_DIAGNOSTICS").is_some()
                || std::env::var_os("VESSEL_WINDOWS_SMOKE_REPORT").is_some());
        return builder
            .incognito(false)
            .data_store_identifier(if render_probe {
                0xc37edb746c3b4b929ca4d193166c5865_u128.to_be_bytes()
            } else if diagnostic {
                0xc37edb746c3b4b929ca4d193166c5862_u128.to_be_bytes()
            } else {
                PROFILE
            });
    }
    #[cfg(target_os = "windows")]
    let builder = if let Ok(path) = handle.path().app_data_dir() {
        builder.data_directory(path.join(
            if cfg!(feature = "native-diagnostics")
                && std::env::var_os("VESSEL_SOUNDCLOUD_LOGIN_DIAGNOSTICS").is_some()
            {
                "soundcloud-login-render-diagnostics"
            } else if cfg!(feature = "native-diagnostics")
                && std::env::var_os("VESSEL_WINDOWS_SMOKE_REPORT").is_some()
            {
                "soundcloud-browser-diagnostics"
            } else {
                "soundcloud-browser"
            },
        ))
    } else {
        builder
    };
    builder.incognito(true)
}
fn allowed(url: &url::Url) -> bool {
    if url.as_str() == "about:blank" {
        return true;
    }
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.host_str().is_some_and(|host| {
            host == "soundcloud.com"
                || host.ends_with(".soundcloud.com")
                || host == "accounts.google.com"
                || host == "consent.google.com"
                || host == "appleid.apple.com"
                || host == "www.facebook.com"
                || host == "m.facebook.com"
                || host == "facebook.com"
        })
}
fn capture(window: &WebviewWindow) -> Result<Option<(String, Vec<SessionCookie>)>> {
    let url = window.url().map_err(|_| login_error())?;
    if !allowed(&url)
        || !url
            .host_str()
            .is_some_and(|host| host == "soundcloud.com" || host.ends_with(".soundcloud.com"))
    {
        return Ok(None);
    }
    read_session(window)
}
fn read_session(window: &WebviewWindow) -> Result<Option<(String, Vec<SessionCookie>)>> {
    let cookies = window
        .cookies_for_url("https://soundcloud.com/".parse().unwrap())
        .map_err(|_| login_error())?;
    Ok(super::auth::token_from_cookies(
        cookies
            .into_iter()
            .map(|cookie| SessionCookie {
                domain: cookie.domain().unwrap_or("soundcloud.com").into(),
                path: cookie.path().unwrap_or("/").into(),
                name: cookie.name().into(),
                value: cookie.value().into(),
                secure: cookie.secure().unwrap_or(false),
                http_only: cookie.http_only().unwrap_or(false),
                expires_at: cookie.expires_datetime().map(|time| time.unix_timestamp()),
            })
            .collect(),
    ))
}
impl SoundCloudSignIn {
    pub async fn start(
        self: &Arc<Self>,
        handle: AppHandle,
        app: Arc<Application>,
    ) -> Result<AccountSnapshot> {
        let _operation = self.operation.lock().await;
        let cloud = app
            .soundcloud_service()
            .cloned()
            .ok_or_else(AppError::internal)?;
        if self
            .active
            .lock()
            .map_err(|_| AppError::internal())?
            .is_some()
            && let Some(window) = handle.get_webview_window(LABEL)
        {
            let _ = window.set_focus();
            return Ok(cloud.account());
        }
        let id = cloud.auth.begin(false);
        let (closed, mut receiver) = watch::channel(());
        *self.active.lock().map_err(|_| AppError::internal())? = Some(Attempt {
            id,
            closed: closed.clone(),
        });
        let builder = browser(
            &handle,
            LABEL,
            WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .title("Connect SoundCloud · Vessel")
        .inner_size(560.0, 760.0)
        .min_inner_size(420.0, 560.0)
        .center()
        .on_navigation(allowed)
        .on_new_window(|url, _| {
            if allowed(&url) {
                tauri::webview::NewWindowResponse::Allow
            } else {
                tauri::webview::NewWindowResponse::Deny
            }
        });
        #[cfg(feature = "native-diagnostics")]
        let builder = if std::env::var_os("VESSEL_SOUNDCLOUD_LOGIN_DIAGNOSTICS").is_some() {
            login_render::instrument(builder)
        } else {
            builder
        };
        let window = builder.build();
        let window = match window {
            Ok(window) => window,
            Err(_) => {
                cloud.auth.fail(id, login_error());
                self.finish(&handle, &app, id);
                return Err(login_error());
            }
        };
        #[cfg(feature = "native-diagnostics")]
        login_render::phase("browserOpened", None);
        window.on_window_event(move |event| {
            if matches!(event, WindowEvent::Destroyed) {
                closed.send_replace(());
            }
        });
        let saved = cloud.auth.saved().await;
        if let Ok(Some(saved)) = saved {
            let copy = window.clone();
            let _ = tokio::task::spawn_blocking(move || {
                for cookie in saved.cookies {
                    let value = tauri::webview::Cookie::build((cookie.name, cookie.value))
                        .domain(cookie.domain)
                        .path(cookie.path)
                        .secure(cookie.secure)
                        .http_only(cookie.http_only)
                        .build();
                    let _ = copy.set_cookie(value);
                }
            })
            .await;
        }
        if window
            .navigate("https://soundcloud.com/signin".parse().unwrap())
            .is_err()
        {
            cloud.auth.fail(id, login_error());
            self.finish(&handle, &app, id);
            return Err(login_error());
        }
        let controller = Arc::downgrade(self);
        let snapshot = cloud.account();
        tauri::async_runtime::spawn(async move {
            let work = async {
                let mut verification_start = None;
                loop {
                    let copy = window.clone();
                    let candidate = tokio::time::timeout(
                        Duration::from_secs(10),
                        tokio::task::spawn_blocking(move || capture(&copy)),
                    )
                    .await
                    .map_err(|_| login_error())?
                    .map_err(|_| login_error())??;
                    if let Some((token, cookies)) = candidate {
                        #[cfg(feature = "native-diagnostics")]
                        login_render::phase("sessionCandidateCaptured", None);
                        let start = *verification_start.get_or_insert_with(Instant::now);
                        let remaining = Duration::from_secs(60)
                            .checked_sub(start.elapsed())
                            .ok_or_else(login_error)?;
                        let _ = window.set_title("Finishing SoundCloud connection · Vessel");
                        let verified =
                            tokio::time::timeout(remaining, cloud.verify(&token, cookies)).await;
                        #[cfg(feature = "native-diagnostics")]
                        match &verified {
                            Ok(Ok(_)) => login_render::phase("sessionVerified", None),
                            Ok(Err(error)) => {
                                login_render::phase("sessionVerificationFailed", Some(error.code))
                            }
                            Err(_) => login_render::phase("sessionVerificationTimedOut", None),
                        }
                        if let Ok(Ok(session)) = verified {
                            cloud.auth.activate(id, session, true).await?;
                            #[cfg(feature = "native-diagnostics")]
                            login_render::phase("sessionActivated", None);
                            return Ok::<_, AppError>(());
                        }
                        if start.elapsed() >= Duration::from_secs(60) {
                            return Err(login_error());
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                }
            };
            let result = tokio::select! {result=work=>result,_=receiver.changed()=>Err(AppError::cancelled()),_=tokio::time::sleep(Duration::from_secs(900))=>Err(login_error())};
            #[cfg(feature = "native-diagnostics")]
            if let Err(error) = &result {
                login_render::phase("attemptEnded", Some(error.code));
            }
            if let Err(error) = result
                && error.code != "cancelled"
            {
                cloud.auth.fail(id, error);
            }
            if let Some(controller) = controller.upgrade() {
                controller.finish(&handle, &app, id);
            }
        });
        Ok(snapshot)
    }
    fn finish(&self, handle: &AppHandle, app: &Application, id: u64) {
        if let Ok(mut active) = self.active.lock()
            && active.as_ref().is_some_and(|attempt| attempt.id == id)
        {
            if let Some(attempt) = active.take() {
                attempt.closed.send_replace(());
            }
            if let Some(cloud) = app.soundcloud_service() {
                cloud.auth.cancel(id);
            }
            if let Some(window) = handle.get_webview_window(LABEL) {
                let _ = window.close();
            }
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }
    }
    pub fn shutdown(&self, handle: &AppHandle, app: &Application) {
        let id = self
            .active
            .lock()
            .ok()
            .and_then(|active| active.as_ref().map(|attempt| attempt.id));
        if let Some(id) = id {
            self.finish(handle, app, id);
        }
    }
    pub async fn cancel(&self, handle: &AppHandle, app: &Application) -> Result<AccountSnapshot> {
        let _operation = self.operation.lock().await;
        self.shutdown(handle, app);
        Ok(app.account())
    }
    pub async fn forget(&self, handle: &AppHandle, app: &Application) -> Result<AccountSnapshot> {
        let _operation = self.operation.lock().await;
        self.shutdown(handle, app);
        let cleanup = browser(
            handle,
            "soundcloud-session-cleanup",
            WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .visible(false)
        .on_navigation(|url| url.as_str() == "about:blank")
        .build()
        .map_err(|_| login_error())?;
        let cleared = crate::sign_in::clear_browser_data(cleanup.clone()).await;
        let _ = cleanup.close();
        let cloud = app.soundcloud_service().ok_or_else(AppError::internal)?;
        let snapshot = cloud.auth.sign_out().await?;
        app.stop().await?;
        cleared.map_err(|_| login_error())?;
        Ok(snapshot)
    }
}
fn login_error() -> AppError {
    AppError::new(
        "soundcloud_sign_in",
        "SoundCloud could not verify this browser session. Try connecting again; your YouTube Music sign-in is separate.",
        true,
    )
}

#[cfg(feature = "native-diagnostics")]
#[path = "native_probe.rs"]
mod native_probe;
#[cfg(feature = "native-diagnostics")]
pub use native_probe::isolation_smoke;
#[cfg(feature = "native-diagnostics")]
#[path = "login_render.rs"]
mod login_render;
#[cfg(feature = "native-diagnostics")]
pub use login_render::render_smoke;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn soundcloud_browser_navigation_is_https_and_provider_scoped() {
        for url in [
            "https://soundcloud.com/signin",
            "https://secure.soundcloud.com/authorize",
            "https://accounts.google.com/",
        ] {
            assert!(allowed(&url.parse().unwrap()));
        }
        for url in [
            "http://soundcloud.com/",
            "https://soundcloud.com.evil.test/",
            "https://user@soundcloud.com/",
            "https://soundcloud.com:8443/",
            "tauri://localhost",
        ] {
            assert!(!allowed(&url.parse().unwrap()));
        }
    }
}
