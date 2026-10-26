use std::{
    collections::BTreeMap,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};
use url::Url;

use crate::{
    error::{AppError, Result},
    runtime::RuntimePaths,
};

const MAX_METADATA: u64 = 8 * 1024 * 1024;
const MAX_DIAGNOSTICS: u64 = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFormat {
    pub id: String,
    pub codec: String,
    pub container: String,
    pub bitrate_kbps: Option<f64>,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration_seconds: Option<f64>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInspection {
    pub track: Track,
    pub formats: Vec<AudioFormat>,
    pub recommended_format_id: String,
}

// URLs and request headers stay in Rust and are deliberately not Serialize or Debug.
#[derive(Clone)]
pub(crate) struct StreamSource {
    pub url: String,
    pub headers: BTreeMap<String, String>,
    available_at: Option<f64>,
}

pub(crate) struct ResolvedTrack {
    pub inspection: TrackInspection,
    streams: BTreeMap<String, StreamSource>,
}

impl ResolvedTrack {
    fn ready_in(&self, now: f64) -> Result<Duration> {
        let wait = self
            .streams
            .values()
            .filter_map(|stream| stream.available_at)
            .fold(now, f64::max)
            - now;
        if wait > 60.0 {
            return Err(AppError::new(
                "provider_delay",
                "YouTube has not made this stream available yet. Try again later.",
                true,
            ));
        }
        Ok(Duration::from_secs_f64(if wait > 0.0 {
            wait + 0.25
        } else {
            0.0
        }))
    }

    pub fn source(
        &self,
        track_id: &str,
        format_id: &str,
    ) -> Result<(Track, AudioFormat, StreamSource)> {
        if self.inspection.track.id != track_id {
            return Err(AppError::new(
                "stale_track",
                "Choose this song again before changing its audio quality.",
                true,
            ));
        }
        let format = self
            .inspection
            .formats
            .iter()
            .find(|format| format.id == format_id);
        let source = self.streams.get(format_id);
        match (format, source) {
            (Some(format), Some(source)) => Ok((
                self.inspection.track.clone(),
                format.clone(),
                source.clone(),
            )),
            _ => Err(AppError::new(
                "format_unavailable",
                "This quality is no longer available. Play the song again to refresh its available sources.",
                true,
            )),
        }
    }
}

pub fn canonical_track_url(input: &str) -> Result<String> {
    let invalid = || {
        AppError::new(
            "invalid_url",
            "Paste a YouTube or YouTube Music track link, such as https://music.youtube.com/watch?v=…",
            false,
        )
    };
    let url = Url::parse(input.trim()).map_err(|_| invalid())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(invalid());
    }
    let id = match url.host_str() {
        Some("youtu.be") => url
            .path()
            .strip_prefix('/')
            .filter(|path| !path.contains('/'))
            .map(str::to_owned),
        Some("youtube.com" | "www.youtube.com" | "music.youtube.com" | "m.youtube.com")
            if url.path() == "/watch" =>
        {
            url.query_pairs()
                .find(|(name, _)| name == "v")
                .map(|(_, value)| value.into_owned())
        }
        _ => None,
    }
    .ok_or_else(invalid)?;
    if id.len() != 11
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(invalid());
    }
    // Strip playlist/tracking/time parameters and never allow yt-dlp's generic URL extractor.
    Ok(format!("https://www.youtube.com/watch?v={id}"))
}

pub(crate) async fn resolve(paths: &RuntimePaths, input: &str) -> Result<ResolvedTrack> {
    let canonical = canonical_track_url(input)?;
    let extractor = paths.require("yt-dlp")?;
    let deno = paths.require("deno")?;
    let mut command = Command::new(extractor);
    command
        .args([
            "--ignore-config",
            "--no-plugin-dirs",
            "--no-remote-components",
            "--no-playlist",
            "--no-progress",
            "--no-warnings",
            "--no-colors",
            "--skip-download",
            "--dump-single-json",
            "--socket-timeout",
            "15",
            "--retries",
            "1",
            "--extractor-retries",
            "1",
            "--format",
            "bestaudio",
            "--js-runtimes",
        ])
        .arg(format!("deno:{}", deno.display()))
        .arg("--")
        .arg(canonical)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command.spawn().map_err(|_| {
        AppError::new(
            "resolver_start",
            "Vessel could not open an audio source. Reinstall the app or rebuild its bundled runtime.",
            false,
        )
    })?;
    let group = ProcessGroup(child.id());
    let stdout = child.stdout.take().ok_or_else(AppError::internal)?;
    let stderr = child.stderr.take().ok_or_else(AppError::internal)?;
    timeout(Duration::from_secs(75), async {
        let mut metadata = Vec::new();
        let mut diagnostics = Vec::new();
        let mut stdout = stdout.take(MAX_METADATA + 1);
        let mut stderr = stderr.take(MAX_DIAGNOSTICS + 1);
        let (read_out, read_err, status) = tokio::join!(
            stdout.read_to_end(&mut metadata),
            stderr.read_to_end(&mut diagnostics),
            child.wait()
        );
        read_out.map_err(|_| AppError::internal())?;
        read_err.map_err(|_| AppError::internal())?;
        let status = status.map_err(|_| AppError::internal())?;
        // End the child-group lifetime before waiting for provider availability.
        drop(group);
        if metadata.len() as u64 > MAX_METADATA || diagnostics.len() as u64 > MAX_DIAGNOSTICS {
            return Err(AppError::new(
                "provider_response",
                "The provider returned more data than this player can safely process.",
                true,
            ));
        }
        if !status.success() {
            return Err(provider_error(&String::from_utf8_lossy(&diagnostics)));
        }
        let resolved = parse_metadata(&metadata)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AppError::internal())?
            .as_secs_f64();
        // Signed URLs can be returned before they become usable. Match yt-dlp's
        // downloader by honoring the provider's available_at, not a hard-coded delay.
        tokio::time::sleep(resolved.ready_in(now)?).await;
        Ok(resolved)
    })
    .await
    .map_err(|_| {
        AppError::new(
            "provider_timeout",
            "YouTube Music took too long to load this song. Check your connection and try again.",
            true,
        )
    })?
}

