//! SoundCloud's verified browser session has its own vault and revision domain.
use crate::{
    account::{AccountMode, AccountProfile, AccountSnapshot, SessionCookie, SessionVault},
    error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{Mutex as AsyncMutex, watch};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Session {
    pub token: String,
    pub user_id: u64,
    pub profile: AccountProfile,
    pub cookies: Vec<SessionCookie>,
}

pub(crate) struct Auth {
    state: Mutex<(AccountSnapshot, Option<Arc<Session>>)>,
    changes: watch::Sender<AccountSnapshot>,
    attempt: AtomicU64,
    mutation: AsyncMutex<()>,
    vault: Arc<dyn SessionVault>,
}

impl Auth {
    pub fn new(vault: Arc<dyn SessionVault>) -> Arc<Self> {
        let snapshot = AccountSnapshot {
            mode: AccountMode::Guest,
            profile: None,
            error: None,
            revision: 0,
        };
        let (changes, _) = watch::channel(snapshot.clone());
        Arc::new(Self {
            state: Mutex::new((snapshot, None)),
            changes,
            attempt: AtomicU64::new(0),
            mutation: AsyncMutex::new(()),
            vault,
        })
    }
    pub fn snapshot(&self) -> AccountSnapshot {
        self.changes.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<AccountSnapshot> {
        self.changes.subscribe()
    }
    pub fn session(&self) -> (u64, Option<Arc<Session>>) {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        (state.0.revision, state.1.clone())
    }
    fn update(&self, change: impl FnOnce(&mut (AccountSnapshot, Option<Arc<Session>>))) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        change(&mut state);
        self.changes.send_replace(state.0.clone());
    }
    pub fn begin(&self, restoring: bool) -> u64 {
        let id = self.attempt.fetch_add(1, Ordering::SeqCst) + 1;
        self.update(|state| {
            state.0.mode = if restoring {
                AccountMode::Checking
            } else {
                AccountMode::SigningIn
            };
            state.0.error = None;
        });
        id
    }
    pub fn cancel(&self, id: u64) {
        if self
            .attempt
            .compare_exchange(id, id + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.update(|state| {
                if !matches!(state.0.mode, AccountMode::SigningIn | AccountMode::Checking) {
                    return;
                }
                state.0.mode = if state.1.is_some() {
                    AccountMode::Connected
                } else {
                    AccountMode::Guest
                };
                state.0.error = None;
            });
        }
    }
    pub fn fail(&self, id: u64, error: AppError) {
        if self.attempt.load(Ordering::SeqCst) == id {
            self.update(|state| {
                state.0.mode = if state.1.is_some() {
                    AccountMode::Connected
                } else {
                    AccountMode::Unavailable
                };
                state.0.error = Some(error);
            });
        }
    }
    pub fn expire(&self, revision: u64) {
        self.update(|state| {
            if state.0.revision == revision && state.1.is_some() {
                state.1 = None;
                state.0.mode = AccountMode::Expired;
                state.0.error = Some(super::session_error());
                state.0.revision += 1;
            }
        });
    }
    pub async fn saved(&self) -> Result<Option<Session>> {
        let vault = self.vault.clone();
        let value = tokio::task::spawn_blocking(move || vault.load())
            .await
            .map_err(|_| storage_error())?
            .map_err(|_| storage_error())?;
        value
            .map(|bytes| {
                if bytes.len() > 1_048_576 {
                    return Err(storage_error());
                }
                let mut stored: StoredSession =
                    serde_json::from_slice(&bytes).map_err(|_| storage_error())?;
                if stored.version != 1
                    || !valid_token(&stored.session.token)
                    || stored.session.user_id == 0
                {
                    return Err(storage_error());
                }
                stored.session.cookies = token_from_cookies(stored.session.cookies)
                    .filter(|(token, _)| token == &stored.session.token)
                    .ok_or_else(storage_error)?
                    .1;
                Ok(stored.session)
            })
            .transpose()
    }
    pub async fn activate(
        self: &Arc<Self>,
        id: u64,
        session: Session,
        remember: bool,
    ) -> Result<AccountSnapshot> {
        let auth = self.clone();
        // Own the whole vault transaction even if its caller/browser is cancelled.
        tokio::spawn(async move {
            let _mutation = auth.mutation.lock().await;
            if auth.attempt.load(Ordering::SeqCst) != id {
                return Err(AppError::cancelled());
            }
            let previous = if remember {
                let vault = auth.vault.clone();
                let bytes = serde_json::to_vec(&StoredSession {
                    version: 1,
                    session: session.clone(),
                })
                .map_err(|_| storage_error())?;
                if bytes.len() > 1_048_576 {
                    return Err(storage_error());
                }
                Some(
                    tokio::task::spawn_blocking(move || {
                        let old = vault.load().map_err(|_| storage_error())?;
                        vault.save(&bytes).map_err(|_| storage_error())?;
                        Ok::<_, AppError>(old)
                    })
                    .await
                    .map_err(|_| storage_error())??,
                )
            } else {
                None
            };
            let mut committed = false;
            auth.update(|state| {
                if auth.attempt.load(Ordering::SeqCst) != id {
                    return;
                }
                state.0.mode = AccountMode::Connected;
                state.0.profile = Some(session.profile.clone());
                state.0.error = None;
                state.0.revision += 1;
                state.1 = Some(Arc::new(session));
                committed = true;
            });
            if !committed {
                if let Some(previous) = previous {
                    let vault = auth.vault.clone();
                    tokio::task::spawn_blocking(move || match previous {
                        Some(bytes) => vault.save(&bytes),
                        None => vault.delete(),
                    })
                    .await
                    .map_err(|_| storage_error())?
                    .map_err(|_| storage_error())?;
                }
                return Err(AppError::cancelled());
            }
            Ok(auth.snapshot())
        })
        .await
        .map_err(|_| storage_error())?
    }
    pub async fn sign_out(self: &Arc<Self>) -> Result<AccountSnapshot> {
        self.attempt.fetch_add(1, Ordering::SeqCst);
        let auth = self.clone();
        tokio::spawn(async move {
            let _mutation = auth.mutation.lock().await;
            let vault = auth.vault.clone();
            tokio::task::spawn_blocking(move || vault.delete())
                .await
                .map_err(|_| storage_error())?
                .map_err(|_| storage_error())?;
            auth.update(|state| {
                state.1 = None;
                state.0.mode = AccountMode::Guest;
                state.0.profile = None;
                state.0.error = None;
                state.0.revision += 1;
            });
            Ok(auth.snapshot())
        })
        .await
        .map_err(|_| storage_error())?
    }
}

#[derive(Serialize, Deserialize)]
struct StoredSession {
    version: u8,
    session: Session,
}

pub(crate) fn valid_token(value: &str) -> bool {
    (16..=8192).contains(&value.len()) && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}
pub(crate) fn token_from_cookies(
    cookies: Vec<SessionCookie>,
) -> Option<(String, Vec<SessionCookie>)> {
    let cookies: Vec<_> = cookies
        .into_iter()
        .filter(|cookie| {
            let domain = cookie.domain.trim_start_matches('.');
            (domain == "soundcloud.com" || domain.ends_with(".soundcloud.com"))
                && cookie.secure
                && cookie.name.len() <= 128
                && cookie.value.len() <= 8192
                && !cookie.name.chars().any(char::is_control)
                && !cookie.value.chars().any(char::is_control)
        })
        .take(256)
        .collect();
    let token = cookies
        .iter()
        .find(|cookie| cookie.name == "oauth_token" && valid_token(&cookie.value))?
        .value
        .clone();
    Some((token, cookies))
}
pub(crate) fn storage_error() -> AppError {
    AppError::new(
        "soundcloud_storage",
        "The protected SoundCloud session could not be updated. Reconnect SoundCloud or forget its saved session.",
        true,
    )
}

#[cfg(target_os = "macos")]
pub(crate) struct SoundCloudVault;
#[cfg(target_os = "macos")]
impl SessionVault for SoundCloudVault {
    fn load(&self) -> Result<Option<Vec<u8>>> {
        match security_framework::passwords::get_generic_password(
            "app.vessel.desktop.soundcloud",
            "verified-session",
        ) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => Err(storage_error()),
        }
    }
    fn save(&self, bytes: &[u8]) -> Result<()> {
        security_framework::passwords::set_generic_password(
            "app.vessel.desktop.soundcloud",
            "verified-session",
            bytes,
        )
        .map_err(|_| storage_error())
    }
    fn delete(&self) -> Result<()> {
        match security_framework::passwords::delete_generic_password(
            "app.vessel.desktop.soundcloud",
            "verified-session",
        ) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => Err(storage_error()),
        }
    }
}
