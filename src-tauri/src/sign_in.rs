//! Google owns the sign-in page. Its isolated browser profile has no Vessel IPC privileges.
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tokio::sync::{Mutex as AsyncMutex, watch};

use crate::{
    account::{AccountSnapshot, CookieSession, SessionCookie},
    application::Application,
    error::{AppError, Result},
};

const LABEL: &str = "google-sign-in";
const START: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F&hl=en";

fn session_window<'a>(
    handle: &'a AppHandle,
    label: &str,
    url: WebviewUrl,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let builder = WebviewWindowBuilder::new(handle, label, url);
    #[cfg(target_os = "macos")]
    if persistent_browser_available() {
        let identifier = if cfg!(feature = "native-diagnostics")
            && (std::env::var_os("VESSEL_REMEMBER_DIAGNOSTICS").is_some()
                || std::env::var_os("VESSEL_SIGN_IN_DIAGNOSTICS").is_some())
        {
            0xc37edb746c3b4b929ca4d193166c5860_u128.to_be_bytes()
        } else {
            0xc37edb746c3b4b929ca4d193166c5859_u128.to_be_bytes()
        };
        #[cfg(feature = "native-diagnostics")]
        let identifier = {
            let mut selected = identifier;
            if std::env::var_os("VESSEL_REMEMBER_DIAGNOSTICS").is_some()
                && let Ok(value) = std::env::var("VESSEL_TEST_PROFILE_ID")
                && value.is_ascii()
                && value.len() == 32
            {
                let bytes = (0..16)
                    .map(|index| u8::from_str_radix(&value[index * 2..index * 2 + 2], 16))
                    .collect::<std::result::Result<Vec<_>, _>>();
                if let Ok(bytes) = bytes {
                    selected.copy_from_slice(&bytes);
                }
            }
            selected
        };
        return builder.incognito(false).data_store_identifier(identifier);
    }
    // Older macOS lacks named stores. Keep browser isolation and Keychain restore
    // rather than sharing/clearing the main window's preferences store.
    builder.incognito(true)
}

#[cfg(target_os = "macos")]
pub fn persistent_browser_available() -> bool {
    objc2_foundation::NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion
        >= 14
}

#[cfg(target_os = "macos")]
async fn clear_browser_data(window: WebviewWindow) -> Result<()> {
    let (send, receive) = tokio::sync::oneshot::channel();
    window.with_webview(move |platform| {
        let send = Mutex::new(Some(send));
        let completion = block2::RcBlock::new(move || {
            if let Some(send) = send.lock().ok().and_then(|mut send| send.take()) { let _ = send.send(()); }
        });
        // SAFETY: Tauri supplies a live WKWebView on the main thread. The store
        // copies the completion block and retains all data needed by deletion.
        unsafe {
            let view = platform.inner().cast::<objc2::runtime::AnyObject>();
            let config: *mut objc2::runtime::AnyObject = objc2::msg_send![view, configuration];
            let store: *mut objc2::runtime::AnyObject = objc2::msg_send![config, websiteDataStore];
            let types: *mut objc2::runtime::AnyObject = objc2::msg_send![objc2::class!(WKWebsiteDataStore), allWebsiteDataTypes];
            let date: *mut objc2::runtime::AnyObject = objc2::msg_send![objc2::class!(NSDate), dateWithTimeIntervalSince1970: 0.0_f64];
            let _: () = objc2::msg_send![store, removeDataOfTypes: types, modifiedSince: date, completionHandler: &*completion];
        }
    }).map_err(|_| browser_store_error())?;
    tokio::time::timeout(Duration::from_secs(15), receive)
        .await
        .map_err(|_| browser_store_error())?
        .map_err(|_| browser_store_error())
}

#[cfg(not(target_os = "macos"))]
async fn clear_browser_data(window: WebviewWindow) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        window
            .clear_all_browsing_data()
            .map_err(|_| browser_store_error())?;
        // WebKit deletion is asynchronous. Confirm cookies are gone before a
        // later Connect operation may reuse this profile.
        for _ in 0..100 {
            if window
                .cookies()
                .map_err(|_| browser_store_error())?
                .is_empty()
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(browser_store_error())
    })
    .await
    .map_err(|_| browser_store_error())?
}

fn browser_store_error() -> AppError {
    AppError::new(
        "sign_in_storage",
        "The remembered Google browser session could not be updated. Try reconnecting or forgetting the saved session.",
        true,
    )
}

