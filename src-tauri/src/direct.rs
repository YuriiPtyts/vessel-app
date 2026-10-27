//! Small guest-only fast path. Unsupported/ciphered sources fall back to the bundled resolver.
use crate::{
    error::{AppError, Result},
    provider::{ResolvedTrack, parse_metadata},
};
use reqwest::{Client, Response};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// Protocol constants checked against the pinned yt-dlp 2026.10.21 client profile.
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
const ORIGIN: &str = "https://www.youtube.com";
const CONSENT: &str = "SOCS=CAI; PREF=hl=en&tz=UTC";

struct Visitor {
    value: String,
    created: Instant,
}
pub(crate) struct DirectGuest {
    client: Client,
    visitor: Mutex<Option<Visitor>>,
}

impl DirectGuest {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            visitor: Mutex::new(None),
        }
    }

    pub async fn warm_up(&self) {
        let _ = self.visitor().await;
    }

    async fn visitor(&self) -> Result<String> {
        let mut cached = self.visitor.lock().await;
        if let Some(visitor) = cached
            .as_ref()
            .filter(|visitor| visitor.created.elapsed() < Duration::from_secs(900))
        {
            return Ok(visitor.value.clone());
        }
        let response = self
            .client
            .get(format!("{ORIGIN}/"))
            .header("User-Agent", USER_AGENT)
            .header("Cookie", CONSENT)
            .send()
            .await
            .map_err(|_| unavailable())?;
        let html = body(response).await?;
        let html = std::str::from_utf8(&html).map_err(|_| unavailable())?;
        let value = visitor_data(html)?;
        *cached = Some(Visitor {
            value: value.clone(),
            created: Instant::now(),
        });
        Ok(value)
    }

    pub async fn resolve(&self, id: &str) -> Result<ResolvedTrack> {
        let visitor = self.visitor().await?;
        let mut visitor_header =
            reqwest::header::HeaderValue::from_str(&visitor).map_err(|_| unavailable())?;
        visitor_header.set_sensitive(true);
        let response = self
            .client
            .post(format!("{ORIGIN}/youtubei/v1/player?prettyPrint=false"))
            .header("User-Agent", USER_AGENT)
            .header("Origin", ORIGIN)
            .header("Cookie", CONSENT)
            .header("X-YouTube-Client-Name", "101")
            .header("X-YouTube-Client-Version", "1.02")
            .header("X-Goog-Visitor-Id", visitor_header)
            .json(&json!({
                "context": {"client": {
                    "clientName": "VISIONOS", "clientVersion": "1.02",
                    "deviceMake": "Apple", "deviceModel": "RealityDevice17,1",
                    "userAgent": USER_AGENT, "osName": "visionOS", "osVersion": "26.5.23O471",
                    "hl": "en", "timeZone": "UTC", "utcOffsetMinutes": 0,
                    "visitorData": visitor
                }},
                "videoId": id, "contentCheckOk": true, "racyCheckOk": true,
                "playbackContext": {"contentPlaybackContext": {"html5Preference": "HTML5_PREF_WANTS"}}
            }))
            .send()
            .await
            .map_err(|_| unavailable())?;
        let response: Value =
            serde_json::from_slice(&body(response).await?).map_err(|_| unavailable())?;
        parse_response(&response, id)
    }
}

fn parse_response(data: &Value, id: &str) -> Result<ResolvedTrack> {
    if data
        .pointer("/playabilityStatus/status")
        .and_then(Value::as_str)
        != Some("OK")
        || data
            .pointer("/videoDetails/videoId")
            .and_then(Value::as_str)
            != Some(id)
        || data
            .pointer("/videoDetails/isLiveContent")
            .and_then(Value::as_bool)
            == Some(true)
        || data.pointer("/streamingData/licenseInfos").is_some()
    {
        return Err(unavailable());
    }
    let formats: Vec<_> = data
        .pointer("/streamingData/adaptiveFormats")
        .and_then(Value::as_array)
        .ok_or_else(unavailable)?
        .iter()
        .filter_map(parse_format)
        .collect();
    if formats.is_empty() {
        return Err(unavailable());
    }
    let preferred = formats
        .iter()
        .find(|format| format["format_id"] == "251")
        .or_else(|| formats.iter().find(|format| format["format_id"] == "140"))
        .unwrap_or(&formats[0])["format_id"]
        .clone();
    let details = &data["videoDetails"];
    let metadata = json!({
        "id": id,
        "title": details.get("title").and_then(Value::as_str).unwrap_or("Unknown song"),
        "artist": details.get("author").and_then(Value::as_str),
        "duration": details.get("lengthSeconds").and_then(Value::as_str).and_then(|duration| duration.parse::<f64>().ok()),
        "thumbnails": details.pointer("/thumbnail/thumbnails").filter(|value| value.is_array()).cloned().unwrap_or_else(|| json!([])),
        "format_id": preferred,
        "http_headers": {"User-Agent": USER_AGENT, "Referer": format!("{ORIGIN}/")},
        "formats": formats
    });
    parse_metadata(&serde_json::to_vec(&metadata).map_err(|_| unavailable())?)
}