// Cancellation/timeouts terminate yt-dlp and its transient JavaScript runtime together.
struct ProcessGroup(Option<u32>);

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.0 {
            // SAFETY: the child was spawned in a new process group with this ID. SIGKILL
            // targets only that group and does not dereference any memory.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}

fn provider_error(diagnostics: &str) -> AppError {
    let text = diagnostics.to_lowercase();
    if text.contains("sign in")
        || text.contains("sign-in")
        || text.contains("confirm you’re not a bot")
    {
        AppError::new(
            "account_required",
            "YouTube requires authentication or additional verification for this request. Try another public track; account support is not connected yet.",
            false,
        )
    } else if text.contains("unavailable")
        || text.contains("private video")
        || text.contains("copyright")
        || text.contains("not available")
    {
        AppError::new(
            "track_unavailable",
            "This track is unavailable to guest playback in your current region or session. Try another track.",
            false,
        )
    } else if text.contains("requested format") {
        AppError::new(
            "no_audio_formats",
            "No playable audio-only format was returned for this track.",
            false,
        )
    } else {
        // Never forward raw stderr: it can contain signed URLs, headers, or local paths.
        AppError::new(
            "provider_failed",
            "YouTube could not return a playable stream. Check your connection, retry, or try another public track.",
            true,
        )
    }
}

#[derive(Deserialize)]
struct Metadata {
    id: String,
    title: String,
    artist: Option<String>,
    uploader: Option<String>,
    duration: Option<f64>,
    album: Option<String>,
    #[serde(default)]
    thumbnails: Vec<MetadataThumbnail>,
    is_live: Option<bool>,
    format_id: Option<String>,
    #[serde(default)]
    http_headers: BTreeMap<String, String>,
    #[serde(default)]
    formats: Vec<ProviderFormat>,
}

#[derive(Deserialize)]
struct MetadataThumbnail {
    url: String,
    width: Option<u32>,
}

#[derive(Deserialize)]
struct ProviderFormat {
    format_id: String,
    url: Option<String>,
    acodec: Option<String>,
    vcodec: Option<String>,
    ext: Option<String>,
    abr: Option<f64>,
    asr: Option<u32>,
    audio_channels: Option<u32>,
    has_drm: Option<bool>,
    available_at: Option<f64>,
    #[serde(default)]
    http_headers: BTreeMap<String, String>,
}

fn parse_metadata(json: &[u8]) -> Result<ResolvedTrack> {
    let metadata: Metadata = serde_json::from_slice(json).map_err(|_| {
        AppError::new(
            "provider_response",
            "The stream resolver returned an unsupported response.",
            true,
        )
    })?;
    if metadata.is_live == Some(true) {
        return Err(AppError::new(
            "live_unsupported",
            "Live streams are outside this first playback slice. Choose a recorded track.",
            false,
        ));
    }
    let mut formats = Vec::new();
    let mut streams = BTreeMap::new();
    for format in metadata.formats {
        let Some(codec) = format
            .acodec
            .filter(|codec| codec != "none" && !codec.is_empty())
        else {
            continue;
        };
        if format.vcodec.as_deref() != Some("none") || format.has_drm == Some(true) {
            continue;
        }
        let Some(url) = format
            .url
            .filter(|value| Url::parse(value).is_ok_and(|url| url.scheme() == "https"))
        else {
            continue;
        };
        let mut headers = metadata.http_headers.clone();
        headers.extend(format.http_headers);
        // Header values cross into a native media engine; reject line injection rather than normalize it.
        if headers
            .iter()
            .any(|(key, value)| key.contains(['\r', '\n', ':']) || value.contains(['\r', '\n']))
        {
            continue;
        }
        streams.insert(
            format.format_id.clone(),
            StreamSource {
                url,
                headers,
                available_at: metadata_number(format.available_at),
            },
        );
        formats.push(AudioFormat {
            id: format.format_id,
            codec,
            container: format.ext.unwrap_or_else(|| "unknown".into()),
            bitrate_kbps: metadata_number(format.abr),
            sample_rate_hz: format.asr.filter(|rate| *rate > 0),
            channels: format.audio_channels.filter(|channels| *channels > 0),
        });
    }
    let recommended_format_id = metadata
        .format_id
        .filter(|id| streams.contains_key(id))
        .or_else(|| formats.last().map(|format| format.id.clone()))
        .ok_or_else(|| {
            AppError::new(
                "no_audio_formats",
                "No playable audio-only formats were returned for this track.",
                false,
            )
        })?;
    Ok(ResolvedTrack {
        inspection: TrackInspection {
            track: Track {
                id: metadata.id,
                title: metadata.title,
                artist: metadata
                    .artist
                    .or(metadata.uploader)
                    .unwrap_or_else(|| "Unknown artist".into()),
                duration_seconds: metadata_number(metadata.duration),
                album: metadata.album,
                thumbnail_url: metadata
                    .thumbnails
                    .iter()
                    .rev()
                    .filter(|thumbnail| thumbnail.width.unwrap_or(0) <= 512)
                    .find_map(|thumbnail| artwork_url(&thumbnail.url)),
            },
            formats,
            recommended_format_id,
        },
        streams,
    })
}

pub(crate) fn artwork_url(value: &str) -> Option<String> {
    if value.len() > 2048 {
        return None;
    }
    let url = Url::parse(value).ok()?;
    let host = url.host_str()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && (host.ends_with(".googleusercontent.com") || host.ends_with(".ytimg.com")))
    .then(|| value.to_owned())
}