async fn restore_browser_cookies(window: WebviewWindow, cookies: Vec<SessionCookie>) -> Result<()> {
    let cookies = CookieSession::retained_browser_cookies(cookies)?;
    if !cookies.is_empty() {
        // The verified vault is authoritative; do not mix another cancelled
        // browser login's cookies with the remembered account.
        clear_browser_data(window.clone()).await?;
    }
    tokio::task::spawn_blocking(move || {
        for cookie in cookies {
            let mut restored = tauri::webview::Cookie::build((cookie.name, cookie.value))
                .domain(cookie.domain)
                .path(cookie.path)
                .secure(cookie.secure)
                .http_only(cookie.http_only)
                .build();
            if let Some(expiry) = cookie
                .expires_at
                .and_then(|value| time::OffsetDateTime::from_unix_timestamp(value).ok())
            {
                restored.set_expires(expiry);
            }
            window
                .set_cookie(restored)
                .map_err(|_| browser_store_error())?;
        }
        Ok(())
    })
    .await
    .map_err(|_| browser_store_error())?
}

struct Attempt {
    id: u64,
    closed: watch::Sender<Option<AppError>>,
}

#[derive(Default)]
pub struct SignIn {
    active: Mutex<Option<Attempt>>,
    operation: AsyncMutex<()>,
}

impl SignIn {
    pub async fn start(
        self: &Arc<Self>,
        handle: AppHandle,
        app: Arc<Application>,
    ) -> Result<AccountSnapshot> {
        let _operation = self.operation.lock().await;
        if self
            .active
            .lock()
            .map_err(|_| AppError::internal())?
            .is_some()
            && let Some(window) = handle.get_webview_window(LABEL)
        {
            let _ = window.set_focus();
            return Ok(app.account());
        }
        let id = app.begin_sign_in();
        let (closed, mut receiver) = watch::channel(None);
        *self.active.lock().map_err(|_| AppError::internal())? = Some(Attempt {
            id,
            closed: closed.clone(),
        });
        let denied = closed.clone();
        let window = session_window(&handle, LABEL, WebviewUrl::External("about:blank".parse().map_err(|_| AppError::internal())?))
            .title("Connect Google · Vessel")
            .inner_size(520.0, 740.0)
            .min_inner_size(420.0, 560.0)
            .center()
            .on_navigation(move |url| {
                if url.as_str() == "about:blank" { return true; }
                if google_rejected(url) {
                    denied.send_replace(Some(AppError::new("google_sign_in_unavailable", "Google declined sign-in in this window. You can close it and keep listening as a guest.", false)));
                    return false;
                }
                allowed_navigation(url)
            })
            .build();
        let window = match window {
            Ok(window) => window,
            Err(_) => {
                let error = AppError::new(
                    "sign_in_window",
                    "The Google sign-in window could not open. Try again.",
                    true,
                );
                app.fail_sign_in(id, error.clone());
                self.finish(&handle, &app, id);
                return Err(error);
            }
        };
        window.on_window_event(move |event| {
            if matches!(event, WindowEvent::Destroyed) {
                closed.send_replace(None);
            }
        });
        let restored = async {
            let cookies = app.saved_browser_cookies().await?;
            restore_browser_cookies(window.clone(), cookies).await?;
            window
                .navigate(START.parse().map_err(|_| AppError::internal())?)
                .map_err(|_| AppError::internal())
        }
        .await;
        if let Err(error) = restored {
            app.fail_sign_in(id, error.clone());
            self.finish(&handle, &app, id);
            return Err(error);
        }
        let controller = Arc::downgrade(self);
        let state = app.account();
        tauri::async_runtime::spawn(async move {
            let capture = async {
                let mut interval = tokio::time::interval(Duration::from_millis(1500));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    let window = window.clone();
                    // Tauri's cookie API dispatches to the main thread; never call it there.
                    let session = tokio::task::spawn_blocking(move || capture_session(&window))
                        .await
                        .map_err(|_| AppError::internal())??;
                    if let Some(session) = session {
                        app.complete_sign_in(id, session).await?;
                        return Ok::<(), AppError>(());
                    }
                }
            };
            let result = tokio::select! {
                result = capture => result,
                _ = receiver.changed() => {
                    if let Some(error) = receiver.borrow().clone() { Err(error) } else { Err(AppError::cancelled()) }
                },
                _ = tokio::time::sleep(Duration::from_secs(15 * 60)) => Err(AppError::new("sign_in_timeout", "Google sign-in timed out. Connect again when you are ready; guest listening remains available.", true)),
            };
            if let Err(error) = result
                && error.code != "cancelled"
            {
                app.fail_sign_in(id, error);
            }
            if let Some(controller) = controller.upgrade() {
                controller.finish(&handle, &app, id);
            }
        });
        Ok(state)
    }

    pub async fn cancel(&self, handle: &AppHandle, app: &Application) -> Result<AccountSnapshot> {
        let _operation = self.operation.lock().await;
        self.shutdown(handle, app);
        Ok(app.account())
    }

    pub async fn forget(&self, handle: &AppHandle, app: &Application) -> Result<AccountSnapshot> {
        let _operation = self.operation.lock().await;
        self.shutdown(handle, app);
        let browser = async {
            let cleanup = session_window(
                handle,
                "google-session-cleanup",
                WebviewUrl::External("about:blank".parse().map_err(|_| browser_store_error())?),
            )
            .visible(false)
            .on_navigation(|url| url.as_str() == "about:blank")
            .build()
            .map_err(|_| browser_store_error())?;
            let result = clear_browser_data(cleanup.clone()).await;
            let _ = cleanup.close();
            result
        };
        // Browser forgetting must still run if playback stop fails or stalls.
        let (snapshot, browser) = tokio::join!(app.sign_out(), browser);
        let snapshot = snapshot?;
        browser?;
        Ok(snapshot)
    }

    pub fn shutdown(&self, handle: &AppHandle, app: &Application) {
        let id = self
            .active
            .lock()
            .ok()
            .and_then(|active| active.as_ref().map(|attempt| attempt.id));
        if let Some(id) = id {
            self.finish(handle, app, id);
        } else {
            app.cancel_account_connection();
        }
    }

    fn finish(&self, handle: &AppHandle, app: &Application, id: u64) {
        let attempt = {
            let Ok(mut active) = self.active.lock() else {
                return;
            };
            if active.as_ref().is_none_or(|attempt| attempt.id != id) {
                return;
            }
            active.take()
        };
        if let Some(attempt) = attempt {
            attempt.closed.send_replace(None);
            app.cancel_sign_in(id);
            if let Some(window) = handle.get_webview_window(LABEL) {
                let _ = window.close();
            }
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }
    }
}

