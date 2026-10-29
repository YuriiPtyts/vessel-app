//! Rust-owned SoundCloud website compatibility: catalog, streams, and independent accounts.
pub(crate) mod auth;
mod client;
pub mod sign_in;

use crate::{
    account::{AccountSnapshot, MemoryVault, SessionVault},
    account_library::{AccountLibrarySnapshot, PlaylistMutation, PlaylistPrivacy},
    catalog::{SearchKind, SearchResults},
    error::{AppError, Result},
    home::{CollectionPage, HomeFeed, HomeItem, HomeItemKind, HomeShelf},
    provider::{AudioFormat, ResolvedTrack, Track},
    radio::{RadioBatch, RadioCursor},
};
use reqwest::Method;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, watch};
use url::Url;

#[derive(Clone)]
struct Target {
    item: HomeItem,
    endpoint: String,
}
struct Collection {
    page: CollectionPage,
    ids: Vec<u64>,
    offset: usize,
    revision: u64,
}
struct State {
    library: AccountLibrarySnapshot,
    refreshed: Option<Instant>,
    known: VecDeque<Track>,
    links: BTreeMap<String, String>,
    targets: BTreeMap<String, Target>,
    search: Option<(SearchResults, Option<Url>, u64)>,
    home: Option<(HomeFeed, Instant, u64)>,
    collection: Option<Collection>,
    streams: VecDeque<(String, ResolvedTrack, Instant)>,
}
enum LibraryPart {
    Likes,
    Playlists,
}
pub(crate) struct SoundCloud {
    pub auth: Arc<auth::Auth>,
    api: client::Api,
    state: Mutex<State>,
    browse: AsyncMutex<()>,
    writes: AsyncMutex<()>,
    emit: Arc<dyn Fn(AccountLibrarySnapshot) + Send + Sync>,
    restored: std::sync::atomic::AtomicBool,
}
impl SoundCloud {
    pub fn new(
        vault: Arc<dyn SessionVault>,
        emit: Arc<dyn Fn(AccountLibrarySnapshot) + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let auth = auth::Auth::new(vault);
        Ok(Arc::new(Self {
            api: client::Api::new(auth.clone())?,
            auth,
            state: Mutex::new(State {
                library: AccountLibrarySnapshot::empty(0, 0, false),
                refreshed: None,
                known: VecDeque::new(),
                links: BTreeMap::new(),
                targets: BTreeMap::new(),
                search: None,
                home: None,
                collection: None,
                streams: VecDeque::new(),
            }),
            browse: AsyncMutex::new(()),
            writes: AsyncMutex::new(()),
            emit,
            restored: std::sync::atomic::AtomicBool::new(false),
        }))
    }
    pub fn desktop(
        path: &Path,
        isolated: bool,
        emit: Arc<dyn Fn(AccountLibrarySnapshot) + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let vault: Arc<dyn SessionVault> = if isolated {
            Arc::new(MemoryVault::default())
        } else {
            #[cfg(target_os = "macos")]
            {
                let _ = path;
                Arc::new(auth::SoundCloudVault)
            }
            #[cfg(target_os = "windows")]
            {
                Arc::new(crate::account::WindowsVault::new(
                    path.join("soundcloud-session.dpapi"),
                ))
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let _ = path;
                Arc::new(MemoryVault::default())
            }
        };
        Self::new(vault, emit)
    }
    pub fn subscribe(&self) -> watch::Receiver<AccountSnapshot> {
        self.auth.subscribe()
    }
    pub fn account(&self) -> AccountSnapshot {
        self.auth.snapshot()
    }
    pub fn invalidate(&self) {
        if let Ok(mut state) = self.state.lock() {
            let revision = self.auth.snapshot().revision;
            let next = state.library.revision + 1;
            state.library =
                AccountLibrarySnapshot::empty(revision, next, self.auth.session().1.is_some());
            state.refreshed = None;
            state.known.clear();
            state.links.clear();
            state.targets.clear();
            state.search = None;
            state.home = None;
            state.collection = None;
            state.streams.clear();
            (self.emit)(state.library.clone());
        }
    }
    fn revision(&self, revision: u64) -> Result<()> {
        if self.auth.snapshot().revision == revision {
            Ok(())
        } else {
            Err(AppError::cancelled())
        }
    }
    fn connected(&self, revision: u64) -> Result<Arc<auth::Session>> {
        self.revision(revision)?;
        self.auth.session().1.ok_or_else(account_required)
    }
    pub fn library(&self) -> Result<AccountLibrarySnapshot> {
        let snapshot = self.auth.snapshot();
        let stale = self
            .state
            .lock()
            .map_err(|_| AppError::internal())?
            .library
            .account_revision
            != snapshot.revision;
        if stale {
            self.invalidate();
        }
        self.state
            .lock()
            .map(|state| state.library.clone())
            .map_err(|_| AppError::internal())
    }
    pub async fn verify(
        &self,
        token: &str,
        cookies: Vec<crate::account::SessionCookie>,
    ) -> Result<auth::Session> {
        let (user_id, profile) = self.api.candidate_profile(token).await?;
        Ok(auth::Session {
            token: token.into(),
            user_id,
            profile,
            cookies,
        })
    }
    pub async fn restore(&self) {
        if self
            .restored
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let id = self.auth.begin(true);
        match self.auth.saved().await {
            Ok(Some(saved)) => match self.verify(&saved.token, saved.cookies).await {
                Ok(session) => {
                    if let Err(error) = self.auth.activate(id, session, false).await {
                        self.auth.fail(id, error);
                    }
                }
                Err(error) => self.auth.fail(id, error),
            },
            Ok(None) => self.auth.cancel(id),
            Err(error) => self.auth.fail(id, error),
        }
    }
    fn remember(&self, values: &[Value], tracks: &[Track]) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| AppError::internal())?;
        for value in values {
            if let Some(track) = parse_track(value) {
                let link = if value.get("public").and_then(Value::as_bool) == Some(false) {
                    value.pointer("/user/permalink_url")
                } else {
                    value.get("permalink_url")
                };
                if let Some(url) = link.and_then(Value::as_str).and_then(public_page) {
                    state.links.insert(track.id, url);
                }
            }
        }
        for track in tracks {
            state.known.retain(|item| item.id != track.id);
            state.known.push_front(track.clone());
        }
        state.known.truncate(512);
        if state.links.len() > 10_000 {
            state.links.clear();
        }
        Ok(())
    }
    pub fn find_track(&self, id: &str) -> Option<Track> {
        let state = self.state.lock().ok()?;
        state
            .known
            .iter()
            .chain(state.library.liked.iter())
            .chain(
                state
                    .collection
                    .iter()
                    .flat_map(|page| page.page.tracks.iter()),
            )
            .find(|track| track.id == id)
            .cloned()
    }
    pub async fn search(&self, query: &str, kind: SearchKind, more: bool) -> Result<SearchResults> {
        if kind == SearchKind::Videos {
            return Err(AppError::new(
                "soundcloud_search_kind",
                "SoundCloud mode searches audio tracks. Choose Songs.",
                false,
            ));
        }
        let query = query.trim();
        if !(2..=160).contains(&query.chars().count()) || query.chars().any(char::is_control) {
            return Err(AppError::new(
                "invalid_query",
                "Search with 2–160 characters.",
                false,
            ));
        }
        let _work = self.browse.lock().await;
        let revision = self.auth.snapshot().revision;
        let (mut tracks, next) = if more {
            let state = self.state.lock().map_err(|_| AppError::internal())?;
            let (previous, next, saved) = state
                .search
                .as_ref()
                .filter(|(previous, _, saved)| previous.query == query && *saved == revision)
                .ok_or_else(|| {
                    AppError::new(
                        "stale_search",
                        "Search again before loading more SoundCloud tracks.",
                        true,
                    )
                })?;
            let _ = saved;
            if next.is_none() {
                return Ok(previous.clone());
            }
            (previous.tracks.clone(), next.clone())
        } else {
            (Vec::new(), None)
        };
        let data = if let Some(next) = next {
            self.api
                .request_url(Method::GET, next, &[], None, revision, None)
                .await?
        } else {
            self.api
                .request(
                    Method::GET,
                    "search/tracks",
                    &[
                        ("q", query.into()),
                        ("limit", "25".into()),
                        ("offset", "0".into()),
                        ("linked_partitioning", "1".into()),
                    ],
                    None,
                    revision,
                )
                .await?
        };
        let values = rows(&data)?;
        let mut ids: BTreeSet<_> = tracks.iter().map(|track| track.id.clone()).collect();
        tracks.extend(
            values
                .iter()
                .filter_map(parse_track)
                .filter(|track| ids.insert(track.id.clone())),
        );
        tracks.truncate(100);
        let next = next_page(&data)?;
        let result = SearchResults {
            query: query.into(),
            kind,
            tracks,
            has_more: next.is_some() && ids.len() < 100,
        };
        self.revision(revision)?;
        self.remember(&values, &result.tracks)?;
        self.state.lock().map_err(|_| AppError::internal())?.search =
            Some((result.clone(), next, revision));
        Ok(result)
    }
    pub async fn home(&self, refresh: bool) -> Result<HomeFeed> {
        let revision = self.auth.snapshot().revision;
        if !refresh
            && let Some((feed, _, _)) = self
                .state
                .lock()
                .map_err(|_| AppError::internal())?
                .home
                .as_ref()
                .filter(|(_, time, scope)| {
                    *scope == revision && time.elapsed() < Duration::from_secs(300)
                })
        {
            return Ok(feed.clone());
        }
        let data = self
            .api
            .request(
                Method::GET,
                "mixed-selections",
                &[("limit", "12".into())],
                None,
                revision,
            )
            .await?;
        let mut shelves = Vec::new();
        let mut targets = BTreeMap::new();
        let mut known = Vec::new();
        for selection in rows(&data)?.iter().take(12) {
            let Some(title) = selection.get("title").and_then(Value::as_str) else {
                continue;
            };
            let mut items = Vec::new();
            for value in selection
                .pointer("/items/collection")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(30)
            {
                if let Some(track) = parse_track(value) {
                    known.push(value.clone());
                    items.push(HomeItem {
                        id: track.id.clone(),
                        kind: HomeItemKind::Song,
                        title: track.title.clone(),
                        subtitle: track.artist.clone(),
                        thumbnail_url: track.thumbnail_url.clone(),
                        track: Some(track),
                    });
                } else if let Some(target) = parse_target(value) {
                    items.push(target.item.clone());
                    targets.insert(target.item.id.clone(), target);
                }
            }
            if !items.is_empty() {
                shelves.push(HomeShelf {
                    id: format!("sc-shelf-{}", shelves.len()),
                    title: title.chars().take(512).collect(),
                    items,
                });
            }
        }
        if shelves.is_empty() {
            return Err(unavailable());
        }
        let feed = HomeFeed {
            shelves,
            personalized: self.auth.session().1.is_some(),
        };
        self.revision(revision)?;
        self.remember(
            &known,
            &known.iter().filter_map(parse_track).collect::<Vec<_>>(),
        )?;
        let mut state = self.state.lock().map_err(|_| AppError::internal())?;
        state.targets.extend(targets);
        state.home = Some((feed.clone(), Instant::now(), revision));
        Ok(feed)
    }
    pub async fn collection(&self, id: &str, more: bool, expected: u64) -> Result<CollectionPage> {
        self.revision(expected)?;
        let _work = self.browse.lock().await;
        let (target, mut ids, mut tracks, offset) = {
            let state = self.state.lock().map_err(|_| AppError::internal())?;
            let target = state.targets.get(id).cloned().ok_or_else(|| {
                AppError::new(
                    "stale_collection",
                    "Refresh SoundCloud and open this playlist again.",
                    true,
                )
            })?;
            if more {
                let cached = state
                    .collection
                    .as_ref()
                    .filter(|cached| cached.page.item.id == id && cached.revision == expected)
                    .ok_or_else(unavailable)?;
                (
                    target,
                    cached.ids.clone(),
                    cached.page.tracks.clone(),
                    cached.offset,
                )
            } else {
                (target, Vec::new(), Vec::new(), 0)
            }
        };
        if !more {
            let data = self
                .api
                .request(
                    Method::GET,
                    &target.endpoint,
                    &[("representation", "full".into())],
                    None,
                    expected,
                )
                .await?;
            let raw = data
                .get("tracks")
                .and_then(Value::as_array)
                .ok_or_else(unavailable)?;
            if raw.len() > 10_000 {
                return Err(library_limit());
            }
            ids = raw
                .iter()
                .filter_map(|track| track.get("id").and_then(Value::as_u64))
                .collect();
            if let Some(count) = data.get("track_count").and_then(Value::as_u64)
                && count as usize > ids.len()
            {
                return Err(unavailable());
            }
            let parsed: Vec<_> = raw.iter().filter_map(parse_track).collect();
            self.remember(raw, &parsed)?;
        }
        let end = (offset + 50).min(ids.len());
        if end > offset {
            let data = self
                .api
                .request(
                    Method::GET,
                    "tracks",
                    &[(
                        "ids",
                        ids[offset..end]
                            .iter()
                            .map(u64::to_string)
                            .collect::<Vec<_>>()
                            .join(","),
                    )],
                    None,
                    expected,
                )
                .await?;
            let values = data.as_array().cloned().ok_or_else(unavailable)?;
            let parsed: Vec<_> = values.iter().filter_map(parse_track).collect();
            self.remember(&values, &parsed)?;
            let by_id: BTreeMap<_, _> = parsed
                .into_iter()
                .map(|track| (track.id.clone(), track))
                .collect();
            let mut seen: BTreeSet<_> = tracks.iter().map(|track| track.id.clone()).collect();
            tracks.extend(
                ids[offset..end]
                    .iter()
                    .filter_map(|id| by_id.get(&format!("sc:{id}")).cloned())
                    .filter(|track| seen.insert(track.id.clone())),
            );
        }
        self.revision(expected)?;
        let page = CollectionPage {
            item: target.item,
            tracks,
            has_more: end < ids.len(),
        };
        self.state
            .lock()
            .map_err(|_| AppError::internal())?
            .collection = Some(Collection {
            page: page.clone(),
            ids,
            offset: end,
            revision: expected,
        });
        Ok(page)
    }
    pub async fn refresh_library(
        &self,
        force: bool,
        revision: u64,
    ) -> Result<AccountLibrarySnapshot> {
        let _work = self.writes.lock().await;
        let session = self.connected(revision)?;
        if !force {
            let state = self.state.lock().map_err(|_| AppError::internal())?;
            if state.library.account_revision == revision
                && state
                    .refreshed
                    .is_some_and(|time| time.elapsed() < Duration::from_secs(60))
            {
                return Ok(state.library.clone());
            }
        }
        let loading = {
            let mut state = self.state.lock().map_err(|_| AppError::internal())?;
            state.library = if state.library.account_revision == revision {
                state.library.clone()
            } else {
                AccountLibrarySnapshot::empty(revision, state.library.revision + 1, true)
            };
            state.library.refreshing = true;
            state.library.likes_error = None;
            state.library.playlists_error = None;
            state.library.revision += 1;
            state.library.clone()
        };
        (self.emit)(loading);
        let likes_path = format!("users/{}/likes", session.user_id);
        let playlists_path = format!("users/{}/playlists", session.user_id);
        let likes = self.pages(&likes_path, revision, 10_000);
        let playlists = self.pages(&playlists_path, revision, 1_000);
        tokio::pin!(likes, playlists);
        let (mut likes_done, mut playlists_done) = (false, false);
        for _ in 0..2 {
            let (part, result) = tokio::select! {
                result = &mut likes, if !likes_done => { likes_done = true; (LibraryPart::Likes, result) },
                result = &mut playlists, if !playlists_done => { playlists_done = true; (LibraryPart::Playlists, result) },
            };
            // A completed section is useful immediately. A slow sibling must
            // not hold it behind an all-or-nothing join; stale accounts still
            // cannot publish either section.
            self.connected(revision)?;
            let snapshot = {
                let mut state = self.state.lock().map_err(|_| AppError::internal())?;
                if state.library.account_revision != revision {
                    return Err(AppError::cancelled());
                }
                match (part, result) {
                    (LibraryPart::Likes, Ok(values)) => {
                        let mut seen = BTreeSet::new();
                        let tracks: Vec<_> = values
                            .iter()
                            .filter_map(|value| parse_track(value.get("track").unwrap_or(value)))
                            .filter(|track| seen.insert(track.id.clone()))
                            .collect();
                        state.library.liked_ids =
                            tracks.iter().map(|track| track.id.clone()).collect();
                        state.library.liked = tracks;
                        state.library.likes_ready = true;
                    }
                    (LibraryPart::Likes, Err(error)) => state.library.likes_error = Some(error),
                    (LibraryPart::Playlists, Ok(values)) => {
                        let mut seen = BTreeSet::new();
                        let targets: Vec<_> = values
                            .iter()
                            .filter_map(parse_target)
                            .filter(|target| seen.insert(target.item.id.clone()))
                            .collect();
                        state.library.editable_playlist_ids = values
                            .iter()
                            .filter(|value| {
                                value
                                    .pointer("/user/id")
                                    .and_then(Value::as_u64)
                                    .or_else(|| value.get("user_id").and_then(Value::as_u64))
                                    == Some(session.user_id)
                            })
                            .filter_map(parse_target)
                            .map(|target| target.item.id)
                            .collect();
                        state.library.playlists =
                            targets.iter().map(|target| target.item.clone()).collect();
                        for target in targets {
                            state.targets.insert(target.item.id.clone(), target);
                        }
                        state.library.playlists_ready = true;
                    }
                    (LibraryPart::Playlists, Err(error)) => {
                        state.library.playlists_error = Some(error)
                    }
                }
                state.library.connected = true;
                state.library.refreshing = !(likes_done && playlists_done);
                state.library.revision += 1;
                if !state.library.refreshing
                    && state.library.likes_error.is_none()
                    && state.library.playlists_error.is_none()
                {
                    state.refreshed = Some(Instant::now());
                }
                state.library.clone()
            };
            (self.emit)(snapshot);
        }
        self.library()
    }
    async fn pages(&self, path: &str, revision: u64, limit: usize) -> Result<Vec<Value>> {
        let work = async {
            let mut result = Vec::new();
            let mut next = Some(
                Url::parse("https://api-v2.soundcloud.com/")
                    .unwrap()
                    .join(path)
                    .map_err(|_| unavailable())?,
            );
            let mut seen = BTreeSet::new();
            for _ in 0..200 {
                let Some(url) = next.take() else {
                    return Ok(result);
                };
                if !seen.insert(url.as_str().to_owned()) {
                    return Err(unavailable());
                }
                let data = self
                    .api
                    .request_url(
                        Method::GET,
                        url,
                        &[("limit", "100".into()), ("linked_partitioning", "1".into())],
                        None,
                        revision,
                        None,
                    )
                    .await?;
                result.extend(rows(&data)?);
                if result.len() > limit {
                    return Err(library_limit());
                }
                next = next_page(&data)?;
            }
            Err(library_limit())
        };
        tokio::time::timeout(Duration::from_secs(120), work)
            .await
            .map_err(|_| unavailable())?
    }
    pub async fn set_like(
        &self,
        id: &str,
        track: Track,
        liked: bool,
        revision: u64,
    ) -> Result<AccountLibrarySnapshot> {
        let session = self.connected(revision)?;
        let number = track_id(id).ok_or_else(unavailable)?;
        let _work = self.writes.lock().await;
        let before = self.library()?;
        if !before.likes_ready {
            return Err(AppError::new(
                "account_library_loading",
                "Wait for your SoundCloud likes to finish loading.",
                true,
            ));
        }
        if before.liked_ids.iter().any(|value| value == id) == liked {
            return Ok(before);
        }
        self.api
            .request(
                if liked { Method::PUT } else { Method::DELETE },
                &format!("users/{}/track_likes/{number}", session.user_id),
                &[],
                Some(json!({})),
                revision,
            )
            .await
            .map_err(write_error)?;
        self.connected(revision)?;
        let mut state = self.state.lock().map_err(|_| AppError::internal())?;
        state.library.liked.retain(|track| track.id != id);
        state.library.liked_ids.retain(|value| value != id);
        if liked {
            state.library.liked_ids.push(id.into());
            state.library.liked.insert(0, track);
        }
        state.library.revision += 1;
        let snapshot = state.library.clone();
        drop(state);
        (self.emit)(snapshot.clone());
        Ok(snapshot)
    }
    pub async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        privacy: PlaylistPrivacy,
        track: Option<&str>,
        revision: u64,
    ) -> Result<PlaylistMutation> {
        self.connected(revision)?;
        let title = title.trim();
        if !(1..=100).contains(&title.chars().count())
            || title.chars().any(char::is_control)
            || description.chars().count() > 5000
        {
            return Err(AppError::new(
                "invalid_playlist",
                "Use a name of 1–100 characters and a description of up to 5,000 characters.",
                false,
            ));
        }
        let sharing = match privacy {
            PlaylistPrivacy::Private => "private",
            PlaylistPrivacy::Public => "public",
            PlaylistPrivacy::Unlisted => {
                return Err(AppError::new(
                    "invalid_playlist",
                    "SoundCloud playlists support Private or Public visibility.",
                    false,
                ));
            }
        };
        let initial = track
            .map(|id| track_id(id).ok_or_else(unavailable))
            .transpose()?;
        let _work = self.writes.lock().await;
        let data=self.api.request(Method::POST,"playlists",&[],Some(json!({"playlist":{"title":title,"description":description,"sharing":sharing,"tracks":initial.into_iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}})),revision).await.map_err(write_error)?;
        let target = parse_target(&data).ok_or_else(|| write_error(unavailable()))?;
        self.connected(revision)?;
        let mut state = self.state.lock().map_err(|_| AppError::internal())?;
        state.targets.insert(target.item.id.clone(), target.clone());
        state.library.account_revision = revision;
        state.library.connected = true;
        state.library.playlists.insert(0, target.item.clone());
        state
            .library
            .editable_playlist_ids
            .push(target.item.id.clone());
        state.library.revision += 1;
        state.refreshed = None;
        let library = state.library.clone();
        drop(state);
        (self.emit)(library.clone());
        Ok(PlaylistMutation {
            playlist: target.item,
            library,
        })
    }
    pub async fn add_to_playlist(
        &self,
        id: &str,
        track: &str,
        revision: u64,
    ) -> Result<PlaylistMutation> {
        let session = self.connected(revision)?;
        let number = track_id(track).ok_or_else(unavailable)?;
        let _work = self.writes.lock().await;
        let target = {
            let state = self.state.lock().map_err(|_| AppError::internal())?;
            if !state
                .library
                .editable_playlist_ids
                .iter()
                .any(|value| value == id)
            {
                return Err(account_required());
            }
            state.targets.get(id).cloned().ok_or_else(unavailable)?
        };
        let current = self
            .api
            .request(
                Method::GET,
                &target.endpoint,
                &[("representation", "full".into())],
                None,
                revision,
            )
            .await?;
        if current
            .pointer("/user/id")
            .and_then(Value::as_u64)
            .or_else(|| current.get("user_id").and_then(Value::as_u64))
            != Some(session.user_id)
        {
            return Err(account_required());
        }
        let raw = current
            .get("tracks")
            .and_then(Value::as_array)
            .ok_or_else(unavailable)?;
        let mut ids: Vec<_> = raw
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_u64))
            .collect();
        if current.get("track_count").and_then(Value::as_u64) != Some(ids.len() as u64)
            || ids.len() >= 10_000
        {
            return Err(library_limit());
        }
        if !ids.contains(&number) {
            ids.push(number);
            let data=self.api.request(Method::PUT,&target.endpoint,&[],Some(json!({"playlist":{"tracks":ids.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}})),revision).await.map_err(write_error)?;
            if data.get("id") != current.get("id")
                || !data
                    .get("tracks")
                    .and_then(Value::as_array)
                    .is_some_and(|tracks| {
                        tracks
                            .iter()
                            .any(|track| track.get("id").and_then(Value::as_u64) == Some(number))
                    })
            {
                return Err(write_error(unavailable()));
            }
        }
        self.connected(revision)?;
        let mut state = self.state.lock().map_err(|_| AppError::internal())?;
        state.collection = None;
        state.refreshed = None;
        state.library.revision += 1;
        let library = state.library.clone();
        drop(state);
        (self.emit)(library.clone());
        Ok(PlaylistMutation {
            playlist: target.item,
            library,
        })
    }
    pub async fn radio(&self, id: &str, cursor: Option<&RadioCursor>) -> Result<RadioBatch> {
        let number = track_id(id).ok_or_else(unavailable)?;
        let revision = self.auth.snapshot().revision;
        let data = if let Some(cursor) = cursor.filter(|cursor| {
            cursor.account_revision == revision && cursor.playlist_id == "soundcloud"
        }) {
            self.api
                .request_url(
                    Method::GET,
                    Url::parse(&cursor.token).map_err(|_| unavailable())?,
                    &[],
                    None,
                    revision,
                    None,
                )
                .await?
        } else {
            self.api
                .request(
                    Method::GET,
                    &format!("tracks/{number}/related"),
                    &[("limit", "50".into())],
                    None,
                    revision,
                )
                .await?
        };
        let values = rows(&data)?;
        let tracks: Vec<_> = values
            .iter()
            .filter_map(parse_track)
            .filter(|track| track.id != id)
            .collect();
        self.remember(&values, &tracks)?;
        Ok(RadioBatch {
            tracks,
            cursor: next_page(&data)?.map(|url| RadioCursor {
                seed_id: id.into(),
                playlist_id: "soundcloud".into(),
                token: url.into(),
                account_revision: revision,
            }),
        })
    }
    pub async fn resolve(&self, input: &str) -> Result<ResolvedTrack> {
        let revision = self.auth.snapshot().revision;
        if let Some((_, track, _)) = self
            .state
            .lock()
            .map_err(|_| AppError::internal())?
            .streams
            .iter()
            .find(|(id, track, created)| {
                id == input
                    && track.account_revision == revision
                    && created.elapsed() < Duration::from_secs(60)
            })
        {
            return Ok(track.clone());
        }
        let data = if let Some(id) = track_id(input) {
            self.api
                .request(Method::GET, &format!("tracks/{id}"), &[], None, revision)
                .await?
        } else {
            let url = public_page(input).ok_or_else(unavailable)?;
            self.api
                .request(Method::GET, "resolve", &[("url", url)], None, revision)
                .await?
        };
        let track=parse_track(&data).ok_or_else(||AppError::new("soundcloud_restricted","This SoundCloud recording is unavailable for full playback in this session or region.",false))?;
        let mut formats = Vec::new();
        for value in data
            .pointer("/media/transcodings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(12)
        {
            let Some((format, url)) = parse_transcoding(value) else {
                continue;
            };
            let resolved = self
                .api
                .request_url(Method::GET, url, &[], None, revision, None)
                .await?;
            if let Some(url) = resolved
                .get("url")
                .and_then(Value::as_str)
                .filter(|value| valid_media_url(value))
            {
                formats.push((format, url.into()));
            }
        }
        let resolved = ResolvedTrack::from_sources(
            track.clone(),
            formats,
            revision,
            self.auth.session().1.is_some(),
        )?;
        self.revision(revision)?;
        self.remember(&[data], std::slice::from_ref(&track))?;
        let mut state = self.state.lock().map_err(|_| AppError::internal())?;
        state.streams.retain(|(id, _, _)| id != &track.id);
        state
            .streams
            .push_front((track.id, resolved.clone(), Instant::now()));
        state.streams.truncate(8);
        Ok(resolved)
    }
    pub async fn track_page(&self, id: &str) -> Result<String> {
        if let Some(url) = self
            .state
            .lock()
            .map_err(|_| AppError::internal())?
            .links
            .get(id)
            .cloned()
        {
            return Ok(url);
        }
        let number = track_id(id).ok_or_else(unavailable)?;
        let data = self
            .api
            .request(
                Method::GET,
                &format!("tracks/{number}"),
                &[],
                None,
                self.auth.snapshot().revision,
            )
            .await?;
        let track = parse_track(&data).ok_or_else(unavailable)?;
        self.remember(&[data], &[track])?;
        self.state
            .lock()
            .map_err(|_| AppError::internal())?
            .links
            .get(id)
            .cloned()
            .ok_or_else(unavailable)
    }
}

