//! Guest/account YouTube Music catalog. Volatile response parsing stays out of playback.
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, watch};

use crate::{
    account::{Account, AccountProfile, CookieSession, MemoryVault, parse_profile, session_error},
    error::{AppError, Result},
    home::{self, CollectionPage, CollectionTarget, HomeFeed},
    provider::{Track, artwork_url},
};

const ORIGIN: &str = "https://music.youtube.com";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchKind {
    Songs,
    Videos,
}

impl SearchKind {
    fn params(self) -> &'static str {
        match self {
            Self::Songs => "EgWKAQIIAWoMEA4QChADEAQQCRAF",
            Self::Videos => "EgWKAQIQAWoMEA4QChADEAQQCRAF",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub query: String,
    pub kind: SearchKind,
    pub tracks: Vec<Track>,
    pub has_more: bool,
}

struct Session {
    context: Value,
    created: Instant,
    revision: u64,
}
struct CachedSearch {
    results: SearchResults,
    continuation: Option<String>,
    created: Instant,
    revision: u64,
}
struct CachedHome {
    feed: HomeFeed,
    targets: std::collections::BTreeMap<String, CollectionTarget>,
    created: Instant,
    revision: u64,
}
struct CachedCollection {
    page: CollectionPage,
    continuation: Option<String>,
    revision: u64,
}

pub struct MusicCatalog {
    client: Client,
    origin: String,
    session: Mutex<Option<Session>>,
    cache: Mutex<Option<CachedSearch>>,
    known_tracks: Mutex<VecDeque<Track>>,
    slot: Semaphore,
    cancellation: watch::Sender<u64>,
    home: Mutex<Option<CachedHome>>,
    collection: Mutex<Option<CachedCollection>>,
    account: Arc<Account>,
    radio_slot: Semaphore,
}

impl MusicCatalog {
    pub fn new() -> Result<Self> {
        Self::with_account(Account::new(Arc::new(MemoryVault::default())))
    }