fn metadata_number(value: Option<f64>) -> Option<f64> {
    value.filter(|number| number.is_finite() && *number > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_supported_links_and_removes_playlist_context() {
        for input in [
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=private",
            "https://youtu.be/dQw4w9WgXcQ?t=12",
        ] {
            assert_eq!(
                canonical_track_url(input).unwrap(),
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
            );
        }
    }

    #[test]
    fn rejects_non_provider_urls_credentials_and_malformed_ids() {
        for input in [
            "file:///etc/passwd",
            "https://youtube.com.evil.test/watch?v=dQw4w9WgXcQ",
            "https://user@youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=short",
            "http://youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ/extra",
        ] {
            assert_eq!(canonical_track_url(input).unwrap_err().code, "invalid_url");
        }
    }

    #[test]
    fn exposes_only_audio_formats_and_preserves_unknown_quality_metadata() {
        let track = parse_metadata(br#"{
          "id":"test", "title":"Fixture", "uploader":"Artist", "format_id":"opus",
          "formats":[
            {"format_id":"video","url":"https://example.com/video","acodec":"aac","vcodec":"h264"},
            {"format_id":"drm","url":"https://example.com/drm","acodec":"aac","vcodec":"none","has_drm":true},
            {"format_id":"local","url":"file:///audio","acodec":"aac","vcodec":"none"},
            {"format_id":"aac","url":"https://example.com/aac","acodec":"mp4a.40.2","vcodec":"none","ext":"m4a","abr":0},
            {"format_id":"opus","url":"https://example.com/opus","acodec":"opus","vcodec":"none","ext":"webm","abr":129.4}
          ]}"#).unwrap();
        assert_eq!(track.inspection.formats.len(), 2);
        assert_eq!(track.inspection.formats[0].bitrate_kbps, None);
        assert_eq!(track.inspection.recommended_format_id, "opus");
        let public_json = serde_json::to_string(&track.inspection).unwrap();
        assert!(!public_json.contains("https://"));
        assert!(!public_json.contains("http_headers"));
        assert!(matches!(track.source("other", "opus"), Err(error) if error.code == "stale_track"));
    }

    #[test]
    fn provider_errors_never_forward_signed_urls() {
        let error = provider_error("HTTP 403 https://example.com?signature=secret");
        assert!(!error.message.contains("secret"));
        assert!(!error.message.contains("example.com"));
    }

    #[test]
    fn respects_provider_stream_availability_instead_of_opening_too_early() {
        let resolved = parse_metadata(br#"{
          "id":"test", "title":"Fixture", "formats":[
            {"format_id":"opus","url":"https://example.com/audio","acodec":"opus","vcodec":"none","available_at":1006}
          ]}"#).unwrap();
        assert_eq!(
            resolved.ready_in(1000.0).unwrap(),
            Duration::from_millis(6250)
        );
        assert_eq!(resolved.ready_in(1010.0).unwrap(), Duration::ZERO);
        assert_eq!(resolved.ready_in(900.0).unwrap_err().code, "provider_delay");
    }
}