pub(crate) fn track_id(id: &str) -> Option<u64> {
    let raw = id.strip_prefix("sc:")?;
    if raw.len() > 20 || raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok().filter(|id| *id > 0)
}
pub(crate) fn parse_track(value: &Value) -> Option<Track> {
    let id = value.get("id")?.as_u64()?;
    if id == 0
        || value
            .get("kind")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind != "track")
        || value.get("streamable").and_then(Value::as_bool) == Some(false)
        || value
            .get("policy")
            .and_then(Value::as_str)
            .is_some_and(|policy| policy != "ALLOW")
        || value
            .get("access")
            .and_then(Value::as_str)
            .is_some_and(|access| matches!(access, "blocked" | "preview"))
    {
        return None;
    }
    let title = value.get("title")?.as_str()?;
    let artist = value.pointer("/user/username")?.as_str()?;
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }
    Some(Track {
        id: format!("sc:{id}"),
        title: title.chars().take(512).collect(),
        artist: artist.chars().take(512).collect(),
        duration_seconds: value
            .get("full_duration")
            .or_else(|| value.get("duration"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| value / 1000.0),
        album: None,
        thumbnail_url: value
            .get("artwork_url")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/user/avatar_url").and_then(Value::as_str))
            .and_then(crate::provider::artwork_url),
    })
}
fn parse_target(value: &Value) -> Option<Target> {
    let kind = value.get("kind")?.as_str()?;
    let title = value.get("title")?.as_str()?;
    let (id, endpoint) = if kind == "playlist" {
        let id = value.get("id")?.as_u64()?;
        (format!("sc:playlist:{id}"), format!("playlists/{id}"))
    } else if kind == "system-playlist" {
        let urn = value
            .get("id")?
            .as_str()?
            .strip_prefix("soundcloud:system-playlists:")?;
        if urn.len() > 256
            || !urn
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_:".contains(&byte))
        {
            return None;
        }
        (
            format!("sc:system:{urn}"),
            format!("system-playlists/soundcloud:system-playlists:{urn}"),
        )
    } else {
        return None;
    };
    Some(Target {
        item: HomeItem {
            id,
            kind: HomeItemKind::Playlist,
            title: title.chars().take(512).collect(),
            subtitle: value
                .pointer("/user/username")
                .and_then(Value::as_str)
                .unwrap_or("SoundCloud")
                .chars()
                .take(512)
                .collect(),
            thumbnail_url: value
                .get("artwork_url")
                .or_else(|| value.get("calculated_artwork_url"))
                .and_then(Value::as_str)
                .and_then(crate::provider::artwork_url),
            track: None,
        },
        endpoint,
    })
}
fn parse_transcoding(value: &Value) -> Option<(AudioFormat, Url)> {
    if value.get("snipped").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let protocol = value.pointer("/format/protocol")?.as_str()?;
    if !matches!(protocol, "hls" | "progressive") {
        return None;
    }
    let preset = value.get("preset")?.as_str()?;
    let (codec, container) = if preset.starts_with("aac_") {
        ("mp4a.40.2", "m4a")
    } else if preset.starts_with("opus_") {
        ("opus", "ogg")
    } else if preset.starts_with("mp3_") {
        ("mp3", "mp3")
    } else {
        return None;
    };
    let url = Url::parse(value.get("url")?.as_str()?).ok()?;
    if !client::valid_api_url(&url) || !url.path().starts_with("/media/") {
        return None;
    }
    let bitrate = preset
        .rsplit('_')
        .next()?
        .strip_suffix('k')
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0 && *value <= 1000.0);
    Some((
        AudioFormat {
            id: format!("{protocol}_{preset}"),
            codec: codec.into(),
            container: container.into(),
            bitrate_kbps: bitrate,
            sample_rate_hz: None,
            channels: None,
        },
        url,
    ))
}
fn rows(data: &Value) -> Result<Vec<Value>> {
    data.as_array()
        .or_else(|| data.get("collection").and_then(Value::as_array))
        .cloned()
        .ok_or_else(unavailable)
}
fn next_page(data: &Value) -> Result<Option<Url>> {
    data.get("next_href")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.len() > 8192 {
                return Err(unavailable());
            }
            let url = Url::parse(value).map_err(|_| unavailable())?;
            if !client::valid_api_url(&url) {
                return Err(unavailable());
            }
            Ok(url)
        })
        .transpose()
}
pub(crate) fn public_page(value: &str) -> Option<String> {
    let mut url = Url::parse(value).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("soundcloud.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let parts: Vec<_> = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect();
    if !(1..=2).contains(&parts.len())
        || parts.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
        })
    {
        return None;
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url.into())
}
fn valid_media_url(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.host_str().is_some_and(|host| {
                host.ends_with(".sndcdn.com")
                    || host == "media.soundcloud.com"
                    || host == "playback.media-streaming.soundcloud.cloud"
            })
    })
}
pub(crate) fn unavailable() -> AppError {
    AppError::new(
        "soundcloud_unavailable",
        "SoundCloud could not complete this request. Check your connection and try again.",
        true,
    )
}
pub(crate) fn session_error() -> AppError {
    AppError::new(
        "session_expired",
        "Reconnect SoundCloud to use its account again. Your YouTube Music connection is preserved.",
        false,
    )
}
fn account_required() -> AppError {
    AppError::new(
        "account_required",
        "Connect and verify your SoundCloud account to use its library.",
        false,
    )
}
fn library_limit() -> AppError {
    AppError::new(
        "soundcloud_library_limit",
        "This SoundCloud collection exceeds the supported limit. Its existing contents are preserved.",
        false,
    )
}
fn write_error(error: AppError) -> AppError {
    if matches!(
        error.code,
        "cancelled" | "session_expired" | "account_required"
    ) {
        error
    } else {
        AppError::new(
            "soundcloud_write_unconfirmed",
            "SoundCloud did not confirm the change. Refresh and check the collection before retrying.",
            true,
        )
    }
}

#[cfg(test)]
mod tests;