    pub(crate) fn with_account(account: Arc<Account>) -> Result<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .https_only(true)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| catalog_error())?;
        let (cancellation, _) = watch::channel(0);
        Ok(Self {
            client,
            origin: ORIGIN.into(),
            session: Mutex::new(None),
            cache: Mutex::new(None),
            known_tracks: Mutex::new(VecDeque::new()),
            slot: Semaphore::new(1),
            cancellation,
            home: Mutex::new(None),
            collection: Mutex::new(None),
            account,
            radio_slot: Semaphore::new(1),
        })
    }

    pub fn cancel(&self) {
        self.cancellation
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub(crate) fn invalidate(&self) {
        self.cancel();
        if let Ok(mut value) = self.session.lock() {
            *value = None;
        }
        if let Ok(mut value) = self.cache.lock() {
            *value = None;
        }
        if let Ok(mut value) = self.home.lock() {
            *value = None;
        }
        if let Ok(mut value) = self.collection.lock() {
            *value = None;
        }
        if let Ok(mut value) = self.known_tracks.lock() {
            value.clear();
        }
    }

    pub fn find_track(&self, id: &str) -> Option<Track> {
        self.known_tracks
            .lock()
            .ok()?
            .iter()
            .find(|track| track.id == id)
            .cloned()
    }

    pub async fn search(&self, query: &str, kind: SearchKind, more: bool) -> Result<SearchResults> {
        let query = query.trim();
        if !(2..=160).contains(&query.chars().count()) || query.chars().any(char::is_control) {
            return Err(AppError::new(
                "invalid_query",
                "Search with 2–160 characters, such as a song, artist, or album name.",
                false,
            ));
        }
        let mut cancel = self.cancellation.subscribe();
        self.cancel();
        cancel.borrow_and_update();
        tokio::select! {
            result = self.search_inner(query, kind, more) => result,
            _ = cancel.changed() => Err(AppError::cancelled()),
        }
    }

    pub async fn home(&self, refresh: bool) -> Result<HomeFeed> {
        let mut cancel = self.cancellation.subscribe();
        self.cancel();
        cancel.borrow_and_update();
        tokio::select! { result = self.home_inner(refresh) => result, _ = cancel.changed() => Err(AppError::cancelled()) }
    }

    async fn home_inner(&self, refresh: bool) -> Result<HomeFeed> {
        let _slot = self
            .slot
            .acquire()
            .await
            .map_err(|_| AppError::internal())?;
        if !refresh
            && let Some(cached) = self
                .home
                .lock()
                .map_err(|_| AppError::internal())?
                .as_ref()
                .filter(|cache| {
                    cache.created.elapsed() < Duration::from_secs(300)
                        && cache.revision == self.account.snapshot().revision
                })
        {
            return Ok(cached.feed.clone());
        }
        let (context, revision) = self.context().await?;
        let data = self
            .browse_request(
                json!({"context": context, "browseId": "FEmusic_home"}),
                None,
                revision,
            )
            .await?;
        let (mut feed, targets) = home::parse_home(&data)?;
        feed.personalized = self.account.request_session().1.is_some();
        let tracks: Vec<_> = feed
            .shelves
            .iter()
            .flat_map(|shelf| &shelf.items)
            .filter_map(|item| item.track.clone())
            .collect();
        self.remember(&tracks)?;
        *self.home.lock().map_err(|_| AppError::internal())? = Some(CachedHome {
            feed: feed.clone(),
            targets,
            created: Instant::now(),
            revision,
        });
        Ok(feed)
    }

    pub async fn collection(&self, id: &str, more: bool) -> Result<CollectionPage> {
        let (target, revision) = self
            .home
            .lock()
            .map_err(|_| AppError::internal())?
            .as_ref()
            .filter(|cache| cache.revision == self.account.snapshot().revision)
            .and_then(|cache| {
                cache
                    .targets
                    .get(id)
                    .cloned()
                    .map(|target| (target, cache.revision))
            })
            .ok_or_else(|| {
                AppError::new(
                    "stale_collection",
                    "Refresh Listen now to open this collection again.",
                    true,
                )
            })?;
        self.collection_target(target, more, revision).await
    }

    pub(crate) async fn collection_target(
        &self,
        target: CollectionTarget,
        more: bool,
        revision: u64,
    ) -> Result<CollectionPage> {
        let mut cancel = self.cancellation.subscribe();
        self.cancel();
        cancel.borrow_and_update();
        tokio::select! { result = self.collection_inner(target, more, revision) => result, _ = cancel.changed() => Err(AppError::cancelled()) }
    }

    async fn collection_inner(
        &self,
        target: CollectionTarget,
        more: bool,
        expected_revision: u64,
    ) -> Result<CollectionPage> {
        let _slot = self
            .slot
            .acquire()
            .await
            .map_err(|_| AppError::internal())?;
        let (previous, continuation) = if more {
            let cache = self.collection.lock().map_err(|_| AppError::internal())?;
            let cache = cache
                .as_ref()
                .filter(|cache| {
                    cache.page.item.id == target.item.id
                        && cache.revision == self.account.snapshot().revision
                })
                .ok_or_else(|| {
                    AppError::new(
                        "stale_collection",
                        "Open this collection again to load more songs.",
                        true,
                    )
                })?;
            if cache.continuation.is_none() {
                return Ok(cache.page.clone());
            }
            (cache.page.tracks.clone(), cache.continuation.clone())
        } else {
            (Vec::new(), None)
        };
        let (context, revision) = self.context().await?;
        if revision != expected_revision {
            return Err(AppError::cancelled());
        }
        let mut body = json!({"context":context, "browseId":target.item.id});
        if let Some(params) = &target.params {
            body["params"] = json!(params);
        }
        if let Some(token) = &continuation {
            body["continuation"] = json!(token);
        }
        let data = self
            .browse_request(body, continuation.as_deref(), revision)
            .await?;
        let (mut page, continuation) = home::parse_collection(&data, &target)?;
        let mut tracks = previous;
        let mut ids: BTreeSet<_> = tracks.iter().map(|track| track.id.clone()).collect();
        tracks.extend(
            page.tracks
                .into_iter()
                .filter(|track| ids.insert(track.id.clone())),
        );
        tracks.truncate(100);
        page.tracks = tracks;
        page.has_more = continuation.is_some() && page.tracks.len() < 100;
        self.remember(&page.tracks)?;
        *self.collection.lock().map_err(|_| AppError::internal())? = Some(CachedCollection {
            page: page.clone(),
            continuation,
            revision,
        });
        Ok(page)
    }

    async fn browse_request(
        &self,
        body: Value,
        continuation: Option<&str>,
        revision: u64,
    ) -> Result<Value> {
        let mut endpoint = url::Url::parse(&format!(
            "{}/youtubei/v1/browse?prettyPrint=false",
            self.origin
        ))
        .map_err(|_| catalog_error())?;
        if let Some(token) = continuation {
            endpoint
                .query_pairs_mut()
                .append_pair("ctoken", token)
                .append_pair("continuation", token);
        }
        let version = body
            .pointer("/context/client/clientVersion")
            .and_then(Value::as_str)
            .ok_or_else(catalog_error)?;
        let request = self
            .client
            .post(endpoint)
            .header("Origin", &self.origin)
            .header("Referer", format!("{}/", self.origin))
            .header("X-YouTube-Client-Name", "67")
            .header("X-YouTube-Client-Version", version)
            .json(&body);
        let response = self
            .authenticate(request, revision)?
            .send()
            .await
            .map_err(|_| catalog_error())?;
        self.response(response, revision).await
    }

    fn remember(&self, tracks: &[Track]) -> Result<()> {
        let mut known = self.known_tracks.lock().map_err(|_| AppError::internal())?;
        for track in tracks {
            known.retain(|previous| previous.id != track.id);
            known.push_front(track.clone());
        }
        known.truncate(256);
        Ok(())
    }

    async fn search_inner(
        &self,
        query: &str,
        kind: SearchKind,
        more: bool,
    ) -> Result<SearchResults> {
        let _slot = self
            .slot
            .acquire()
            .await
            .map_err(|_| AppError::internal())?;
        let (previous, continuation) = {
            let cache = self.cache.lock().map_err(|_| AppError::internal())?;
            let matching = cache.as_ref().filter(|cache| {
                cache.results.query == query
                    && cache.results.kind == kind
                    && cache.revision == self.account.snapshot().revision
            });
            if !more
                && let Some(cache) =
                    matching.filter(|cache| cache.created.elapsed() < Duration::from_secs(60))
            {
                return Ok(cache.results.clone());
            }
            if more {
                let cache = matching.ok_or_else(|| {
                    AppError::new(
                        "stale_search",
                        "Run this search again to load more results.",
                        true,
                    )
                })?;
                if cache.continuation.is_none() {
                    return Ok(cache.results.clone());
                }
                (cache.results.tracks.clone(), cache.continuation.clone())
            } else {
                (Vec::new(), None)
            }
        };
        let (context, revision) = self.context().await?;
        let version = context
            .pointer("/client/clientVersion")
            .and_then(Value::as_str)
            .ok_or_else(catalog_error)?;
        let body = json!({ "context": context, "query": query, "params": kind.params() });
        let mut endpoint = url::Url::parse(&format!(
            "{}/youtubei/v1/search?prettyPrint=false",
            self.origin
        ))
        .map_err(|_| catalog_error())?;
        if let Some(continuation) = &continuation {
            endpoint
                .query_pairs_mut()
                .append_pair("ctoken", continuation)
                .append_pair("continuation", continuation);
        }
        let request = self
            .client
            .post(endpoint)
            .header("Origin", &self.origin)
            .header("Referer", format!("{}/", self.origin))
            .header("X-YouTube-Client-Name", "67")
            .header("X-YouTube-Client-Version", version)
            .json(&body);
        let response = self
            .authenticate(request, revision)?
            .send()
            .await
            .map_err(|_| catalog_error())?;
        let data = self.response(response, revision).await?;
        let (page, continuation) = parse_results(&data, kind)?;
        let mut tracks = previous;
        let mut ids: BTreeSet<String> = tracks.iter().map(|track| track.id.clone()).collect();
        tracks.extend(
            page.into_iter()
                .filter(|track| ids.insert(track.id.clone())),
        );
        tracks.truncate(100);
        let results = SearchResults {
            query: query.to_owned(),
            kind,
            has_more: continuation.is_some() && tracks.len() < 100,
            tracks,
        };
        self.remember(&results.tracks)?;
        *self.cache.lock().map_err(|_| AppError::internal())? = Some(CachedSearch {
            results: results.clone(),
            continuation,
            created: Instant::now(),
            revision,
        });
        Ok(results)
    }

    async fn context(&self) -> Result<(Value, u64)> {
        let (revision, session) = self.account.request_session();
        if let Some(session) = self
            .session
            .lock()
            .map_err(|_| AppError::internal())?
            .as_ref()
            .filter(|session| {
                session.created.elapsed() < Duration::from_secs(15 * 60)
                    && session.revision == revision
            })
        {
            return Ok((session.context.clone(), revision));
        }
        let request = self.client.get(format!("{}/", self.origin));
        let request = if let Some(session) = session {
            session.request_headers(request)?
        } else {
            request
        };
        let response = request.send().await.map_err(|_| catalog_error())?;
        let html = bounded_body(response, 4 * 1024 * 1024).await?;
        let html = std::str::from_utf8(&html).map_err(|_| catalog_error())?;
        let context = parse_context(html)?;
        if self.account.snapshot().revision != revision {
            return Err(AppError::cancelled());
        }
        *self.session.lock().map_err(|_| AppError::internal())? = Some(Session {
            context: context.clone(),
            created: Instant::now(),
            revision,
        });
        Ok((context, revision))
    }

    fn authenticate(
        &self,
        request: reqwest::RequestBuilder,
        revision: u64,
    ) -> Result<reqwest::RequestBuilder> {
        let (current, session) = self.account.request_session();
        if current != revision {
            return Err(AppError::cancelled());
        }
        if let Some(session) = session {
            session.request_headers(request)
        } else {
            Ok(request)
        }
    }

    async fn response(&self, response: reqwest::Response, revision: u64) -> Result<Value> {
        if self.account.snapshot().revision != revision {
            return Err(AppError::cancelled());
        }
        if matches!(response.status().as_u16(), 401 | 403)
            && self.account.request_session().1.is_some()
        {
            self.account.expire(revision);
            return Err(session_error());
        }
        let data: Value = serde_json::from_slice(&bounded_body(response, 4 * 1024 * 1024).await?)
            .map_err(|_| catalog_error())?;
        if self.account.snapshot().revision != revision {
            return Err(AppError::cancelled());
        }
        if self.account.request_session().1.is_some()
            && data
                .pointer("/responseContext/mainAppWebResponseContext/loggedOut")
                .and_then(Value::as_bool)
                == Some(true)
        {
            self.account.expire(revision);
            return Err(session_error());
        }
        if data.get("error").is_some() {
            return Err(catalog_error());
        }
        Ok(data)
    }

    pub(crate) async fn verify_account(&self, session: &CookieSession) -> Result<AccountProfile> {
        let response = session
            .request_headers(self.client.get(format!("{}/", self.origin)))?
            .send()
            .await
            .map_err(|_| catalog_error())?;
        let html = bounded_body(response, 4 * 1024 * 1024).await?;
        let context = parse_context(std::str::from_utf8(&html).map_err(|_| catalog_error())?)?;
        let version = context
            .pointer("/client/clientVersion")
            .and_then(Value::as_str)
            .ok_or_else(catalog_error)?;
        let request = self
            .client
            .post(format!(
                "{}/youtubei/v1/account/account_menu?prettyPrint=false",
                self.origin
            ))
            .header("Origin", &self.origin)
            .header("Referer", format!("{}/", self.origin))
            .header("X-YouTube-Client-Name", "67")
            .header("X-YouTube-Client-Version", version)
            .json(&json!({"context":context}));
        let response = session
            .request_headers(request)?
            .send()
            .await
            .map_err(|_| catalog_error())?;
        if matches!(response.status().as_u16(), 401 | 403) {
            return Err(session_error());
        }
        let data: Value = serde_json::from_slice(&bounded_body(response, 4 * 1024 * 1024).await?)
            .map_err(|_| session_error())?;
        parse_profile(&data)
    }

    pub(crate) async fn radio(
        &self,
        seed_id: &str,
        cursor: Option<&crate::radio::RadioCursor>,
    ) -> Result<crate::radio::RadioBatch> {
        if !valid_video_id(seed_id) {
            return Err(crate::radio::radio_error());
        }
        let _slot = self
            .radio_slot
            .acquire()
            .await
            .map_err(|_| AppError::internal())?;
        let (context, revision) = self.context().await?;
        let cursor = cursor.filter(|cursor| cursor.account_revision == revision);
        let mut body = json!({"context":context,"videoId":seed_id,"playlistId":cursor.map(|cursor|cursor.playlist_id.clone()).unwrap_or_else(||format!("RDAMVM{seed_id}")),"params":"wAEB","enablePersistentPlaylistPanel":true,"isAudioOnly":true,"tunerSettingValue":"AUTOMIX_SETTING_NORMAL"});
        let mut endpoint = url::Url::parse(&format!(
            "{}/youtubei/v1/next?prettyPrint=false",
            self.origin
        ))
        .map_err(|_| catalog_error())?;
        if let Some(cursor) = cursor {
            body["videoId"] = json!(cursor.seed_id);
            body["continuation"] = json!(cursor.token);
            endpoint
                .query_pairs_mut()
                .append_pair("ctoken", &cursor.token)
                .append_pair("continuation", &cursor.token);
        }
        let version = body
            .pointer("/context/client/clientVersion")
            .and_then(Value::as_str)
            .ok_or_else(catalog_error)?;
        let request = self
            .client
            .post(endpoint)
            .header("Origin", &self.origin)
            .header("Referer", format!("{}/", self.origin))
            .header("X-YouTube-Client-Name", "67")
            .header("X-YouTube-Client-Version", version)
            .json(&body);
        let response = self
            .authenticate(request, revision)?
            .send()
            .await
            .map_err(|_| crate::radio::radio_error())?;
        let data = self.response(response, revision).await?;
        let batch = crate::radio::parse_radio(&data, seed_id, revision)?;
        self.remember(&batch.tracks)?;
        Ok(batch)
    }

    pub(crate) async fn account_browse(
        &self,
        id: &str,
        cursor: Option<&crate::account_library::BrowseCursor>,
        revision: u64,
    ) -> Result<Value> {
        self.account_collection_browse(id, None, cursor, revision)
            .await
    }

    pub(crate) async fn account_collection_browse(
        &self,
        id: &str,
        params: Option<&str>,
        cursor: Option<&crate::account_library::BrowseCursor>,
        revision: u64,
    ) -> Result<Value> {
        let mut body = json!({"browseId": id});
        if let Some(params) = params {
            body["params"] = json!(params);
        }
        let legacy = if let Some(cursor) = cursor {
            if cursor.modern {
                body = json!({"continuation": cursor.token});
                None
            } else {
                Some(cursor.token.as_str())
            }
        } else {
            None
        };
        self.account_post("browse", body, legacy, revision).await
    }

    pub(crate) async fn rate_account_song(
        &self,
        id: &str,
        liked: bool,
        revision: u64,
    ) -> Result<()> {
        if !valid_video_id(id) {
            return Err(crate::account_library::library_error());
        }
        let endpoint = if liked {
            "like/like"
        } else {
            "like/removelike"
        };
        let data = self
            .account_post(endpoint, json!({"target":{"videoId":id}}), None, revision)
            .await?;
        crate::account_library::validate_rating_response(&data)
    }

    async fn account_post(
        &self,
        endpoint: &str,
        mut body: Value,
        continuation: Option<&str>,
        expected_revision: u64,
    ) -> Result<Value> {
        self.require_account(expected_revision)?;
        let (context, revision) = self.context().await?;
        if revision != expected_revision {
            return Err(AppError::cancelled());
        }
        body["context"] = context;
        let version = body
            .pointer("/context/client/clientVersion")
            .and_then(Value::as_str)
            .ok_or_else(catalog_error)?;
        let mut endpoint = url::Url::parse(&format!(
            "{}/youtubei/v1/{endpoint}?prettyPrint=false",
            self.origin
        ))
        .map_err(|_| catalog_error())?;
        if let Some(token) = continuation {
            endpoint
                .query_pairs_mut()
                .append_pair("ctoken", token)
                .append_pair("continuation", token);
        }
        let request = self
            .client
            .post(endpoint)
            .header("Origin", &self.origin)
            .header("Referer", format!("{}/", self.origin))
            .header("X-YouTube-Client-Name", "67")
            .header("X-YouTube-Client-Version", version)
            .json(&body);
        let session = self.require_account(expected_revision)?;
        let response = session
            .request_headers(request)?
            .send()
            .await
            .map_err(|_| crate::account_library::library_error())?;
        let data = self.response(response, expected_revision).await?;
        self.require_account(expected_revision)?;
        Ok(data)
    }

    fn require_account(&self, revision: u64) -> Result<Arc<CookieSession>> {
        let state = self.account.snapshot();
        let (current, session) = self.account.request_session();
        if state.revision != revision || current != revision {
            return Err(AppError::cancelled());
        }
        if state.mode != crate::account::AccountMode::Connected {
            return Err(crate::account_library::account_required());
        }
        session.ok_or_else(crate::account_library::account_required)
    }

    #[cfg(test)]
    pub(crate) fn with_account_test_server(account: Arc<Account>, origin: String) -> Self {
        assert!(origin.starts_with("http://127.0.0.1:"));
        let revision = account.snapshot().revision;
        let mut catalog = Self::with_account(account).unwrap();
        catalog.client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        catalog.origin = origin;
        *catalog.session.lock().unwrap() = Some(Session {
            context: json!({"client":{"clientVersion":"fixture","clientName":"WEB_REMIX"}}),
            created: Instant::now(),
            revision,
        });
        catalog
    }
}

