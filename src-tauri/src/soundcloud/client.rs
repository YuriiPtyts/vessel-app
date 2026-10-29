//! Bounded compatibility client for SoundCloud's public website API. No app secret.
use super::{auth, unavailable};
use crate::error::{AppError, Result};
use reqwest::{Client, Method, Response};
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use url::Url;

const ORIGIN: &str = "https://soundcloud.com";
pub(crate) struct Api {
    http: Client,
    client_id: Mutex<Option<(String, Instant)>>,
    pub auth: Arc<auth::Auth>,
    #[cfg(test)]
    pub test_origin: Option<String>,
}
impl Api {
    pub fn new(auth: Arc<auth::Auth>) -> Result<Self> {
        Ok(Self { http: Client::builder().https_only(true).user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36").timeout(Duration::from_secs(20)).connect_timeout(Duration::from_secs(8)).build().map_err(|_| unavailable())?, client_id: Mutex::new(None), auth, #[cfg(test)] test_origin: None })
    }
    async fn browser_client(&self) -> Result<String> {
        let mut cached = self.client_id.lock().await;
        if let Some((value, created)) = cached
            .as_ref()
            .filter(|(_, created)| created.elapsed() < Duration::from_secs(900))
        {
            let _ = created;
            return Ok(value.clone());
        }
        #[cfg(test)]
        if self.test_origin.is_some() {
            return Ok("fixture-client".into());
        }
        let html = self
            .http
            .get(ORIGIN)
            .send()
            .await
            .map_err(|_| unavailable())?;
        let html = String::from_utf8(body(html, 4_194_304).await?).map_err(|_| unavailable())?;
        let scripts: Vec<_> = html
            .split("<script")
            .skip(1)
            .filter_map(|tag| {
                let tag = tag.split('>').next()?;
                let rest = tag.split("src=\"").nth(1)?;
                let url = Url::parse(rest.split('"').next()?).ok()?;
                (url.scheme() == "https"
                    && url.host_str() == Some("a-v2.sndcdn.com")
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.port().is_none())
                .then_some(url)
            })
            .collect();
        for script in scripts.into_iter().rev().take(12) {
            let Ok(response) = self.http.get(script).send().await else {
                continue;
            };
            let Ok(bytes) = body(response, 4_194_304).await else {
                continue;
            };
            let script = String::from_utf8_lossy(&bytes);
            if let Some(value) = parse_client_id(&script) {
                *cached = Some((value.clone(), Instant::now()));
                return Ok(value);
            }
        }
        Err(unavailable())
    }
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        payload: Option<Value>,
        revision: u64,
    ) -> Result<Value> {
        let url = Url::parse("https://api-v2.soundcloud.com/")
            .map_err(|_| unavailable())?
            .join(path)
            .map_err(|_| unavailable())?;
        self.request_url(method, url, query, payload, revision, None)
            .await
    }
    pub async fn request_url(
        &self,
        method: Method,
        mut url: Url,
        query: &[(&str, String)],
        payload: Option<Value>,
        revision: u64,
        candidate: Option<&str>,
    ) -> Result<Value> {
        if !valid_api_url(&url) {
            return Err(unavailable());
        }
        let (current, session) = self.auth.session();
        if current != revision {
            return Err(AppError::cancelled());
        }
        let client = tokio::time::timeout(Duration::from_secs(40), self.browser_client())
            .await
            .map_err(|_| unavailable())??;
        // Do not reuse credentials supplied inside provider continuation links.
        let retained: Vec<_> = url
            .query_pairs()
            .filter(|(key, _)| {
                !matches!(key.as_ref(), "client_id" | "oauth_token" | "access_token")
            })
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        url.set_query(None);
        url.query_pairs_mut()
            .extend_pairs(retained)
            .append_pair("client_id", &client)
            .extend_pairs(query.iter().map(|(key, value)| (*key, value.as_str())));
        #[cfg(test)]
        if let Some(origin) = &self.test_origin {
            let mut fixture = Url::parse(origin).unwrap();
            fixture.set_path(url.path());
            fixture.set_query(url.query());
            url = fixture;
        }
        let mut request = self
            .http
            .request(method, url)
            .header("Origin", ORIGIN)
            .header("Referer", format!("{ORIGIN}/"));
        if let Some(token) =
            candidate.or_else(|| session.as_ref().map(|session| session.token.as_str()))
        {
            if !auth::valid_token(token) {
                return Err(super::session_error());
            }
            let mut value = reqwest::header::HeaderValue::from_str(&format!("OAuth {token}"))
                .map_err(|_| super::session_error())?;
            value.set_sensitive(true);
            request = request.header(reqwest::header::AUTHORIZATION, value);
        }
        if let Some(payload) = payload {
            request = request.json(&payload);
        }
        let response = request.send().await.map_err(|_| unavailable())?;
        if self.auth.snapshot().revision != revision {
            return Err(AppError::cancelled());
        }
        if response.status().as_u16() == 401 && session.is_some() && candidate.is_none() {
            self.auth.expire(revision);
            return Err(super::session_error());
        }
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(serde_json::json!({"acknowledged":true}));
        }
        let bytes = body(response, 8_388_608).await?;
        let value: Value = if bytes.is_empty() {
            serde_json::json!({"acknowledged":true})
        } else {
            serde_json::from_slice(&bytes).map_err(|_| unavailable())?
        };
        if value.get("error").is_some_and(|error| !error.is_null())
            || value
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
        {
            return Err(unavailable());
        }
        if self.auth.snapshot().revision != revision {
            return Err(AppError::cancelled());
        }
        Ok(value)
    }
    pub async fn candidate_profile(
        &self,
        token: &str,
    ) -> Result<(u64, crate::account::AccountProfile)> {
        let revision = self.auth.snapshot().revision;
        let data = self
            .request_url(
                Method::GET,
                Url::parse("https://api-v2.soundcloud.com/me").unwrap(),
                &[],
                None,
                revision,
                Some(token),
            )
            .await?;
        let id = data
            .get("id")
            .and_then(Value::as_u64)
            .filter(|id| *id > 0)
            .ok_or_else(super::session_error)?;
        let name = data
            .get("username")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(super::session_error)?;
        Ok((
            id,
            crate::account::AccountProfile {
                name: name.chars().take(512).collect(),
                email: None,
                picture_url: data
                    .get("avatar_url")
                    .and_then(Value::as_str)
                    .and_then(crate::provider::artwork_url),
            },
        ))
    }
    #[cfg(test)]
    pub fn fixture(auth: Arc<auth::Auth>, origin: String) -> Self {
        let mut api = Self::new(auth).unwrap();
        api.http = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        api.test_origin = Some(origin);
        api
    }
}
pub(crate) fn valid_api_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("api-v2.soundcloud.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
}
fn parse_client_id(script: &str) -> Option<String> {
    for rest in script.split("client_id").skip(1) {
        let rest = rest.trim_start();
        let rest = rest.strip_prefix('"').unwrap_or(rest);
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let rest = rest.trim_start();
        if let Some(rest) = rest.strip_prefix('"') {
            let value = rest.split('"').next()?;
            if value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
                return Some(value.into());
            }
        }
    }
    None
}
async fn body(mut response: Response, limit: usize) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        // Status is useful for account-library failures; response bodies,
        // headers and private request/continuation URLs are not UI errors.
        return Err(AppError::new(
            "soundcloud_http",
            format!(
                "SoundCloud returned HTTP {}. Refresh to try again; reconnect SoundCloud if account access keeps failing.",
                response.status().as_u16()
            ),
            true,
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > limit {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_client_discovery_skips_references_and_accepts_only_bounded_identifiers() {
        let id = "abcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(
            parse_client_id(&format!(
                "client_id=function(){{}}; a={{client_id:\"{id}\"}}"
            )),
            Some(id.into())
        );
        assert_eq!(
            parse_client_id(&format!("{{\"client_id\":\"{id}\"}}")),
            Some(id.into())
        );
        assert!(parse_client_id("client_id:\"too-short\"").is_none());
        for value in [
            "https://evil.test/tracks/1",
            "https://user@api-v2.soundcloud.com/tracks/1",
            "http://api-v2.soundcloud.com/me",
            "https://api-v2.soundcloud.com:8443/me",
        ] {
            assert!(!valid_api_url(&value.parse().unwrap()));
        }
    }
}