#[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
mod remember_probe;

#[cfg(all(target_os = "macos", feature = "native-diagnostics"))]
pub async fn remember_smoke(handle: AppHandle) {
    let Some(destination) = std::env::var_os("VESSEL_REMEMBER_DIAGNOSTICS") else {
        return;
    };
    let result = remember_probe::run(&handle).await;
    let data =
        result.unwrap_or_else(|error| serde_json::json!({"passed":false,"error":error.code}));
    let _ = std::fs::write(
        destination,
        serde_json::to_vec_pretty(&data).unwrap_or_default(),
    );
    // The harness requests a normal native quit after receiving this report,
    // allowing WebKit to finish its application-termination lifecycle.
}

fn capture_session(window: &WebviewWindow) -> Result<Option<CookieSession>> {
    let url = window.url().map_err(|_| AppError::cancelled())?;
    if url.scheme() != "https" || url.host_str() != Some("music.youtube.com") {
        return Ok(None);
    }
    let cookies = window.cookies().map_err(|_| {
        AppError::new(
            "sign_in_cookies",
            "The Google session could not be read. Close this window and try connecting again.",
            true,
        )
    })?;
    let cookies = cookies
        .into_iter()
        .map(|cookie| SessionCookie {
            domain: cookie.domain().unwrap_or("music.youtube.com").into(),
            path: cookie.path().unwrap_or("/").into(),
            name: cookie.name().into(),
            value: cookie.value().into(),
            secure: cookie.secure().unwrap_or(false),
            http_only: cookie.http_only().unwrap_or(false),
            expires_at: cookie
                .expires_datetime()
                .map(|expiry| expiry.unix_timestamp()),
        })
        .collect();
    // Anonymous YouTube cookies are not an account and are never persisted.
    Ok(CookieSession::from_cookies(cookies).ok())
}

fn allowed_navigation(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && matches!(
            url.host_str(),
            Some(
                "accounts.google.com"
                    | "consent.google.com"
                    | "myaccount.google.com"
                    | "accounts.youtube.com"
                    | "youtube.com"
                    | "www.youtube.com"
                    | "music.youtube.com"
            )
        )
}

fn google_rejected(url: &url::Url) -> bool {
    url.host_str() == Some("accounts.google.com")
        && (url.path().to_ascii_lowercase().contains("deniedsignin")
            || url
                .query_pairs()
                .any(|(key, value)| key == "error" && value == "disallowed_useragent"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sign_in_navigation_stays_on_google_and_youtube_https_origins() {
        assert!(allowed_navigation(&START.parse().unwrap()));
        assert!(allowed_navigation(
            &"https://music.youtube.com/".parse().unwrap()
        ));
        for address in [
            "https://accounts.google.com.attacker.test/",
            "http://accounts.google.com/",
            "https://music.youtube.com@attacker.test/",
            "file:///tmp/page",
            "tauri://localhost/",
        ] {
            assert!(!allowed_navigation(&address.parse().unwrap()));
        }
        assert!(google_rejected(
            &"https://accounts.google.com/signin/v2/deniedsignin"
                .parse()
                .unwrap()
        ));
    }
}