fn parse_format(format: &Value) -> Option<Value> {
    let mime = format.get("mimeType")?.as_str()?;
    let container = if mime.starts_with("audio/webm;") {
        "webm"
    } else if mime.starts_with("audio/mp4;") {
        "m4a"
    } else {
        return None;
    };
    if format.get("signatureCipher").is_some()
        || format.get("cipher").is_some()
        || format.get("drmFamilies").is_some()
    {
        return None;
    }
    let codec = mime.split_once("codecs=\"")?.1.split('"').next()?;
    if codec.is_empty() || codec.contains(',') {
        return None;
    }
    let source = format.get("url")?.as_str()?;
    let url = url::Url::parse(source).ok()?;
    if url.scheme() != "https"
        || !url.host_str()?.ends_with(".googlevideo.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query_pairs().any(|(key, _)| key == "n")
    {
        return None;
    }
    Some(json!({
        "format_id": format.get("itag")?.as_u64()?.to_string(),
        "url": source, "acodec": codec, "vcodec": "none", "ext": container,
        "abr": format.get("averageBitrate").or_else(|| format.get("bitrate")).and_then(Value::as_f64).map(|bits| bits / 1000.0),
        "asr": format.get("audioSampleRate").and_then(Value::as_str).and_then(|rate| rate.parse::<u32>().ok()),
        "audio_channels": format.get("audioChannels").and_then(Value::as_u64)
    }))
}

fn visitor_data(html: &str) -> Result<String> {
    let (_, value) = html
        .split_once("\"VISITOR_DATA\":")
        .ok_or_else(unavailable)?;
    let value = serde_json::Deserializer::from_str(value)
        .into_iter::<String>()
        .next()
        .and_then(std::result::Result::ok)
        .ok_or_else(unavailable)?;
    if value.is_empty() || value.len() > 4096 {
        return Err(unavailable());
    }
    Ok(value)
}

async fn body(mut response: Response) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn unavailable() -> AppError {
    AppError::new(
        "direct_source_unavailable",
        "This song needs the compatibility resolver.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_matching_recorded_audio_with_direct_sources_reaches_the_player() {
        let id = "khnokW3Mw24";
        let mut response = json!({"playabilityStatus":{"status":"OK"},"videoDetails":{"videoId":id,"title":"Fixture","author":"Artist","lengthSeconds":"100"},"streamingData":{"adaptiveFormats":[
            {"itag":251,"mimeType":"audio/webm; codecs=\"opus\"","url":"https://fixture.googlevideo.com/audio?expire=9999999999","bitrate":128000,"audioSampleRate":"48000","audioChannels":2},
            {"itag":140,"mimeType":"audio/mp4; codecs=\"mp4a.40.2\"","signatureCipher":"private-cipher"},
            {"itag":137,"mimeType":"video/mp4; codecs=\"avc1\"","url":"https://fixture.googlevideo.com/video"}
        ]}});
        let track = parse_response(&response, id).unwrap();
        assert_eq!(track.inspection.formats.len(), 1);
        assert_eq!(track.inspection.formats[0].bitrate_kbps, Some(128.0));
        assert!(
            !serde_json::to_string(&track.inspection)
                .unwrap()
                .contains("googlevideo")
        );
        assert!(parse_response(&response, "d7woYjGWs6k").is_err());
        let valid = response.clone();
        for source in [
            "https://fixture.googlevideo.com/audio?n=requires-transform",
            "https://fixture.googlevideo.com.example.org/audio",
            "https://user:password@fixture.googlevideo.com/audio",
            "http://fixture.googlevideo.com/audio",
        ] {
            response = valid.clone();
            response["streamingData"]["adaptiveFormats"][0]["url"] = json!(source);
            assert!(parse_response(&response, id).is_err());
        }
        response = valid.clone();
        response["videoDetails"]["isLiveContent"] = json!(true);
        assert!(parse_response(&response, id).is_err());
        response = valid.clone();
        response["streamingData"]["licenseInfos"] = json!([{}]);
        assert!(parse_response(&response, id).is_err());
        response = valid;
        response["playabilityStatus"]["status"] = json!("LOGIN_REQUIRED");
        assert!(parse_response(&response, id).is_err());
    }
}
