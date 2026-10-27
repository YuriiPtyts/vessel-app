//! Google website sessions remain in Rust; only a verified public profile crosses IPC.
use crate::{
    error::{AppError, Result},
    provider::artwork_url,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex as AsyncMutex, watch};

#[cfg(target_os = "macos")]
const SERVICE: &str = "app.vessel.desktop.google-session";
#[cfg(target_os = "macos")]
const ENTRY: &str = "youtube-music";
#[cfg(target_os = "windows")]
mod windows_vault;
#[cfg(target_os = "windows")]
pub(crate) use windows_vault::WindowsVault;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub name: String,
    pub email: Option<String>,
    pub picture_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountMode {
    Guest,
    Checking,
    SigningIn,
    Connected,
    Expired,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSnapshot {
    pub mode: AccountMode,
    pub profile: Option<AccountProfile>,
    pub error: Option<AppError>,
    pub revision: u64,
}

// Serializable exclusively for the OS credential vault; deliberately no Debug implementation.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SessionCookie {
    pub domain: String,
    pub path: String,
    pub name: String,
    pub value: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires_at: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CookieSession {
    pub cookies: Vec<SessionCookie>,
    #[serde(default)]
    google_cookies: Vec<SessionCookie>,
}

#[derive(Serialize, Deserialize)]
struct StoredSession {
    version: u8,
    session: CookieSession,
    profile: AccountProfile,
}

impl CookieSession {
    pub fn from_cookies(cookies: Vec<SessionCookie>) -> Result<Self> {
        let (cookies, google_cookies) = Self::retained_browser_cookies(cookies)?
            .into_iter()
            .partition(|cookie| {
                let domain = cookie.domain.trim_start_matches('.');
                domain == "youtube.com" || domain.ends_with(".youtube.com")
            });
        let result = Self {
            cookies,
            google_cookies,
        };
        if result
            .cookie_value("SAPISID")
            .or_else(|| result.cookie_value("__Secure-3PAPISID"))
            .is_none()
            || result.cookie_value("LOGIN_INFO").is_none()
        {
            return Err(session_error());
        }
        Ok(result)
    }

    pub fn browser_cookies(&self) -> Vec<SessionCookie> {
        self.cookies
            .iter()
            .chain(&self.google_cookies)
            .cloned()
            .collect()
    }

    pub fn retained_browser_cookies(cookies: Vec<SessionCookie>) -> Result<Vec<SessionCookie>> {
        let now = epoch()? as i64;
        let cookies: Vec<_> = cookies
            .into_iter()
            .filter(|cookie| {
                let domain = cookie.domain.trim_start_matches('.');
                (domain == "youtube.com"
                    || domain.ends_with(".youtube.com")
                    || domain == "google.com"
                    || domain.ends_with(".google.com"))
                    && domain.len() <= 253
                    && domain
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
                    && !cookie.name.is_empty()
                    && cookie.name.len() <= 128
                    && cookie.value.len() <= 8192
                    && cookie
                        .name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_-.".contains(&byte))
                    && !cookie.value.contains(['\r', '\n', '\0', ';', '\t'])
                    && cookie.path.starts_with('/')
                    && cookie.path.len() <= 2048
                    && !cookie.path.contains(['\r', '\n', '\t', '\0'])
                    && cookie.expires_at.is_none_or(|expiry| expiry > now)
            })
            .take(100)
            .collect();
        if serde_json::to_vec(&cookies)
            .map_err(|_| session_error())?
            .len()
            > 64 * 1024
        {
            return Err(session_error());
        }
        Ok(cookies)
    }

    fn cookie_value(&self, name: &str) -> Option<&str> {
        self.cookies
            .iter()
            .filter(|cookie| {
                cookie.name == name && cookie.path == "/" && music_domain(&cookie.domain)
            })
            .max_by_key(|cookie| cookie.domain.len())
            .map(|cookie| cookie.value.as_str())
    }

    pub fn request_headers(
        &self,
        mut request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder> {
        let mut cookies: BTreeMap<&str, &SessionCookie> = BTreeMap::new();
        for cookie in &self.cookies {
            if music_domain(&cookie.domain) && cookie.path == "/" {
                let entry = cookies.entry(&cookie.name).or_insert(cookie);
                if cookie.domain.len() > entry.domain.len() {
                    *entry = cookie;
                }
            }
        }
        let cookie = cookies
            .into_iter()
            .map(|(name, cookie)| format!("{name}={}", cookie.value))
            .collect::<Vec<_>>()
            .join("; ");
        let authorization = self.authorization(epoch()?);
        for (name, value) in [
            (reqwest::header::COOKIE, cookie),
            (reqwest::header::AUTHORIZATION, authorization),
        ] {
            let mut value =
                reqwest::header::HeaderValue::from_str(&value).map_err(|_| session_error())?;
            value.set_sensitive(true);
            request = request.header(name, value);
        }
        Ok(request
            .header("X-Goog-AuthUser", "0")
            .header("X-Origin", "https://music.youtube.com"))
    }

    fn authorization(&self, timestamp: u64) -> String {
        let primary = self
            .cookie_value("SAPISID")
            .or_else(|| self.cookie_value("__Secure-3PAPISID"));
        [
            ("SAPISIDHASH", primary),
            ("SAPISID1PHASH", self.cookie_value("__Secure-1PAPISID")),
            ("SAPISID3PHASH", self.cookie_value("__Secure-3PAPISID")),
        ]
        .into_iter()
        .filter_map(|(scheme, value)| {
            value.map(|sid| {
                let hash =
                    Sha1::digest(format!("{timestamp} {sid} https://music.youtube.com").as_bytes());
                format!("{scheme} {timestamp}_{hash:x}")
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
    }

    pub fn cookie_file(&self) -> Result<tempfile::NamedTempFile> {
        use std::io::Write;
        let mut file = tempfile::Builder::new()
            .prefix(".vessel-session-")
            .tempfile()
            .map_err(|_| vault_error())?;
        let mut text = String::from("# Netscape HTTP Cookie File\n");
        for cookie in &self.cookies {
            text.push_str(&format!(
                "{}{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                if cookie.http_only { "#HttpOnly_" } else { "" },
                cookie.domain,
                if cookie.domain.starts_with('.') {
                    "TRUE"
                } else {
                    "FALSE"
                },
                cookie.path,
                if cookie.secure { "TRUE" } else { "FALSE" },
                cookie
                    .expires_at
                    .map(|time| time.to_string())
                    .unwrap_or_default(),
                cookie.name,
                cookie.value
            ));
        }
        file.write_all(text.as_bytes()).map_err(|_| vault_error())?;
        Ok(file)
    }
}

pub(crate) trait SessionVault: Send + Sync {
    fn load(&self) -> Result<Option<Vec<u8>>>;
    fn save(&self, value: &[u8]) -> Result<()>;
    fn delete(&self) -> Result<()>;
}
#[derive(Default)]
pub(crate) struct MemoryVault(Mutex<Option<Vec<u8>>>);
impl SessionVault for MemoryVault {
    fn load(&self) -> Result<Option<Vec<u8>>> {
        self.0
            .lock()
            .map(|value| value.clone())
            .map_err(|_| vault_error())
    }
    fn save(&self, value: &[u8]) -> Result<()> {
        *self.0.lock().map_err(|_| vault_error())? = Some(value.into());
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        *self.0.lock().map_err(|_| vault_error())? = None;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct KeychainVault;
#[cfg(target_os = "macos")]
impl SessionVault for KeychainVault {
    fn load(&self) -> Result<Option<Vec<u8>>> {
        match security_framework::passwords::get_generic_password(SERVICE, ENTRY) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => Err(vault_error()),
        }
    }
    fn save(&self, value: &[u8]) -> Result<()> {
        security_framework::passwords::set_generic_password(SERVICE, ENTRY, value)
            .map_err(|_| vault_error())
    }
    fn delete(&self) -> Result<()> {
        match security_framework::passwords::delete_generic_password(SERVICE, ENTRY) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => Err(vault_error()),
        }
    }
}

pub(crate) struct Account {
    state: Mutex<(AccountSnapshot, Option<Arc<CookieSession>>)>,
    changes: watch::Sender<AccountSnapshot>,
    vault: Arc<dyn SessionVault>,
    mutation: AsyncMutex<()>,
    attempt: AtomicU64,
}

impl Account {
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
            vault,
            mutation: AsyncMutex::new(()),
            attempt: AtomicU64::new(0),
        })
    }
    pub fn snapshot(&self) -> AccountSnapshot {
        self.changes.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<AccountSnapshot> {
        self.changes.subscribe()
    }
    pub fn request_session(&self) -> (u64, Option<Arc<CookieSession>>) {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        (state.0.revision, state.1.clone())
    }
    pub fn begin(&self, restoring: bool) -> u64 {
        self.update(|state| {
            let id = self.attempt.fetch_add(1, Ordering::SeqCst) + 1;
            state.0.mode = if restoring {
                AccountMode::Checking
            } else {
                AccountMode::SigningIn
            };
            state.0.error = None;
            id
        })
    }
    pub fn begin_restore(&self) -> Option<u64> {
        self.update(|state| {
            self.attempt
                .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                .ok()?;
            state.0.mode = AccountMode::Checking;
            state.0.error = None;
            Some(1)
        })
    }
    pub fn remember_profile(&self, id: u64, profile: AccountProfile) {
        self.update(|state| {
            if self.attempt.load(Ordering::SeqCst) == id && state.0.mode == AccountMode::Checking {
                state.0.profile = Some(profile);
            }
        });
    }
    pub fn cancel(&self, id: u64) {
        self.update(|state| {
            if self
                .attempt
                .compare_exchange(id, id + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return;
            }
            if matches!(state.0.mode, AccountMode::SigningIn | AccountMode::Checking) {
                state.0.mode = if state.1.is_some() {
                    AccountMode::Connected
                } else {
                    AccountMode::Guest
                };
                state.0.error = None;
            }
        });
    }
    pub fn cancel_current(&self) {
        self.cancel(self.attempt.load(Ordering::SeqCst));
    }
    pub fn fail(&self, id: u64, error: AppError) {
        self.update(|state| {
            if self.attempt.load(Ordering::SeqCst) != id {
                return;
            }
            state.0.mode = if state.1.is_some() {
                AccountMode::Connected
            } else if error.code == "session_expired" {
                AccountMode::Expired
            } else {
                AccountMode::Unavailable
            };
            state.0.error = Some(error);
        });
    }
    pub async fn saved(&self) -> Result<Option<(CookieSession, AccountProfile)>> {
        let vault = self.vault.clone();
        let data = tokio::task::spawn_blocking(move || vault.load())
            .await
            .map_err(|_| vault_error())??;
        let Some(data) = data else { return Ok(None) };
        let saved: StoredSession = serde_json::from_slice(&data).map_err(|_| session_error())?;
        if saved.version != 1 {
            return Err(session_error());
        }
        Ok(Some((
            CookieSession::from_cookies(saved.session.browser_cookies())?,
            saved.profile,
        )))
    }
    pub async fn saved_browser_cookies(&self) -> Result<Vec<SessionCookie>> {
        let _guard = self.mutation.lock().await;
        let vault = self.vault.clone();
        let data = tokio::task::spawn_blocking(move || vault.load())
            .await
            .map_err(|_| vault_error())??;
        let Some(data) = data else {
            return Ok(Vec::new());
        };
        let stored: StoredSession = serde_json::from_slice(&data).map_err(|_| session_error())?;
        if stored.version != 1 {
            return Err(session_error());
        }
        CookieSession::retained_browser_cookies(stored.session.browser_cookies())
    }
    pub async fn activate(
        self: &Arc<Self>,
        id: u64,
        session: CookieSession,
        profile: AccountProfile,
        persist: bool,
    ) -> Result<AccountSnapshot> {
        let account = self.clone();
        // A cancelled command must not abandon an in-flight OS vault write or rollback.
        tokio::spawn(async move { account.activate_inner(id, session, profile, persist).await })
            .await
            .map_err(|_| vault_error())?
    }

    async fn activate_inner(
        &self,
        id: u64,
        session: CookieSession,
        profile: AccountProfile,
        persist: bool,
    ) -> Result<AccountSnapshot> {
        let _guard = self.mutation.lock().await;
        if self.attempt.load(Ordering::SeqCst) != id {
            return Err(AppError::cancelled());
        }
        let previous = if persist {
            let previous = self.vault.clone();
            let value = serde_json::to_vec(&StoredSession {
                version: 1,
                session: session.clone(),
                profile: profile.clone(),
            })
            .map_err(|_| vault_error())?;
            let old = tokio::task::spawn_blocking(move || previous.load())
                .await
                .map_err(|_| vault_error())??;
            let vault = self.vault.clone();
            tokio::task::spawn_blocking(move || vault.save(&value))
                .await
                .map_err(|_| vault_error())??;
            Some(old)
        } else {
            None
        };
        let activated = self.update(|state| {
            if self.attempt.load(Ordering::SeqCst) != id {
                return false;
            }
            state.0.mode = AccountMode::Connected;
            state.0.profile = Some(profile);
            state.0.error = None;
            state.0.revision += 1;
            state.1 = Some(Arc::new(session));
            true
        });
        if !activated {
            if let Some(previous) = previous {
                let vault = self.vault.clone();
                tokio::task::spawn_blocking(move || match previous {
                    Some(value) => vault.save(&value),
                    None => vault.delete(),
                })
                .await
                .map_err(|_| vault_error())??;
            }
            return Err(AppError::cancelled());
        }
        Ok(self.snapshot())
    }
    pub async fn sign_out(self: &Arc<Self>) -> Result<AccountSnapshot> {
        let id = self.update(|_| self.attempt.fetch_add(1, Ordering::SeqCst) + 1);
        let account = self.clone();
        tokio::spawn(async move { account.sign_out_inner(id).await })
            .await
            .map_err(|_| vault_error())?
    }
    async fn sign_out_inner(&self, id: u64) -> Result<AccountSnapshot> {
        let _guard = self.mutation.lock().await;
        let vault = self.vault.clone();
        tokio::task::spawn_blocking(move || vault.delete())
            .await
            .map_err(|_| vault_error())??;
        self.update(|state| {
            if self.attempt.load(Ordering::SeqCst) == id || state.0.mode != AccountMode::SigningIn {
                state.0.mode = AccountMode::Guest;
            }
            state.0.profile = None;
            state.0.error = None;
            state.0.revision += 1;
            state.1 = None;
        });
        Ok(self.snapshot())
    }
    pub fn expire(&self, revision: u64) {
        self.update(|state| {
            if state.0.revision == revision && state.1.is_some() {
                state.1 = None;
                state.0.mode = AccountMode::Expired;
                state.0.error = Some(session_error());
                state.0.revision += 1;
            }
        });
    }
    fn update<T>(
        &self,
        change: impl FnOnce(&mut (AccountSnapshot, Option<Arc<CookieSession>>)) -> T,
    ) -> T {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let result = change(&mut state);
        self.changes.send_replace(state.0.clone());
        result
    }
}

pub(crate) fn parse_profile(data: &serde_json::Value) -> Result<AccountProfile> {
    let mut profile = None;
    crate::catalog::walk(data, &mut |value| {
        if let Some(header) = value.get("activeAccountHeaderRenderer") {
            let text = |node: Option<&serde_json::Value>| -> String {
                node.and_then(|node| {
                    node.get("simpleText")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            node.get("runs")
                                .and_then(serde_json::Value::as_array)
                                .map(|runs| {
                                    runs.iter()
                                        .filter_map(|run| run.get("text")?.as_str())
                                        .collect()
                                })
                        })
                })
                .unwrap_or_default()
            };
            let name = text(header.get("accountName"));
            let email = text(header.get("email"));
            let picture = header
                .pointer("/accountPhoto/thumbnails")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| items.last())
                .and_then(|item| item.get("url"))
                .and_then(serde_json::Value::as_str)
                .and_then(artwork_url);
            if !name.is_empty() {
                profile = Some(AccountProfile {
                    name,
                    email: (!email.is_empty()).then_some(email),
                    picture_url: picture,
                });
            }
        }
    });
    profile.ok_or_else(session_error)
}

fn music_domain(domain: &str) -> bool {
    let domain = domain.trim_start_matches('.');
    domain == "music.youtube.com" || domain == "youtube.com"
}

fn epoch() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| session_error())
}
pub(crate) fn session_error() -> AppError {
    AppError::new(
        "session_expired",
        "YouTube could not verify this account session. Reconnect with Google; guest listening remains available.",
        false,
    )
}
fn vault_error() -> AppError {
    #[cfg(target_os = "windows")]
    let message = "Windows could not read or save the protected YouTube session. Try reconnecting.";
    #[cfg(target_os = "windows")]
    let code = "credential_store_unavailable";
    #[cfg(not(target_os = "windows"))]
    let code = "keychain_unavailable";
    #[cfg(not(target_os = "windows"))]
    let message =
        "macOS Keychain could not read or save the YouTube session. Unlock Keychain and try again.";
    AppError::new(code, message, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Condvar, atomic::AtomicBool};

    struct PausedVault {
        memory: MemoryVault,
        pause_once: AtomicBool,
        entered: tokio::sync::Notify,
        released: (Mutex<bool>, Condvar),
    }
    impl SessionVault for PausedVault {
        fn load(&self) -> Result<Option<Vec<u8>>> {
            self.memory.load()
        }
        fn save(&self, value: &[u8]) -> Result<()> {
            if self.pause_once.swap(false, Ordering::SeqCst) {
                self.entered.notify_one();
                let (lock, condition) = &self.released;
                drop(
                    condition
                        .wait_while(lock.lock().unwrap(), |released| !*released)
                        .unwrap(),
                );
            }
            self.memory.save(value)
        }
        fn delete(&self) -> Result<()> {
            self.memory.delete()
        }
    }
    fn session() -> CookieSession {
        CookieSession::from_cookies(
            [
                ("SAPISID", "fixture-session"),
                ("LOGIN_INFO", "fixture-login"),
            ]
            .into_iter()
            .map(|(name, value)| SessionCookie {
                domain: ".youtube.com".into(),
                path: "/".into(),
                name: name.into(),
                value: value.into(),
                secure: true,
                http_only: true,
                expires_at: None,
            })
            .collect(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn a_verified_session_survives_restart_until_explicit_sign_out() {
        let vault = Arc::new(MemoryVault::default());
        let original = Account::new(vault.clone());
        let mut remembered = session();
        remembered.google_cookies.push(SessionCookie {
            domain: ".google.com".into(),
            path: "/".into(),
            name: "SID".into(),
            value: "browser-only-fixture".into(),
            secure: true,
            http_only: true,
            expires_at: None,
        });
        let profile = AccountProfile {
            name: "Remembered fixture".into(),
            email: None,
            picture_url: None,
        };
        let id = original.begin(false);
        original
            .activate(id, remembered, profile.clone(), true)
            .await
            .unwrap();
        drop(original);

        let restarted = Account::new(vault.clone());
        let id = restarted.begin_restore().unwrap();
        let (session, saved_profile) = restarted.saved().await.unwrap().unwrap();
        restarted.remember_profile(id, saved_profile.clone());
        assert!(
            session
                .browser_cookies()
                .iter()
                .any(|cookie| cookie.name == "SID")
        );
        assert!(
            !std::fs::read_to_string(session.cookie_file().unwrap().path())
                .unwrap()
                .contains("browser-only-fixture")
        );
        let request = session
            .request_headers(reqwest::Client::new().get("https://music.youtube.com/"))
            .unwrap()
            .build()
            .unwrap();
        assert!(
            !request.headers()[reqwest::header::COOKIE]
                .to_str()
                .unwrap()
                .contains("browser-only-fixture")
        );
        assert_eq!(saved_profile, profile);
        assert_eq!(restarted.snapshot().profile.as_ref(), Some(&profile));
        // A saved profile is not sufficient to announce a verified connection.
        assert_eq!(restarted.snapshot().mode, AccountMode::Checking);
        assert!(restarted.request_session().1.is_none());
        restarted
            .activate(id, session, profile, false)
            .await
            .unwrap();
        assert_eq!(restarted.snapshot().mode, AccountMode::Connected);
        restarted.sign_out().await.unwrap();
        drop(restarted);
        assert!(Account::new(vault).saved().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn browser_reconnect_retains_google_cookies_when_youtube_session_expires() {
        let vault = Arc::new(MemoryVault::default());
        let mut stored = session();
        for cookie in &mut stored.cookies {
            cookie.expires_at = Some(0);
        }
        stored.google_cookies.push(SessionCookie {
            domain: ".google.com".into(),
            path: "/".into(),
            name: "SID".into(),
            value: "browser-fixture".into(),
            secure: true,
            http_only: true,
            expires_at: None,
        });
        vault
            .save(
                &serde_json::to_vec(&StoredSession {
                    version: 1,
                    session: stored,
                    profile: AccountProfile {
                        name: "Fixture".into(),
                        email: None,
                        picture_url: None,
                    },
                })
                .unwrap(),
            )
            .unwrap();
        let restarted = Account::new(vault);
        assert!(matches!(restarted.saved().await, Err(error) if error.code == "session_expired"));
        let cookies = restarted.saved_browser_cookies().await.unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "SID");
        assert_eq!(restarted.snapshot().mode, AccountMode::Guest);
    }

    #[test]
    fn previous_vault_records_without_browser_cookies_still_restore() {
        let mut value = serde_json::to_value(StoredSession {
            version: 1,
            session: session(),
            profile: AccountProfile {
                name: "Legacy".into(),
                email: None,
                picture_url: None,
            },
        })
        .unwrap();
        value["session"]
            .as_object_mut()
            .unwrap()
            .remove("google_cookies");
        let stored: StoredSession = serde_json::from_value(value).unwrap();
        assert_eq!(
            CookieSession::from_cookies(stored.session.browser_cookies())
                .unwrap()
                .cookies
                .len(),
            2
        );
    }

    #[test]
    fn remembered_browser_cookies_stay_inside_google_and_youtube_scopes() {
        let mut cookies = session().browser_cookies();
        for domain in [".google.com", "google.com.attacker.test", "unrelated.test"] {
            cookies.push(SessionCookie {
                domain: domain.into(),
                path: "/".into(),
                name: "SID".into(),
                value: "fixture".into(),
                secure: true,
                http_only: true,
                expires_at: None,
            });
        }
        let retained = CookieSession::from_cookies(cookies).unwrap();
        assert_eq!(retained.cookies.len(), 2);
        assert_eq!(retained.browser_cookies().len(), 3);
        assert_eq!(retained.google_cookies[0].domain, ".google.com");
    }
    #[tokio::test]
    async fn public_state_never_contains_the_session_and_sign_out_removes_the_vault() {
        let vault = Arc::new(MemoryVault::default());
        let account = Account::new(vault.clone());
        let attempt = account.begin(false);
        account
            .activate(
                attempt,
                session(),
                AccountProfile {
                    name: "Fixture".into(),
                    email: None,
                    picture_url: None,
                },
                true,
            )
            .await
            .unwrap();
        assert_eq!(account.snapshot().mode, AccountMode::Connected);
        assert!(
            !serde_json::to_string(&account.snapshot())
                .unwrap()
                .contains("fixture-session")
        );
        account.sign_out().await.unwrap();
        assert!(vault.load().unwrap().is_none());
        assert!(account.request_session().1.is_none());
    }

    #[tokio::test]
    async fn cancelling_the_caller_during_an_os_write_cannot_leave_a_session_in_the_vault() {
        let vault = Arc::new(PausedVault {
            memory: MemoryVault::default(),
            pause_once: AtomicBool::new(true),
            entered: tokio::sync::Notify::new(),
            released: (Mutex::new(false), Condvar::new()),
        });
        let account = Account::new(vault.clone());
        let id = account.begin(false);
        let request = account.clone();
        let caller = tokio::spawn(async move {
            request
                .activate(
                    id,
                    session(),
                    AccountProfile {
                        name: "Fixture".into(),
                        email: None,
                        picture_url: None,
                    },
                    true,
                )
                .await
        });
        vault.entered.notified().await;
        account.cancel(id);
        caller.abort();
        *vault.released.0.lock().unwrap() = true;
        vault.released.1.notify_one();
        // The mutation guard stays owned until the detached write and rollback finish.
        let _finished = account.mutation.lock().await;
        assert!(vault.load().unwrap().is_none());
        assert_eq!(account.snapshot().mode, AccountMode::Guest);
        assert!(account.request_session().1.is_none());
    }

    #[tokio::test]
    async fn an_old_restore_or_expiry_cannot_replace_a_new_connection() {
        let account = Account::new(Arc::new(MemoryVault::default()));
        let restored = account.begin_restore().unwrap();
        let current = account.begin(false);
        assert!(account.begin_restore().is_none());
        assert!(
            account
                .activate(
                    restored,
                    session(),
                    AccountProfile {
                        name: "Old".into(),
                        email: None,
                        picture_url: None
                    },
                    false
                )
                .await
                .is_err()
        );
        account
            .activate(
                current,
                session(),
                AccountProfile {
                    name: "New".into(),
                    email: None,
                    picture_url: None,
                },
                true,
            )
            .await
            .unwrap();
        let revision = account.snapshot().revision;
        account.expire(revision.wrapping_sub(1));
        assert_eq!(account.snapshot().mode, AccountMode::Connected);
        account.expire(revision);
        assert_eq!(account.snapshot().mode, AccountMode::Expired);
        assert!(account.request_session().1.is_none());
        account.sign_out().await.unwrap();
        assert!(account.saved().await.unwrap().is_none());
    }
    #[test]
    fn a_sign_in_page_is_not_a_verified_account() {
        assert!(parse_profile(&serde_json::json!({"actions": [{"text":"Sign in"}]})).is_err());
    }
    #[test]
    fn cookie_export_is_private_and_explicitly_scoped_to_youtube() {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        let file = session().cookie_file().unwrap();
        #[cfg(unix)]
        assert_eq!(
            file.as_file().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(CookieSession::from_cookies(vec![]).is_err());
        assert!(file.path().exists());
    }
}
