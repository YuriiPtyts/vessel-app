//! Fixture-only verification of the same Keychain-to-browser restoration path.
use super::{clear_browser_data, restore_browser_cookies, session_window};
use crate::{
    account::SessionCookie,
    error::{AppError, Result},
};
use serde_json::{Value, json};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow};

struct FixtureVault {
    service: String,
    keep: bool,
}

impl FixtureVault {
    fn load(&self) -> Result<Vec<SessionCookie>> {
        match security_framework::passwords::get_generic_password(&self.service, "browser-probe") {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| AppError::internal()),
            Err(error) if error.code() == -25300 => Ok(Vec::new()),
            Err(_) => Err(AppError::internal()),
        }
    }
    fn delete(&self) -> Result<()> {
        match security_framework::passwords::delete_generic_password(&self.service, "browser-probe")
        {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => Err(AppError::internal()),
        }
    }
}

impl Drop for FixtureVault {
    fn drop(&mut self) {
        if !self.keep {
            let _ = self.delete();
        }
    }
}

async fn contains(window: WebviewWindow) -> Result<bool> {
    tokio::task::spawn_blocking(move || {
        window.cookies().map(|cookies| {
            cookies
                .iter()
                .any(|cookie| cookie.name() == "vessel_remember_probe")
        })
    })
    .await
    .map_err(|_| AppError::internal())?
    .map_err(|_| AppError::internal())
}

async fn close(handle: &AppHandle, window: WebviewWindow) -> Result<()> {
    let label = window.label().to_owned();
    window.close().map_err(|_| AppError::internal())?;
    drop(window);
    for _ in 0..100 {
        if handle.get_webview_window(&label).is_none() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(AppError::internal())
}

pub(super) async fn run(handle: &AppHandle) -> Result<Value> {
    let profile = std::env::var("VESSEL_TEST_PROFILE_ID").map_err(|_| AppError::internal())?;
    if profile.len() != 32 || !profile.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::internal());
    }
    let mut vault = FixtureVault {
        service: format!("app.vessel.desktop.remember-probe.{profile}"),
        keep: false,
    };
    let open = || {
        session_window(
            handle,
            "google-remember-probe",
            WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .visible(false)
        .build()
        .map_err(|_| AppError::internal())
    };
    let stage = std::env::var("VESSEL_REMEMBER_STAGE").unwrap_or_default();
    let cookies = if stage == "restore" {
        vault.load()?
    } else {
        vec![SessionCookie {
            domain: ".google.com".into(),
            path: "/".into(),
            name: "vessel_remember_probe".into(),
            value: "fixture".into(),
            secure: true,
            http_only: true,
            expires_at: Some(
                (time::OffsetDateTime::now_utc() + time::Duration::days(1)).unix_timestamp(),
            ),
        }]
    };
    let first = open()?;
    restore_browser_cookies(first.clone(), cookies.clone()).await?;
    let stored = contains(first.clone()).await?;
    let isolated = !contains(
        handle
            .get_webview_window("main")
            .ok_or_else(AppError::internal)?,
    )
    .await?;
    if stage != "restore" {
        security_framework::passwords::set_generic_password(
            &vault.service,
            "browser-probe",
            &serde_json::to_vec(&cookies).map_err(|_| AppError::internal())?,
        )
        .map_err(|_| AppError::internal())?;
    }
    // Erase browser data deliberately. Recovery must come from the OS vault,
    // including after a different process starts, rather than an in-memory jar.
    clear_browser_data(first.clone()).await?;
    let browser_cleared = !contains(first.clone()).await?;
    close(handle, first).await?;
    if stage == "seed" {
        vault.keep = true;
        return Ok(
            json!({"passed":stored && isolated && browser_cleared,"stored":stored,"isolated":isolated,"browserCleared":browser_cleared,"keychainSaved":!vault.load()?.is_empty()}),
        );
    }
    let second = open()?;
    restore_browser_cookies(second.clone(), vault.load()?).await?;
    let remembered = contains(second.clone()).await?;
    vault.delete()?;
    clear_browser_data(second.clone()).await?;
    close(handle, second).await?;
    let third = open()?;
    restore_browser_cookies(third.clone(), vault.load()?).await?;
    let forgotten = !contains(third.clone()).await?;
    close(handle, third).await?;
    Ok(
        json!({"passed":stored && isolated && browser_cleared && remembered && forgotten,"stored":stored,"isolated":isolated,"browserCleared":browser_cleared,"rememberedAfterClose":remembered,"forgottenAfterReopen":forgotten}),
    )
}
