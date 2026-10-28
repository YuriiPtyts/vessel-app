//! Account metadata and ratings are separate from the durable guest collection.
//! No credentials, continuation tokens, or private browse parameters cross IPC.
use crate::{
    account::{Account, AccountMode},
    catalog::{MusicCatalog, parse_track, valid_video_id, walk},
    error::{AppError, Result},
    home::{CollectionPage, CollectionTarget, HomeItem, HomeItemKind},
    provider::Track,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, watch};

#[path = "playlist_writes.rs"]
mod playlist_writes;
pub use playlist_writes::{PlaylistMutation, PlaylistPrivacy};

const MAX_LIKES: usize = 10_000;
const MAX_PLAYLISTS: usize = 1_000;
const MAX_PAGES: usize = 200;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountLibrarySnapshot {
    pub revision: u64,
    pub account_revision: u64,
    pub connected: bool,
    pub refreshing: bool,
    pub likes_ready: bool,
    pub playlists_ready: bool,
    pub liked: Vec<Track>,
    pub liked_ids: Vec<String>,
    pub playlists: Vec<HomeItem>,
    pub editable_playlist_ids: Vec<String>,
    pub likes_error: Option<AppError>,
    pub playlists_error: Option<AppError>,
}

impl AccountLibrarySnapshot {
    fn empty(account_revision: u64, revision: u64, connected: bool) -> Self {
        Self {
            revision,
            account_revision,
            connected,
            refreshing: false,
            likes_ready: false,
            playlists_ready: false,
            liked: Vec::new(),
            liked_ids: Vec::new(),
            playlists: Vec::new(),
            editable_playlist_ids: Vec::new(),
            likes_error: None,
            playlists_error: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BrowseCursor {
    pub token: String,
    pub modern: bool,
}

struct State {
    public: AccountLibrarySnapshot,
    targets: BTreeMap<String, CollectionTarget>,
    refreshed: Option<Instant>,
    collection: Option<SavedCollection>,
}

struct SavedCollection {
    page: CollectionPage,
    cursor: Option<BrowseCursor>,
    seen: BTreeSet<String>,
}

pub(crate) struct AccountLibrary {
    account: Arc<Account>,
    state: Mutex<State>,
    work: AsyncMutex<()>,
    collection_work: AsyncMutex<()>,
    cancellation: watch::Sender<u64>,
    emit: Arc<dyn Fn(AccountLibrarySnapshot) + Send + Sync>,
}

impl AccountLibrary {
    pub fn new(
        account: Arc<Account>,
        emit: Arc<dyn Fn(AccountLibrarySnapshot) + Send + Sync>,
    ) -> Self {
        let (cancellation, _) = watch::channel(0);
        Self {
            account,
            state: Mutex::new(State {
                public: AccountLibrarySnapshot::empty(0, 0, false),
                targets: BTreeMap::new(),
                refreshed: None,
                collection: None,
            }),
            work: AsyncMutex::new(()),
            collection_work: AsyncMutex::new(()),
            cancellation,
            emit,
        }
    }

    pub fn cancel(&self) {
        self.cancellation
            .send_modify(|epoch| *epoch = epoch.wrapping_add(1));
    }

    pub fn synchronize_account(&self) -> Result<()> {
        let changed = {
            let mut state = self.state.lock().map_err(|_| AppError::internal())?;
            let account = self.account.snapshot();
            let (revision, session) = self.account.request_session();
            if revision != account.revision {
                return Err(AppError::cancelled());
            }
            let connected = account.mode == AccountMode::Connected && session.is_some();
            if state.public.account_revision == account.revision
                && state.public.connected == connected
            {
                None
            } else {
                self.cancel();
                state.public = AccountLibrarySnapshot::empty(
                    account.revision,
                    state.public.revision + 1,
                    connected,
                );
                state.targets.clear();
                state.refreshed = None;
                state.collection = None;
                Some(state.public.clone())
            }
        };
        if let Some(snapshot) = changed {
            (self.emit)(snapshot);
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Result<AccountLibrarySnapshot> {
        self.synchronize_account()?;
        self.state
            .lock()
            .map(|state| state.public.clone())
            .map_err(|_| AppError::internal())
    }

    pub fn check_revision(&self, revision: u64) -> Result<()> {
        let account = self.account.snapshot();
        if account.revision != revision {
            return Err(AppError::cancelled());
        }
        let (current, session) = self.account.request_session();
        if current != revision {
            return Err(AppError::cancelled());
        }
        if account.mode != AccountMode::Connected || session.is_none() {
            return Err(account_required());
        }
        Ok(())
    }

    pub fn target(&self, id: &str, revision: u64) -> Result<CollectionTarget> {
        self.synchronize_account()?;
        self.check_revision(revision)?;
        self.state
            .lock()
            .map_err(|_| AppError::internal())?
            .targets
            .get(id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    "stale_account_playlist",
                    "Refresh your YouTube Music playlists and open this playlist again.",
                    true,
                )
            })
    }

    pub fn find_track(&self, id: &str) -> Option<Track> {
        self.synchronize_account().ok()?;
        let state = self.state.lock().ok()?;
        state
            .public
            .liked
            .iter()
            .chain(
                state
                    .collection
                    .iter()
                    .flat_map(|collection| &collection.page.tracks),
            )
            .find(|track| track.id == id)
            .cloned()
    }

    pub async fn refresh(
        &self,
        catalog: &MusicCatalog,
        force: bool,
        revision: u64,
    ) -> Result<AccountLibrarySnapshot> {
        self.synchronize_account()?;
        self.check_revision(revision)?;
        let mut cancel = self.cancellation.subscribe();
        let _work = tokio::select! { lock = self.work.lock() => lock, _ = cancel.changed() => return Err(AppError::cancelled()) };
        self.check_revision(revision)?;
        {
            let state = self.state.lock().map_err(|_| AppError::internal())?;
            if !force
                && state
                    .refreshed
                    .is_some_and(|time| time.elapsed() < Duration::from_secs(60))
            {
                return Ok(state.public.clone());
            }
        }
        let loading = {
            let mut state = self.state.lock().map_err(|_| AppError::internal())?;
            if state.public.account_revision != revision {
                return Err(AppError::cancelled());
            }
            state.public.refreshing = true;
            state.public.likes_error = None;
            state.public.playlists_error = None;
            state.public.revision += 1;
            state.public.clone()
        };
        (self.emit)(loading);
        let operation = async {
            let (likes, playlists) = tokio::join!(
                fetch_likes(catalog, revision),
                fetch_playlists(catalog, revision)
            );
            self.check_revision(revision)?;
            let snapshot = {
                let mut state = self.state.lock().map_err(|_| AppError::internal())?;
                if state.public.account_revision != revision {
                    return Err(AppError::cancelled());
                }
                match likes {
                    Ok((tracks, ids)) => {
                        state.public.liked = tracks;
                        state.public.liked_ids = ids.into_iter().collect();
                        state.public.likes_ready = true;
                    }
                    Err(error) => state.public.likes_error = Some(error),
                }
                match playlists {
                    Ok((items, targets, editable)) => {
                        if state.collection.as_ref().is_some_and(|collection| {
                            !targets.contains_key(&collection.page.item.id)
                        }) {
                            state.collection = None;
                        }
                        state.public.playlists = items;
                        state.public.editable_playlist_ids = editable.into_iter().collect();
                        state.targets = targets;
                        state.public.playlists_ready = true;
                    }
                    Err(error) => state.public.playlists_error = Some(error),
                }
                if state.public.likes_error.is_none() && state.public.playlists_error.is_none() {
                    state.refreshed = Some(Instant::now());
                }
                state.public.refreshing = false;
                state.public.revision += 1;
                state.public.clone()
            };
            (self.emit)(snapshot.clone());
            Ok(snapshot)
        };
        tokio::select! { result = operation => result, _ = cancel.changed() => Err(AppError::cancelled()) }
    }

    pub async fn collection(
        &self,
        catalog: &MusicCatalog,
        id: &str,
        more: bool,
        revision: u64,
    ) -> Result<CollectionPage> {
        let target = self.target(id, revision)?;
        let mut cancel = self.cancellation.subscribe();
        let _work = tokio::select! { lock = self.collection_work.lock() => lock, _ = cancel.changed() => return Err(AppError::cancelled()) };
        self.check_revision(revision)?;
        let (previous, cursor, mut seen) = {
            let state = self.state.lock().map_err(|_| AppError::internal())?;
            if more {
                let cached = state
                    .collection
                    .as_ref()
                    .filter(|cached| cached.page.item.id == id)
                    .ok_or_else(|| {
                        AppError::new(
                            "stale_account_playlist",
                            "Open this playlist again to load more songs.",
                            true,
                        )
                    })?;
                if cached.cursor.is_none() {
                    return Ok(cached.page.clone());
                }
                (
                    cached.page.tracks.clone(),
                    cached.cursor.clone(),
                    cached.seen.clone(),
                )
            } else {
                (Vec::new(), None, BTreeSet::new())
            }
        };
        let data = tokio::select! {
            result = catalog.account_collection_browse(id, target.params.as_deref(), cursor.as_ref(), revision) => result?,
            _ = cancel.changed() => return Err(AppError::cancelled()),
        };
        let (mut page, cursor) = parse_saved_collection(&data, &target)?;
        let mut tracks = previous;
        let mut ids: BTreeSet<_> = tracks.iter().map(|track| track.id.clone()).collect();
        tracks.extend(
            page.tracks
                .into_iter()
                .filter(|track| ids.insert(track.id.clone())),
        );
        if tracks.len() > MAX_LIKES || seen.len() >= MAX_PAGES {
            return Err(library_limit());
        }
        if let Some(cursor) = &cursor
            && !seen.insert(cursor.token.clone())
        {
            return Err(library_error());
        }
        page.tracks = tracks;
        self.check_revision(revision)?;
        let mut state = self.state.lock().map_err(|_| AppError::internal())?;
        if state.public.account_revision != revision || !state.targets.contains_key(id) {
            return Err(AppError::cancelled());
        }
        state.collection = Some(SavedCollection {
            page: page.clone(),
            cursor,
            seen,
        });
        Ok(page)
    }

    pub async fn set_like(
        &self,
        catalog: &MusicCatalog,
        track: Track,
        liked: bool,
        revision: u64,
    ) -> Result<AccountLibrarySnapshot> {
        self.synchronize_account()?;
        self.check_revision(revision)?;
        let mut cancel = self.cancellation.subscribe();
        let _work = tokio::select! { lock = self.work.lock() => lock, _ = cancel.changed() => return Err(AppError::cancelled()) };
        self.check_revision(revision)?;
        {
            let state = self.state.lock().map_err(|_| AppError::internal())?;
            if !state.public.likes_ready {
                return Err(AppError::new(
                    "account_library_loading",
                    "Wait for your YouTube Music likes to finish syncing, then try again.",
                    true,
                ));
            }
            if state.public.liked_ids.contains(&track.id) == liked {
                return Ok(state.public.clone());
            }
            if liked && state.public.liked_ids.len() >= MAX_LIKES {
                return Err(library_limit());
            }
        }
        tokio::select! { result = catalog.rate_account_song(&track.id, liked, revision) => result?, _ = cancel.changed() => return Err(AppError::cancelled()) }
        self.check_revision(revision)?;
        let snapshot = {
            let mut state = self.state.lock().map_err(|_| AppError::internal())?;
            if state.public.account_revision != revision {
                return Err(AppError::cancelled());
            }
            state.public.liked.retain(|item| item.id != track.id);
            state.public.liked_ids.retain(|id| *id != track.id);
            if liked {
                state.public.liked_ids.push(track.id.clone());
                state.public.liked.insert(0, track);
            }
            state.public.likes_error = None;
            state.public.revision += 1;
            state.public.clone()
        };
        (self.emit)(snapshot.clone());
        Ok(snapshot)
    }
}

async fn fetch_likes(
    catalog: &MusicCatalog,
    revision: u64,
) -> Result<(Vec<Track>, BTreeSet<String>)> {
    let operation = async {
        let mut tracks = Vec::new();
        let mut ids = BTreeSet::new();
        let mut track_ids = BTreeSet::new();
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        for _ in 0..MAX_PAGES {
            let data = catalog
                .account_browse("VLLM", cursor.as_ref(), revision)
                .await?;
            let page = parse_liked_page(&data)?;
            ids.extend(page.ids);
            for track in page.tracks {
                if track_ids.insert(track.id.clone()) {
                    tracks.push(track);
                }
            }
            if ids.len() > MAX_LIKES {
                return Err(library_limit());
            }
            cursor = page.cursor;
            let Some(next) = &cursor else {
                return Ok((tracks, ids));
            };
            if !seen.insert(next.token.clone()) {
                return Err(library_error());
            }
        }
        Err(library_limit())
    };
    tokio::time::timeout(Duration::from_secs(120), operation)
        .await
        .map_err(|_| library_error())?
}

async fn fetch_playlists(
    catalog: &MusicCatalog,
    revision: u64,
) -> Result<(
    Vec<HomeItem>,
    BTreeMap<String, CollectionTarget>,
    BTreeSet<String>,
)> {
    let operation = async {
        let mut items = Vec::new();
        let mut targets = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut editable = BTreeSet::new();
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        for _ in 0..MAX_PAGES {
            let data = catalog
                .account_browse("FEmusic_liked_playlists", cursor.as_ref(), revision)
                .await?;
            let page = parse_playlists_page(&data)?;
            editable.extend(page.editable);
            for target in page.targets {
                if ids.insert(target.item.id.clone()) {
                    items.push(target.item.clone());
                    targets.insert(target.item.id.clone(), target);
                }
            }
            if items.len() > MAX_PLAYLISTS {
                return Err(library_limit());
            }
            cursor = page.cursor;
            let Some(next) = &cursor else {
                return Ok((items, targets, editable));
            };
            if !seen.insert(next.token.clone()) {
                return Err(library_error());
            }
        }
        Err(library_limit())
    };
    tokio::time::timeout(Duration::from_secs(120), operation)
        .await
        .map_err(|_| library_error())?
}

pub(crate) struct LikedPage {
    tracks: Vec<Track>,
    ids: BTreeSet<String>,
    cursor: Option<BrowseCursor>,
}
struct PlaylistsPage {
    targets: Vec<CollectionTarget>,
    editable: BTreeSet<String>,
    cursor: Option<BrowseCursor>,
}

fn page_root(data: &Value, playlists: bool) -> Result<&Value> {
    let contents = data
        .get("continuationContents")
        .or_else(|| data.get("onResponseReceivedActions"))
        .or_else(|| data.get("contents"))
        .ok_or_else(library_error)?;
    let mut root = None;
    walk(contents, &mut |value| {
        if root.is_none() {
            root = if playlists {
                value
                    .get("gridRenderer")
                    .or_else(|| value.get("gridContinuation"))
                    .or_else(|| value.get("musicShelfRenderer"))
            } else {
                value
                    .get("musicPlaylistShelfRenderer")
                    .or_else(|| value.get("musicPlaylistShelfContinuation"))
                    .or_else(|| value.get("musicShelfContinuation"))
                    .or_else(|| value.get("musicShelfRenderer"))
            };
            root = root.filter(|root| {
                root.get("contents")
                    .or_else(|| root.get("items"))
                    .is_some_and(Value::is_array)
            });
            if root.is_none() {
                root = value
                    .get("appendContinuationItemsAction")
                    .filter(|action| action.get("continuationItems").is_some_and(Value::is_array));
            }
        }
    });
    root.ok_or_else(library_error)
}

fn cursor_from(root: &Value) -> Result<Option<BrowseCursor>> {
    let mut cursor = None;
    let mut invalid = false;
    walk(root, &mut |value| {
        if let Some(token) = value.pointer("/nextContinuationData/continuation") {
            match token
                .as_str()
                .filter(|token| !token.is_empty() && token.len() <= 8192)
            {
                Some(token) => {
                    cursor = Some(BrowseCursor {
                        token: token.into(),
                        modern: false,
                    })
                }
                None => invalid = true,
            }
        }
        if let Some(command) = value.get("continuationCommand")
            && command
                .get("request")
                .and_then(Value::as_str)
                .is_none_or(|request| request == "CONTINUATION_REQUEST_TYPE_BROWSE")
        {
            match command
                .get("token")
                .and_then(Value::as_str)
                .filter(|token| !token.is_empty() && token.len() <= 8192)
            {
                Some(token) => {
                    cursor = Some(BrowseCursor {
                        token: token.into(),
                        modern: true,
                    })
                }
                None => invalid = true,
            }
        }
    });
    if invalid {
        Err(library_error())
    } else {
        Ok(cursor)
    }
}

pub(crate) fn parse_liked_page(data: &Value) -> Result<LikedPage> {
    let root = page_root(data, false)?;
    let mut tracks = Vec::new();
    let mut ids = BTreeSet::new();
    let mut parsed = BTreeSet::new();
    walk(root, &mut |value| {
        if let Some(row) = value.get("musicResponsiveListItemRenderer") {
            if let Some(id) = row
                .pointer("/playlistItemData/videoId")
                .and_then(Value::as_str)
                .filter(|id| valid_video_id(id))
            {
                ids.insert(id.into());
            }
            if let Some(mut track) = parse_track(row, None) {
                ids.insert(track.id.clone());
                if parsed.insert(track.id.clone()) {
                    track.title = track.title.chars().take(512).collect();
                    track.artist = track.artist.chars().take(512).collect();
                    tracks.push(track);
                }
            }
        }
    });
    Ok(LikedPage {
        tracks,
        ids,
        cursor: cursor_from(root)?,
    })
}

fn parse_playlists_page(data: &Value) -> Result<PlaylistsPage> {
    let root = page_root(data, true)?;
    let mut targets = BTreeMap::new();
    let mut order = Vec::new();
    let mut ids = BTreeSet::new();
    let mut editable = BTreeSet::new();
    walk(root, &mut |value| {
        if let Some(row) = value.get("musicTwoRowItemRenderer") {
            let item = crate::home::parse_card(row, &mut targets);
            if let Some(item) = item.filter(|item| {
                item.kind == HomeItemKind::Playlist
                    && item.id.starts_with("VL")
                    && item.id != "VLLM"
            }) && ids.insert(item.id.clone())
            {
                walk(row.get("menu").unwrap_or(&Value::Null), &mut |value| {
                    if value
                        .pointer("/playlistEditorEndpoint/playlistId")
                        .and_then(Value::as_str)
                        == item.id.strip_prefix("VL")
                    {
                        editable.insert(item.id.clone());
                    }
                });
                order.push(item.id);
            }
        }
    });
    Ok(PlaylistsPage {
        editable,
        targets: order
            .into_iter()
            .filter_map(|id| targets.remove(&id))
            .collect(),
        cursor: cursor_from(root)?,
    })
}

pub(crate) fn parse_saved_collection(
    data: &Value,
    target: &CollectionTarget,
) -> Result<(CollectionPage, Option<BrowseCursor>)> {
    let page = parse_liked_page(data).or_else(|error| {
        // Empty owned playlists may have a shelf with no `contents` key.
        // Unknown payloads still fail instead of erasing a collection.
        let mut empty_shelf = false;
        walk(data.get("contents").unwrap_or(&Value::Null), &mut |value| {
            if let Some(shelf) = value
                .get("musicPlaylistShelfRenderer")
                .and_then(Value::as_object)
            {
                empty_shelf |=
                    !shelf.contains_key("contents") && !shelf.contains_key("continuations");
            }
        });
        if empty_shelf {
            Ok(LikedPage {
                tracks: Vec::new(),
                ids: BTreeSet::new(),
                cursor: None,
            })
        } else {
            Err(error)
        }
    })?;
    Ok((
        CollectionPage {
            item: target.item.clone(),
            tracks: page.tracks,
            has_more: page.cursor.is_some(),
        },
        page.cursor,
    ))
}

pub(crate) fn validate_rating_response(data: &Value) -> Result<()> {
    let mut gated = false;
    walk(data, &mut |value| {
        gated |= value.get("showEngagementPanelEndpoint").is_some()
            || value.get("confirmDialogRenderer").is_some()
            || value.get("signinPromoRenderer").is_some();
    });
    let status = data
        .pointer("/likeResponse/status")
        .or_else(|| data.get("status"))
        .and_then(Value::as_str);
    if gated
        || data.get("error").is_some()
        || !data.get("responseContext").is_some_and(Value::is_object)
        || status.is_some_and(|status| !matches!(status, "STATUS_SUCCEEDED" | "SUCCESS"))
    {
        return Err(AppError::new(
            "account_like_failed",
            "YouTube Music did not confirm this like change. Refresh your account library and try again.",
            true,
        ));
    }
    Ok(())
}

pub(crate) fn library_error() -> AppError {
    AppError::new(
        "account_library_unavailable",
        "Your YouTube Music library could not sync. Check your connection and refresh the account library.",
        true,
    )
}
pub(crate) fn account_required() -> AppError {
    AppError::new(
        "account_required",
        "Connect and verify your Google account to use its YouTube Music library.",
        false,
    )
}
fn library_limit() -> AppError {
    AppError::new(
        "account_library_limit",
        "This beta supports up to 10,000 account likes and 1,000 saved playlists. Your local collection is preserved.",
        false,
    )
}

#[cfg(test)]
#[path = "account_library_tests.rs"]
mod tests;
