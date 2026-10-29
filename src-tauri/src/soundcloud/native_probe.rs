use super::*;
use crate::services::{MusicProvider, MusicServices};
use serde_json::json;

pub async fn isolation_smoke(handle: AppHandle) -> Result<serde_json::Value> {
    let services = handle.state::<Arc<MusicServices>>().inner().clone();
    let before = services.snapshot();
    let cloud = browser(
        &handle,
        "soundcloud-provider-probe",
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .visible(false)
    .on_navigation(|url| url.as_str() == "about:blank")
    .build()
    .map_err(|_| login_error())?;
    let google = crate::sign_in::session_window(
        &handle,
        "youtube-provider-probe",
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .visible(false)
    .on_navigation(|url| url.as_str() == "about:blank")
    .build()
    .map_err(|_| login_error())?;
    let copy = cloud.clone();
    let youtube = google.clone();
    let checked = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::task::spawn_blocking(move || {
            copy.set_cookie(
                tauri::webview::Cookie::build(("oauth_token", "fixture-soundcloud-session-native"))
                    .domain(".soundcloud.com")
                    .path("/")
                    .secure(true)
                    .http_only(true)
                    .build(),
            )
            .map_err(|_| login_error())?;
            youtube
                .set_cookie(
                    tauri::webview::Cookie::build(("vessel_provider_fixture", "youtube-only"))
                        .domain(".youtube.com")
                        .path("/")
                        .secure(true)
                        .http_only(true)
                        .build(),
                )
                .map_err(|_| login_error())?;
            let capture = read_session(&copy)?.is_some();
            let isolated = copy
                .cookies_for_url("https://youtube.com/".parse().unwrap())
                .map_err(|_| login_error())?
                .iter()
                .all(|cookie| cookie.name() != "vessel_provider_fixture")
                && youtube
                    .cookies_for_url("https://soundcloud.com/".parse().unwrap())
                    .map_err(|_| login_error())?
                    .iter()
                    .all(|cookie| cookie.name() != "oauth_token");
            Ok::<_, AppError>((capture, isolated))
        }),
    )
    .await;
    let _ = cloud.close();
    let _ = google.close();
    for _ in 0..50 {
        if handle
            .get_webview_window("soundcloud-provider-probe")
            .is_none()
            && handle
                .get_webview_window("youtube-provider-probe")
                .is_none()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (capture, isolated) = checked
        .map_err(|_| login_error())?
        .map_err(|_| login_error())??;
    let after = services.snapshot();
    let youtube_unchanged = before.youtube == after.youtube;
    let cloud_unchanged = before.soundcloud == after.soundcloud;
    let closed = handle
        .get_webview_window("soundcloud-provider-probe")
        .is_none()
        && handle
            .get_webview_window("youtube-provider-probe")
            .is_none();
    let no_connection = services.app(MusicProvider::SoundCloud).account().mode
        != crate::account::AccountMode::Connected;
    Ok(
        json!({"passed":capture&&isolated&&youtube_unchanged&&cloud_unchanged&&closed&&no_connection,"cookieCapture":capture,"browserStoresIsolated":isolated,"youtubeAccountUnchanged":youtube_unchanged,"soundcloudAccountUnchanged":cloud_unchanged,"noUnverifiedConnection":no_connection,"windowsClosed":closed,"realAccountTested":false}),
    )
}
