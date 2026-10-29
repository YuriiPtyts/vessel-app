//! Real WebView2 cookie/profile/handoff fixture. No Google credentials or network.
use super::*;
use crate::account::AccountMode;
use serde_json::{Value, json};

pub(super) async fn run(handle: AppHandle, app: Arc<Application>) -> Result<Value> {
    let controller = SignIn::default();
    let id = app.begin_sign_in();
    let (closed, _) = watch::channel(None);
    *controller.active.lock().map_err(|_| AppError::internal())? = Some(Attempt { id, closed });
    let window = session_window(
        &handle,
        LABEL,
        WebviewUrl::External("about:blank".parse().map_err(|_| AppError::internal())?),
    )
    .visible(false)
    .on_navigation(|url| url.as_str() == "about:blank")
    .build()
    .map_err(|_| browser_store_error())?;
    let operation = async {
        let browser = window.clone();
        let baseline = Arc::new(
            tokio::task::spawn_blocking(move || read_browser_session(&browser))
                .await
                .map_err(|_| AppError::internal())??,
        );
        let browser = window.clone();
        tokio::task::spawn_blocking(move || {
            browser
                .set_cookie(
                    tauri::webview::Cookie::build(("__Secure-1PAPISID", "windows-fixture-session"))
                        .domain(".youtube.com")
                        .path("/")
                        .secure(true)
                        .http_only(true)
                        .expires(time::OffsetDateTime::now_utc() + time::Duration::hours(1))
                        .build(),
                )
                .map_err(|_| cookie_capture_error())?;
            Ok::<_, AppError>(())
        })
        .await
        .map_err(|_| AppError::internal())??;
        let acknowledged = std::sync::atomic::AtomicBool::new(false);
        wait_for_verified_session(
            || {
                let browser = window.clone();
                let baseline = baseline.clone();
                async move {
                    tokio::task::spawn_blocking(move || {
                        // Only the top-level URL is a fixture; cookie insertion,
                        // native readback and candidate eligibility are real.
                        Ok(handoff_candidate(
                            &"https://accounts.google.com/".parse().unwrap(),
                            read_browser_session(&browser)?,
                            baseline.as_ref().as_ref(),
                        ))
                    })
                    .await
                    .map_err(|_| AppError::internal())?
                }
            },
            |_| {
                // Substitute only the provider acknowledgement. The production path
                // still requires the real catalog verifier before activation.
                acknowledged.store(true, std::sync::atomic::Ordering::SeqCst);
                std::future::ready(Ok(()))
            },
            Duration::from_millis(50),
            Duration::from_secs(2),
        )
        .await?;
        Ok::<_, AppError>(acknowledged.load(std::sync::atomic::Ordering::SeqCst))
    };
    let result = tokio::time::timeout(Duration::from_secs(10), operation)
        .await
        .map_err(|_| cookie_capture_error())
        .and_then(|result| result);
    controller.finish(&handle, &app, id);
    for _ in 0..50 {
        if handle.get_webview_window(LABEL).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let acknowledged = result?;
    let closed = handle.get_webview_window(LABEL).is_none();
    let guest = app.account().mode == AccountMode::Guest;
    Ok(
        json!({"cookieCapture": acknowledged, "candidateOnGoogleOrigin":acknowledged,"topLevelOriginIsFixture":true,"handoffClosed": closed, "noUnverifiedConnection": guest, "passed": acknowledged && closed && guest, "realAccountTested": false}),
    )
}
