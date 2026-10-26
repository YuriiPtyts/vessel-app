//! Normalize provider-selected Home shelves, without editorial artist/query fallbacks.
use crate::{
    catalog::{parse_track, valid_video_id, walk},
    error::{AppError, Result},
    provider::{Track, artwork_url},
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum HomeItemKind {
    Song,
    Video,
    Album,
    Playlist,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeItem {
    pub id: String,
    pub kind: HomeItemKind,
    pub title: String,
    pub subtitle: String,
    pub thumbnail_url: Option<String>,
    pub track: Option<Track>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeShelf {
    pub id: String,
    pub title: String,
    pub items: Vec<HomeItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeFeed {
    pub shelves: Vec<HomeShelf>,
    pub personalized: bool,
}

#[derive(Clone)]
pub(crate) struct CollectionTarget {
    pub item: HomeItem,
    pub params: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPage {
    pub item: HomeItem,
    pub tracks: Vec<Track>,
    pub has_more: bool,
}

pub(crate) fn parse_home(data: &Value) -> Result<(HomeFeed, BTreeMap<String, CollectionTarget>)> {
    let contents = data.get("contents").ok_or_else(home_error)?;
    let mut shelves = Vec::new();
    let mut targets = BTreeMap::new();
    walk(contents, &mut |value| {
        let Some(shelf) = value.get("musicCarouselShelfRenderer") else {
            return;
        };
        if shelves.len() >= 8 {
            return;
        }
        let title = text(shelf.pointer("/header/musicCarouselShelfBasicHeaderRenderer/title"));
        let Some(items) = shelf.get("contents").and_then(Value::as_array) else {
            return;
        };
        let mut normalized = Vec::new();
        let mut ids = BTreeSet::new();
        for entry in items {
            let item = if let Some(row) = entry.get("musicResponsiveListItemRenderer") {
                parse_track(row, None).map(from_track)
            } else if let Some(row) = entry.get("musicTwoRowItemRenderer") {
                parse_card(row, &mut targets)
            } else {
                None
            };
            if let Some(item) = item
                && ids.insert(item.id.clone())
            {
                normalized.push(item);
            }
            if normalized.len() >= 12 {
                break;
            }
        }
        if !title.is_empty() && !normalized.is_empty() {
            shelves.push(HomeShelf {
                id: format!("shelf-{}", shelves.len()),
                title,
                items: normalized,
            });
        }
    });
    Ok((
        HomeFeed {
            shelves,
            personalized: false,
        },
        targets,
    ))
}

fn from_track(track: Track) -> HomeItem {
    HomeItem {
        id: track.id.clone(),
        kind: HomeItemKind::Song,
        title: track.title.clone(),
        subtitle: track.artist.clone(),
        thumbnail_url: track.thumbnail_url.clone(),
        track: Some(track),
    }
}

fn parse_card(row: &Value, targets: &mut BTreeMap<String, CollectionTarget>) -> Option<HomeItem> {
    let title = text(row.get("title"));
    if title.is_empty() {
        return None;
    }
    let subtitle = text(row.get("subtitle"));
    let thumbnail_url =
        thumbnail(row.pointer("/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails"));
    let endpoint = row
        .get("navigationEndpoint")
        .or_else(|| row.pointer("/title/runs/0/navigationEndpoint"))?;
    if let Some(watch) = endpoint.get("watchEndpoint") {
        let id = watch.get("videoId")?.as_str()?;
        if !valid_video_id(id) {
            return None;
        }
        let kind = if watch
            .pointer("/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType")
            .and_then(Value::as_str)
            == Some("MUSIC_VIDEO_TYPE_ATV")
        {
            HomeItemKind::Song
        } else {
            HomeItemKind::Video
        };
        let track = Track {
            id: id.into(),
            title: title.clone(),
            artist: subtitle.clone(),
            album: None,
            duration_seconds: None,
            thumbnail_url: thumbnail_url.clone(),
        };
        return Some(HomeItem {
            id: id.into(),
            kind,
            title,
            subtitle,
            thumbnail_url,
            track: Some(track),
        });
    }
    let browse = endpoint.get("browseEndpoint")?;
    let id = browse.get("browseId")?.as_str()?;
    if id.len() > 256
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
    {
        return None;
    }
    let page_type = browse
        .pointer("/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
        .and_then(Value::as_str);
    let kind = match page_type {
        Some("MUSIC_PAGE_TYPE_ALBUM") => HomeItemKind::Album,
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => HomeItemKind::Playlist,
        _ => return None,
    };
    let item = HomeItem {
        id: id.into(),
        kind,
        title,
        subtitle,
        thumbnail_url,
        track: None,
    };
    let params = browse
        .get("params")
        .and_then(Value::as_str)
        .filter(|params| params.len() <= 4096)
        .map(str::to_owned);
    targets.insert(
        id.into(),
        CollectionTarget {
            item: item.clone(),
            params,
        },
    );
    Some(item)
}

pub(crate) fn parse_collection(
    data: &Value,
    target: &CollectionTarget,
) -> Result<(CollectionPage, Option<String>)> {
    let contents = data
        .get("continuationContents")
        .or_else(|| data.get("onResponseReceivedActions"))
        .or_else(|| data.get("contents"))
        .ok_or_else(home_error)?;
    let mut tracks = Vec::new();
    let mut continuation = None;
    let mut item = target.item.clone();
    let mut ids = BTreeSet::new();
    walk(contents, &mut |value| {
        if let Some(row) = value.get("musicResponsiveListItemRenderer")
            && let Some(track) = parse_track(row, None)
            && ids.insert(track.id.clone())
            && tracks.len() < 100
        {
            tracks.push(track);
        }
        if let Some(header) = value
            .get("musicResponsiveHeaderRenderer")
            .or_else(|| value.get("musicDetailHeaderRenderer"))
        {
            let title = text(header.get("title"));
            if !title.is_empty() {
                item.title = title;
            }
        }
        if let Some(token) = value
            .pointer("/nextContinuationData/continuation")
            .or_else(|| value.pointer("/continuationCommand/token"))
            .and_then(Value::as_str)
            .filter(|token| token.len() <= 8192)
        {
            continuation = Some(token.to_owned());
        }
    });
    Ok((
        CollectionPage {
            item,
            tracks,
            has_more: continuation.is_some(),
        },
        continuation,
    ))
}

fn thumbnail(value: Option<&Value>) -> Option<String> {
    let thumbnails = value?.as_array()?;
    thumbnails
        .iter()
        .rev()
        .filter(|entry| entry.get("width").and_then(Value::as_u64).unwrap_or(0) <= 512)
        .find_map(|entry| artwork_url(entry.get("url")?.as_str()?))
        .or_else(|| {
            thumbnails
                .first()
                .and_then(|entry| artwork_url(entry.get("url")?.as_str()?))
        })
}

fn text(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Some(text) = value.get("simpleText").and_then(Value::as_str) {
        return text.chars().take(512).collect();
    }
    value
        .get("runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|run| run.get("text")?.as_str())
        .collect::<String>()
        .chars()
        .take(512)
        .collect()
}

fn home_error() -> AppError {
    AppError::new(
        "home_unavailable",
        "YouTube Music could not load these recommendations. Try refreshing Listen now.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn home_preserves_provider_titles_and_collection_targets() {
        let data = json!({"contents":{"musicCarouselShelfRenderer":{
            "header":{"musicCarouselShelfBasicHeaderRenderer":{"title":{"runs":[{"text":"Provider's actual picks"}]}}},
            "contents":[{"musicTwoRowItemRenderer":{
                "title":{"runs":[{"text":"A provider playlist"}]},"subtitle":{"runs":[{"text":"Selected by YouTube Music"}]},
                "navigationEndpoint":{"browseEndpoint":{"browseId":"VLplaylist_id","params":"opaque-provider-params","browseEndpointContextSupportedConfigs":{"browseEndpointContextMusicConfig":{"pageType":"MUSIC_PAGE_TYPE_PLAYLIST"}}}}
            }}]
        }}});
        let (feed, targets) = parse_home(&data).unwrap();
        assert_eq!(feed.shelves[0].title, "Provider's actual picks");
        assert_eq!(feed.shelves[0].items[0].title, "A provider playlist");
        assert!(!feed.personalized);
        assert_eq!(
            targets["VLplaylist_id"].params.as_deref(),
            Some("opaque-provider-params")
        );
        assert!(
            !serde_json::to_string(&feed)
                .unwrap()
                .contains("opaque-provider-params")
        );
    }
}
