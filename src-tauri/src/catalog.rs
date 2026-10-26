//! Guest YouTube Music catalog. Volatile response parsing is isolated from playback.
use std::{
    collections::{BTreeSet, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, watch};

use crate::{
    error::{AppError, Result},
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
}
struct CachedSearch {
    results: SearchResults,
    continuation: Option<String>,
    created: Instant,
}

pub struct MusicCatalog {
    client: Client,
    session: Mutex<Option<Session>>,
    cache: Mutex<Option<CachedSearch>>,
    known_tracks: Mutex<VecDeque<Track>>,
    slot: Semaphore,
    cancellation: watch::Sender<u64>,
}

impl MusicCatalog {
    pub fn new() -> Result<Self> {
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
            session: Mutex::new(None),
            cache: Mutex::new(None),
            known_tracks: Mutex::new(VecDeque::new()),
            slot: Semaphore::new(1),
            cancellation,
        })
    }

    pub fn cancel(&self) {
        self.cancellation
            .send_modify(|revision| *revision = revision.wrapping_add(1));
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
            let matching = cache
                .as_ref()
                .filter(|cache| cache.results.query == query && cache.results.kind == kind);
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
        let context = self.context().await?;
        let version = context
            .pointer("/client/clientVersion")
            .and_then(Value::as_str)
            .ok_or_else(catalog_error)?;
        let body = json!({ "context": context, "query": query, "params": kind.params() });
        let mut endpoint =
            url::Url::parse(&format!("{ORIGIN}/youtubei/v1/search?prettyPrint=false"))
                .map_err(|_| catalog_error())?;
        if let Some(continuation) = &continuation {
            endpoint
                .query_pairs_mut()
                .append_pair("ctoken", continuation)
                .append_pair("continuation", continuation);
        }
        let response = self
            .client
            .post(endpoint)
            .header("Origin", ORIGIN)
            .header("Referer", format!("{ORIGIN}/"))
            .header("X-YouTube-Client-Name", "67")
            .header("X-YouTube-Client-Version", version)
            .json(&body)
            .send()
            .await
            .map_err(|_| catalog_error())?;
        let data: Value = serde_json::from_slice(&bounded_body(response, 4 * 1024 * 1024).await?)
            .map_err(|_| catalog_error())?;
        if data.get("error").is_some() {
            return Err(catalog_error());
        }
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
        {
            let mut known = self.known_tracks.lock().map_err(|_| AppError::internal())?;
            for track in &results.tracks {
                known.retain(|previous| previous.id != track.id);
                known.push_front(track.clone());
            }
            known.truncate(128);
        }
        *self.cache.lock().map_err(|_| AppError::internal())? = Some(CachedSearch {
            results: results.clone(),
            continuation,
            created: Instant::now(),
        });
        Ok(results)
    }

    async fn context(&self) -> Result<Value> {
        if let Some(session) = self
            .session
            .lock()
            .map_err(|_| AppError::internal())?
            .as_ref()
            .filter(|session| session.created.elapsed() < Duration::from_secs(15 * 60))
        {
            return Ok(session.context.clone());
        }
        let response = self
            .client
            .get(format!("{ORIGIN}/"))
            .send()
            .await
            .map_err(|_| catalog_error())?;
        let html = bounded_body(response, 4 * 1024 * 1024).await?;
        let html = std::str::from_utf8(&html).map_err(|_| catalog_error())?;
        let context = parse_context(html)?;
        *self.session.lock().map_err(|_| AppError::internal())? = Some(Session {
            context: context.clone(),
            created: Instant::now(),
        });
        Ok(context)
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
            && let Some(track) = parse_track(item, kind)
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

fn parse_track(item: &Value, kind: SearchKind) -> Option<Track> {
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
    if kind == SearchKind::Songs && video_type.is_some_and(|kind| kind != "MUSIC_VIDEO_TYPE_ATV") {
        return None;
    }
    if kind == SearchKind::Videos && video_type == Some("MUSIC_VIDEO_TYPE_ATV") {
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

fn walk(value: &Value, visit: &mut impl FnMut(&Value)) {
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

fn parse_duration(text: &str) -> Option<f64> {
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
        "YouTube Music search is unavailable right now. Check your connection and try again.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
