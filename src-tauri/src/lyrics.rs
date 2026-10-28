//! On-demand provider lyrics. Private browse identifiers never leave Rust.
use crate::{
    catalog::walk,
    error::{AppError, Result},
};
use serde::Serialize;
use serde_json::Value;
use std::time::Instant;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongLyrics {
    pub video_id: String,
    pub text: Option<String>,
    pub source: Option<String>,
}

pub(crate) struct CachedLyrics {
    pub lyrics: SongLyrics,
    pub revision: u64,
    pub created: Instant,
}

pub(crate) fn lyrics_target(data: &Value) -> Result<Option<String>> {
    let tabs = data.pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs")
        .and_then(Value::as_array).ok_or_else(lyrics_error)?;
    for tab in tabs {
        let Some(tab) = tab.get("tabRenderer") else {
            continue;
        };
        if tab.get("unselectable").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if let Some(id) = tab
            .pointer("/endpoint/browseEndpoint/browseId")
            .and_then(Value::as_str)
            .filter(|id| {
                id.starts_with("MPLY")
                    && id.len() <= 512
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
        {
            return Ok(Some(id.into()));
        }
    }
    Ok(None)
}

fn text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("simpleText").and_then(Value::as_str) {
        return Some(text.into());
    }
    let runs = value.get("runs")?.as_array()?;
    Some(
        runs.iter()
            .filter_map(|run| run.get("text").and_then(Value::as_str))
            .collect(),
    )
}

pub(crate) fn parse_lyrics(id: &str, data: &Value) -> Result<SongLyrics> {
    let mut lyrics = None;
    let mut source = None;
    let mut recognized = false;
    walk(data.get("contents").unwrap_or(&Value::Null), &mut |value| {
        if let Some(shelf) = value.get("musicDescriptionShelfRenderer") {
            recognized = true;
            if lyrics.is_none() {
                lyrics = shelf.get("description").and_then(text);
                source = shelf.get("footer").and_then(text);
            }
        }
        if let Some(lines) = value.get("timedLyricsData").and_then(Value::as_array) {
            recognized = true;
            if lyrics.is_none() {
                lyrics = Some(
                    lines
                        .iter()
                        .filter_map(|line| line.get("lyricLine").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                source = value
                    .get("sourceMessage")
                    .and_then(|value| value.as_str().map(String::from).or_else(|| text(value)));
            }
        }
    });
    if !recognized
        || lyrics
            .as_ref()
            .is_some_and(|text| text.len() > 65_536 || text.contains('\0'))
    {
        return Err(lyrics_error());
    }
    let lyrics = lyrics
        .map(|text| text.replace("\r\n", "\n").trim().to_owned())
        .filter(|text| !text.is_empty());
    Ok(SongLyrics {
        video_id: id.into(),
        text: lyrics,
        source: source.map(|text| text.chars().take(512).collect()),
    })
}

pub(crate) fn lyrics_error() -> AppError {
    AppError::new(
        "lyrics_unavailable",
        "Lyrics could not load from YouTube Music. Check your connection and try again.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_or_disabled_lyrics_are_distinct_from_a_changed_response() {
        let tabs = json!({"contents":{"singleColumnMusicWatchNextResultsRenderer":{"tabbedRenderer":{"watchNextTabbedResultsRenderer":{"tabs":[{"tabRenderer":{"unselectable":true,"endpoint":{"browseEndpoint":{"browseId":"MPLYdisabled"}}}}]}}}}});
        assert_eq!(lyrics_target(&tabs).unwrap(), None);
        assert!(lyrics_target(&json!({})).is_err());
        assert!(parse_lyrics("aaaaaaaaaaa", &json!({"contents":{}})).is_err());
    }

    #[test]
    fn timed_lines_and_attribution_are_preserved_without_exposing_browse_data() {
        let data = json!({"contents":{"timedLyricsModel":{"lyricsData":{"timedLyricsData":[{"lyricLine":"First fixture line"},{"lyricLine":"Другий рядок"}],"sourceMessage":"Fixture source"}}}});
        let lyrics = parse_lyrics("aaaaaaaaaaa", &data).unwrap();
        assert_eq!(
            lyrics.text.as_deref(),
            Some("First fixture line\nДругий рядок")
        );
        assert_eq!(lyrics.source.as_deref(), Some("Fixture source"));
        let huge = json!({"contents":{"musicDescriptionShelfRenderer":{"description":{"simpleText":"x".repeat(65_537)}}}});
        assert!(parse_lyrics("aaaaaaaaaaa", &huge).is_err());
    }

    #[tokio::test]
    #[ignore = "queries real guest song search and available provider lyrics; no credentials or audio"]
    async fn live_song_lyrics_load_from_youtube_music() {
        let catalog = crate::catalog::MusicCatalog::new().unwrap();
        let songs = catalog
            .search("Coldplay Yellow", crate::catalog::SearchKind::Songs, false)
            .await
            .unwrap();
        assert!(!songs.tracks.is_empty());
        for song in songs.tracks.iter().take(3) {
            let lyrics = catalog.lyrics(&song.id).await.unwrap();
            if let Some(text) = lyrics.text {
                assert!(!text.trim().is_empty());
                println!(
                    "Live provider lyrics available: {} lines; attribution: {}",
                    text.lines().count(),
                    lyrics.source.is_some()
                );
                return;
            }
        }
        panic!("No lyrics supplied for the first three song results in this live run");
    }
}
