use crate::{
    catalog::{parse_duration, valid_video_id},
    error::{AppError, Result},
    provider::{Track, artwork_url},
};
use serde_json::Value;

#[derive(Clone)]
pub(crate) struct RadioCursor {
    pub seed_id: String,
    pub playlist_id: String,
    pub token: String,
    pub account_revision: u64,
}
pub(crate) struct RadioBatch {
    pub tracks: Vec<Track>,
    pub cursor: Option<RadioCursor>,
}

pub(crate) fn parse_radio(data: &Value, seed_id: &str, revision: u64) -> Result<RadioBatch> {
    let mut panel = data.pointer("/continuationContents/playlistPanelContinuation");
    if panel.is_none() {
        panel = find_panel(data.get("contents").unwrap_or(&Value::Null));
    }
    let panel = panel.ok_or_else(radio_error)?;
    let mut tracks = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for item in panel
        .get("contents")
        .and_then(Value::as_array)
        .ok_or_else(radio_error)?
    {
        let row = item.get("playlistPanelVideoRenderer").or_else(|| {
            item.pointer(
                "/playlistPanelVideoWrapperRenderer/primaryRenderer/playlistPanelVideoRenderer",
            )
        });
        if let Some(row) = row
            && row.get("unplayableText").is_none()
            && let Some(track) = parse_watch_track(row)
            && ids.insert(track.id.clone())
        {
            tracks.push(track);
        }
        if tracks.len() >= 60 {
            break;
        }
    }
    if tracks.is_empty() {
        return Err(radio_error());
    }
    let token = panel
        .pointer("/continuations/0/nextRadioContinuationData/continuation")
        .or_else(|| panel.pointer("/continuations/0/nextContinuationData/continuation"))
        .and_then(Value::as_str)
        .filter(|token| token.len() <= 8192);
    let playlist = panel
        .get("playlistId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let cursor = token.map(|token| RadioCursor {
        seed_id: seed_id.into(),
        playlist_id: if playlist.is_empty() {
            format!("RDAMVM{seed_id}")
        } else {
            playlist.into()
        },
        token: token.into(),
        account_revision: revision,
    });
    Ok(RadioBatch { tracks, cursor })
}

fn find_panel(value: &Value) -> Option<&Value> {
    if let Some(panel) = value.get("playlistPanelRenderer") {
        return Some(panel);
    }
    match value {
        Value::Object(map) => map.values().find_map(find_panel),
        Value::Array(items) => items.iter().find_map(find_panel),
        _ => None,
    }
}

fn text(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
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

fn parse_watch_track(row: &Value) -> Option<Track> {
    let id = row.get("videoId")?.as_str()?;
    if !valid_video_id(id) {
        return None;
    }
    let title = text(row.get("title"));
    if title.is_empty() {
        return None;
    }
    let runs = row
        .pointer("/longBylineText/runs")
        .or_else(|| row.pointer("/shortBylineText/runs"))
        .and_then(Value::as_array);
    let mut artists = Vec::new();
    let mut album = None;
    if let Some(runs) = runs {
        for run in runs {
            let Some(name) = run.get("text").and_then(Value::as_str) else {
                continue;
            };
            let kind = run.pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType").and_then(Value::as_str);
            match kind {
                Some("MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL") => {
                    artists.push(name.to_owned())
                }
                Some("MUSIC_PAGE_TYPE_ALBUM") => album = Some(name.into()),
                _ => {}
            }
        }
        if artists.is_empty()
            && let Some(name) = runs
                .first()
                .and_then(|run| run.get("text"))
                .and_then(Value::as_str)
        {
            artists.push(name.into());
        }
    }
    let thumbnail_url = row
        .pointer("/thumbnail/thumbnails")
        .and_then(Value::as_array)
        .and_then(|images| {
            images
                .iter()
                .rev()
                .filter(|image| image.get("width").and_then(Value::as_u64).unwrap_or(0) <= 512)
                .find_map(|image| artwork_url(image.get("url")?.as_str()?))
        });
    Some(Track {
        id: id.into(),
        title,
        artist: artists.join(" & "),
        album,
        thumbnail_url,
        duration_seconds: parse_duration(&text(row.get("lengthText"))),
    })
}

pub(crate) fn radio_error() -> AppError {
    AppError::new(
        "radio_unavailable",
        "YouTube Music could not generate more songs. Retry radio or choose another seed song.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_primary_radio_items_and_skips_unavailable_and_duplicate_versions() {
        let row = serde_json::json!({"videoId":"khnokW3Mw24","title":{"runs":[{"text":"Song"}]},"lengthText":{"runs":[{"text":"3:10"}]}});
        let mut unavailable = row.clone();
        unavailable["videoId"] = serde_json::json!("d7woYjGWs6k");
        unavailable["unplayableText"] = serde_json::json!({"runs":[{"text":"Unavailable"}]});
        let data = serde_json::json!({"contents":{"playlistPanelRenderer":{"contents":[{"playlistPanelVideoRenderer":row},{"playlistPanelVideoRenderer":unavailable}],"continuations":[{"nextRadioContinuationData":{"continuation":"opaque-cursor"}}]}}});
        let batch = parse_radio(&data, "khnokW3Mw24", 0).unwrap();
        assert_eq!(batch.tracks.len(), 1);
        assert_eq!(batch.tracks[0].duration_seconds, Some(190.0));
        assert_eq!(batch.cursor.unwrap().token, "opaque-cursor");
    }
}
