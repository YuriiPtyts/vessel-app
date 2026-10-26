//! Google owns the sign-in page. The ephemeral webview has no Vessel IPC privileges.
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
        let window = WebviewWindowBuilder::new(&handle, LABEL, WebviewUrl::External(START.parse().map_err(|_| AppError::internal())?))
            .title("Connect Google · Vessel")
            .inner_size(520.0, 740.0)
            .min_inner_size(420.0, 560.0)
            .center()
            .incognito(true)
            .on_navigation(move |url| {
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