async fn bounded_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(catalog_error());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| catalog_error())? {
        if bytes.len() + chunk.len() > limit {
            return Err(catalog_error());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn parse_context(html: &str) -> Result<Value> {
    let marker = "\"INNERTUBE_CONTEXT\":";
    let offset = html.find(marker).ok_or_else(catalog_error)? + marker.len();
    let mut values = serde_json::Deserializer::from_str(&html[offset..]).into_iter::<Value>();
    let mut context = values
        .next()
        .and_then(std::result::Result::ok)
        .ok_or_else(catalog_error)?;
    if context
        .pointer("/client/clientVersion")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err(catalog_error());
    }
    context["client"]["hl"] = json!("en");
    Ok(context)
}

fn parse_results(data: &Value, kind: SearchKind) -> Result<(Vec<Track>, Option<String>)> {
    let contents = data
        .get("continuationContents")
        .or_else(|| data.get("contents"))
        .or_else(|| data.get("onResponseReceivedActions"))
        .ok_or_else(catalog_error)?;
    let mut tracks = Vec::new();
    let mut continuation = None;
    walk(contents, &mut |value| {
        if let Some(item) = value.get("musicResponsiveListItemRenderer")
            && let Some(track) = parse_track(item, Some(kind))
        {
            tracks.push(track);
        }
        if let Some(token) = value
            .pointer("/nextContinuationData/continuation")
            .or_else(|| value.pointer("/continuationCommand/token"))
            .and_then(Value::as_str)
            && token.len() <= 8192
        {
            continuation = Some(token.to_owned());
        }
    });
    let mut ids = BTreeSet::new();
    tracks.retain(|track| ids.insert(track.id.clone()));
    Ok((tracks, continuation))
}

pub(crate) fn parse_track(item: &Value, kind: Option<SearchKind>) -> Option<Track> {
    if item
        .get("musicItemRendererDisplayPolicy")
        .and_then(Value::as_str)
        == Some("MUSIC_ITEM_RENDERER_DISPLAY_POLICY_GREY_OUT")
    {
        return None;
    }
    let title_runs = item
        .pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs")?
        .as_array()?;
    let endpoint = item.pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint")
        .or_else(|| title_runs.first()?.pointer("/navigationEndpoint/watchEndpoint"));
    let video_type = endpoint
        .and_then(|endpoint| {
            endpoint.pointer(
                "/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType",
            )
        })
        .and_then(Value::as_str);
    if kind == Some(SearchKind::Songs)
        && video_type.is_some_and(|kind| kind != "MUSIC_VIDEO_TYPE_ATV")
    {
        return None;
    }
    if kind == Some(SearchKind::Videos) && video_type == Some("MUSIC_VIDEO_TYPE_ATV") {
        return None;
    }
    let id = item
        .pointer("/playlistItemData/videoId")
        .or_else(|| endpoint?.get("videoId"))?
        .as_str()?;
    if !valid_video_id(id) {
        return None;
    }
    let title = title_runs
        .iter()
        .filter_map(|run| run.get("text")?.as_str())
        .collect::<String>();
    if title.is_empty() {
        return None;
    }
    let metadata = item
        .pointer("/flexColumns/1/musicResponsiveListItemFlexColumnRenderer/text/runs")?
        .as_array()?;
    let mut artists = Vec::new();
    let mut album = None;
    let mut duration = None;
    for run in metadata {
        let Some(text) = run.get("text").and_then(Value::as_str) else {
            continue;
        };
        let page_type = run.pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType").and_then(Value::as_str);
        match page_type {
            Some("MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL") => {
                artists.push(text.to_owned())
            }
            Some("MUSIC_PAGE_TYPE_ALBUM") => album = Some(text.to_owned()),
            _ => {
                if let Some(seconds) = parse_duration(text) {
                    duration = Some(seconds);
                }
            }
        }
    }
    if artists.is_empty()
        && let Some(text) = metadata
            .first()
            .and_then(|run| run.get("text"))
            .and_then(Value::as_str)
    {
        artists.push(text.to_owned());
    }
    if duration.is_none() {
        duration = item
            .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text/runs/0/text")
            .and_then(Value::as_str)
            .and_then(parse_duration);
    }
    let thumbnail_url = item
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
        .and_then(Value::as_array)
        .and_then(|thumbnails| {
            thumbnails
                .iter()
                .rev()
                .find_map(|thumbnail| artwork_url(thumbnail.get("url")?.as_str()?))
        });
    Some(Track {
        id: id.into(),
        title,
        artist: artists.join(" & "),
        duration_seconds: duration,
        album,
        thumbnail_url,
    })
}

pub(crate) fn walk<'a>(value: &'a Value, visit: &mut impl FnMut(&'a Value)) {
    visit(value);
    match value {
        Value::Object(map) => map.values().for_each(|value| walk(value, visit)),
        Value::Array(values) => values.iter().for_each(|value| walk(value, visit)),
        _ => {}
    }
}

pub(crate) fn valid_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub(crate) fn parse_duration(text: &str) -> Option<f64> {
    let parts: Vec<_> = text.split(':').collect();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let mut seconds = 0_u32;
    for (index, part) in parts.iter().enumerate() {
        let part = part.parse::<u32>().ok()?;
        if index > 0 && part >= 60 {
            return None;
        }
        seconds = seconds.checked_mul(60)?.checked_add(part)?;
    }
    (seconds > 0).then_some(f64::from(seconds))
}

fn catalog_error() -> AppError {
    AppError::new(
        "catalog_unavailable",
        "YouTube Music is unavailable right now. Check your connection and try again.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requests a real YouTube Music radio and continuation; no account or audio"]
    async fn live_radio_continuation_returns_new_recommendations() {
        let catalog = MusicCatalog::new().unwrap();
        let first = catalog.radio("khnokW3Mw24", None).await.unwrap();
        assert!(first.tracks.len() >= 10);
        let cursor = first.cursor.as_ref().expect("provider radio continuation");
        let next = catalog.radio("khnokW3Mw24", Some(cursor)).await.unwrap();
        let ids: BTreeSet<_> = first.tracks.iter().map(|track| &track.id).collect();
        let new = next
            .tracks
            .iter()
            .filter(|track| !ids.contains(&track.id))
            .count();
        println!(
            "Live radio: {} initial tracks; {} continuation tracks, {new} new",
            first.tracks.len(),
            next.tracks.len()
        );
        assert!(new > 0, "radio continuation must extend the sequence");
        assert!(next.tracks.iter().all(|track| valid_video_id(&track.id)));
    }

    #[test]
    fn bootstrap_parses_only_the_context_object_and_rejects_changed_responses() {
        let context = parse_context(r#"ytcfg.set({"INNERTUBE_CONTEXT":{"client":{"clientVersion":"1.test","hl":"pl"}},"other":42});"#).unwrap();
        assert_eq!(context["client"]["hl"], "en");
        assert!(parse_context("consent page").is_err());
    }

    #[test]
    fn durations_are_real_times_and_not_play_counts() {
        assert_eq!(parse_duration("5:38"), Some(338.0));
        assert_eq!(parse_duration("1:02:03"), Some(3723.0));
        assert_eq!(parse_duration("1.2B plays"), None);
        assert_eq!(parse_duration("3:99"), None);
    }

    #[test]
    fn continuation_payload_takes_precedence_over_repeated_initial_contents() {
        let data = json!({
            "contents": {"nextContinuationData": {"continuation": "old-page"}},
            "continuationContents": {"musicShelfContinuation": {"contents": [], "continuations": [{"nextContinuationData": {"continuation": "next-page"}}]}}
        });
        let (_, continuation) = parse_results(&data, SearchKind::Songs).unwrap();
        assert_eq!(continuation.as_deref(), Some("next-page"));
    }

    #[test]
    fn parses_realistic_song_metadata_without_leaking_provider_payloads() {
        let item = json!({
            "playlistItemData": {"videoId": "khnokW3Mw24"},
            "flexColumns": [
              {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Instant Crush"}]}}},
              {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[
                {"text":"Daft Punk","navigationEndpoint":{"browseEndpoint":{"browseEndpointContextSupportedConfigs":{"browseEndpointContextMusicConfig":{"pageType":"MUSIC_PAGE_TYPE_ARTIST"}}}}},
                {"text":" • "}, {"text":"Random Access Memories","navigationEndpoint":{"browseEndpoint":{"browseEndpointContextSupportedConfigs":{"browseEndpointContextMusicConfig":{"pageType":"MUSIC_PAGE_TYPE_ALBUM"}}}}},
                {"text":" • "}, {"text":"5:38"}
              ]}}}
            ],
            "thumbnail":{"musicThumbnailRenderer":{"thumbnail":{"thumbnails":[{"url":"https://yt3.googleusercontent.com/art=w120-h120"}]}}}
        });
        let response = json!({"contents":{"musicShelfRenderer":{"contents":[{"musicResponsiveListItemRenderer":item}],"continuations":[{"nextContinuationData":{"continuation":"private-cursor"}}]}}});
        let (tracks, cursor) = parse_results(&response, SearchKind::Songs).unwrap();
        assert_eq!(tracks[0].artist, "Daft Punk");
        assert_eq!(tracks[0].album.as_deref(), Some("Random Access Memories"));
        assert_eq!(tracks[0].duration_seconds, Some(338.0));
        assert_eq!(cursor.as_deref(), Some("private-cursor"));
        let public = serde_json::to_string(&tracks).unwrap();
        assert!(!public.contains("private-cursor"));
        assert!(!public.contains("navigationEndpoint"));
    }
}
