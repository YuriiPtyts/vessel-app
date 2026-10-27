use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Serialize;
use tokio::sync::{oneshot, watch};

use crate::{
    error::{AppError, Result},
    mpv::Mpv,
    provider::{AudioFormat, StreamSource, Track},
    runtime::RuntimePaths,
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaybackStatus {
    Idle,
    Loading,
    Playing,
    Paused,
    Buffering,
    Ended,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSnapshot {
    pub playback_id: u64,
    pub account_source: bool,
    pub status: PlaybackStatus,
    pub track: Option<Track>,
    pub active_format: Option<AudioFormat>,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub volume: f64,
    pub volume_revision: u64,
    pub error: Option<AppError>,
    pub video_track_selected: bool,
    pub decoded_sample_rate_hz: Option<f64>,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            playback_id: 0,
            account_source: false,
            status: PlaybackStatus::Idle,
            track: None,
            active_format: None,
            position_seconds: 0.0,
            duration_seconds: None,
            volume: 40.0,
            volume_revision: 0,
            error: None,
            video_track_selected: false,
            decoded_sample_rate_hz: None,
        }
    }
}

enum Operation {
    WarmUp,
    Load(Box<LoadRequest>),
    Pause(bool),
    Seek(f64),
    Volume(f64),
    ToggleMute,
    Stop,
    Shutdown,
}

struct LoadRequest {
    track: Track,
    format: AudioFormat,
    source: StreamSource,
    cancellation: watch::Receiver<u64>,
    generation: u64,
}

struct Request {
    operation: Operation,
    reply: oneshot::Sender<Result<PlaybackSnapshot>>,
}

pub(crate) struct Player {
    sender: SyncSender<Request>,
    snapshot: Arc<Mutex<PlaybackSnapshot>>,
    stopping: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Player {
    pub fn new(paths: RuntimePaths, emit: Arc<dyn Fn(PlaybackSnapshot) + Send + Sync>) -> Self {
        let (sender, receiver) = mpsc::sync_channel(32);
        let snapshot = Arc::new(Mutex::new(PlaybackSnapshot::default()));
        let shared = snapshot.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let shutdown = stopping.clone();
        let thread = thread::spawn(move || worker(paths, receiver, shared, shutdown, emit));
        Self {
            sender,
            snapshot,
            stopping,
            thread: Mutex::new(Some(thread)),
        }
    }

    pub fn snapshot(&self) -> Result<PlaybackSnapshot> {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| AppError::internal())
    }

    async fn request(&self, operation: Operation) -> Result<PlaybackSnapshot> {
        let (reply, result) = oneshot::channel();
        self.sender
            .try_send(Request { operation, reply })
            .map_err(|_| {
                AppError::new(
                    "player_busy",
                    "The player is busy. Try again in a moment.",
                    true,
                )
            })?;
        result.await.map_err(|_| AppError::internal())?
    }

    pub async fn load(
        &self,
        track: Track,
        format: AudioFormat,
        source: StreamSource,
        cancellation: watch::Receiver<u64>,
        generation: u64,
    ) -> Result<PlaybackSnapshot> {
        self.request(Operation::Load(Box::new(LoadRequest {
            track,
            format,
            source,
            cancellation,
            generation,
        })))
        .await
    }

    pub async fn pause(&self, paused: bool) -> Result<PlaybackSnapshot> {
        self.request(Operation::Pause(paused)).await
    }

    pub async fn warm_up(&self) -> Result<PlaybackSnapshot> {
        self.request(Operation::WarmUp).await
    }
    pub async fn seek(&self, position: f64) -> Result<PlaybackSnapshot> {
        if !position.is_finite() || position < 0.0 {
            return Err(AppError::new(
                "invalid_seek",
                "Choose a valid position in the current track.",
                false,
            ));
        }
        self.request(Operation::Seek(position)).await
    }
    pub async fn volume(&self, volume: f64) -> Result<PlaybackSnapshot> {
        if !volume.is_finite() || !(0.0..=100.0).contains(&volume) {
            return Err(AppError::new(
                "invalid_volume",
                "Volume must be between 0 and 100.",
                false,
            ));
        }
        self.request(Operation::Volume(volume)).await
    }
    pub async fn toggle_mute(&self) -> Result<PlaybackSnapshot> {
        self.request(Operation::ToggleMute).await
    }
    pub async fn stop(&self) -> Result<PlaybackSnapshot> {
        self.request(Operation::Stop).await
    }

    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::Relaxed);
        let (reply, _) = oneshot::channel();
        let _ = self.sender.try_send(Request {
            operation: Operation::Shutdown,
            reply,
        });
        if let Ok(mut handle) = self.thread.lock()
            && let Some(handle) = handle.take()
        {
            let _ = handle.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker(
    paths: RuntimePaths,
    receiver: Receiver<Request>,
    shared: Arc<Mutex<PlaybackSnapshot>>,
    stopping: Arc<AtomicBool>,
    emit: Arc<dyn Fn(PlaybackSnapshot) + Send + Sync>,
) {
    let mut engine: Option<Mpv> = None;
    let mut snapshot = PlaybackSnapshot::default();
    let mut next_playback_id = 0_u64;
    let mut last_audible_volume = snapshot.volume;
    let mut last_refresh = Instant::now();
    while !stopping.load(Ordering::Relaxed) {
        let active = matches!(
            snapshot.status,
            PlaybackStatus::Loading | PlaybackStatus::Playing | PlaybackStatus::Buffering
        );
        let request = if engine.is_none() || snapshot.track.is_none() {
            receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            receiver.recv_timeout(Duration::from_millis(if active { 250 } else { 1000 }))
        };
        let previous = snapshot.clone();
        let volume_only = request.as_ref().is_ok_and(|request| {
            matches!(
                &request.operation,
                Operation::Volume(_) | Operation::ToggleMute
            )
        });
        let poll = !volume_only || last_refresh.elapsed() >= Duration::from_millis(250);
        if poll && let Some(engine) = engine.as_ref() {
            refresh(engine, &mut snapshot);
            last_refresh = Instant::now();
        }
        match request {
            Ok(Request {
                operation: Operation::Shutdown,
                ..
            }) => break,
            Ok(request) => {
                let result = apply(
                    request.operation,
                    &paths,
                    &mut engine,
                    &mut snapshot,
                    &mut next_playback_id,
                    &mut last_audible_volume,
                );
                if let Err(error) = &result
                    && error.code != "cancelled"
                {
                    snapshot.error = Some(error.clone());
                    if snapshot.status == PlaybackStatus::Loading {
                        snapshot.status = PlaybackStatus::Failed;
                    }
                }
                if let Ok(mut shared) = shared.lock() {
                    *shared = snapshot.clone();
                }
                let _ = request.reply.send(result.map(|()| snapshot.clone()));
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        // Volume acknowledgements already carry the new gain/revision to the caller.
        // Keep position/EOF publication at its normal cadence during a slider sweep.
        if snapshot != previous && (!volume_only || poll) {
            if let Ok(mut shared) = shared.lock() {
                *shared = snapshot.clone();
            }
            emit(snapshot.clone());
        }
    }
    // The handle is destroyed here on the same thread that created and used it.
}

fn apply(
    operation: Operation,
    paths: &RuntimePaths,
    engine: &mut Option<Mpv>,
    snapshot: &mut PlaybackSnapshot,
    next_playback_id: &mut u64,
    last_audible_volume: &mut f64,
) -> Result<()> {
    match operation {
        Operation::WarmUp => {
            if engine.is_none() {
                *engine = Some(Mpv::open(paths)?);
            }
        }
        Operation::Load(request) => {
            let LoadRequest {
                track,
                format,
                source,
                cancellation,
                generation,
            } = *request;
            if *cancellation.borrow() != generation {
                return Err(AppError::cancelled());
            }
            if engine.is_none() {
                *engine = Some(Mpv::open(paths)?);
            }
            if *cancellation.borrow() != generation {
                return Err(AppError::cancelled());
            }
            let same_track = snapshot
                .track
                .as_ref()
                .is_some_and(|current| current.id == track.id)
                && !matches!(
                    snapshot.status,
                    PlaybackStatus::Ended | PlaybackStatus::Failed | PlaybackStatus::Idle
                );
            let position = if same_track {
                snapshot.position_seconds
            } else {
                0.0
            };
            let paused = same_track && snapshot.status == PlaybackStatus::Paused;
            if !same_track {
                *next_playback_id = next_playback_id.wrapping_add(1);
                snapshot.playback_id = *next_playback_id;
            }
            snapshot.status = PlaybackStatus::Loading;
            snapshot.error = None;
            engine.as_ref().ok_or_else(AppError::internal)?.load(
                &source,
                position,
                paused,
                snapshot.volume,
            )?;
            snapshot.duration_seconds = track.duration_seconds;
            snapshot.track = Some(track);
            snapshot.active_format = Some(format);
            snapshot.account_source = source.account_source;
            snapshot.position_seconds = position;
            snapshot.decoded_sample_rate_hz = None;
        }
        Operation::Pause(paused) => {
            require_track(snapshot)?;
            engine
                .as_ref()
                .ok_or_else(AppError::internal)?
                .set("pause", if paused { "yes" } else { "no" })?;
            snapshot.status = if paused {
                PlaybackStatus::Paused
            } else {
                PlaybackStatus::Playing
            };
        }
        Operation::Seek(position) => {
            require_track(snapshot)?;
            let position = snapshot
                .duration_seconds
                .map_or(position, |duration| position.min(duration));
            engine.as_ref().ok_or_else(AppError::internal)?.run(&[
                "seek",
                &position.to_string(),
                "absolute+exact",
            ])?;
            snapshot.position_seconds = position;
        }
        Operation::Volume(volume) => {
            apply_volume(engine.as_ref(), snapshot, last_audible_volume, volume)?;
        }
        Operation::ToggleMute => {
            let volume = if snapshot.volume > 0.0 {
                0.0
            } else {
                *last_audible_volume
            };
            apply_volume(engine.as_ref(), snapshot, last_audible_volume, volume)?;
        }
        Operation::Stop => {
            if let Some(engine) = engine.as_ref() {
                engine.run(&["stop"])?;
                engine.set("http-header-fields", "")?;
            }
            let volume = snapshot.volume;
            let volume_revision = snapshot.volume_revision;
            *snapshot = PlaybackSnapshot {
                volume,
                volume_revision,
                ..PlaybackSnapshot::default()
            };
        }
        Operation::Shutdown => {}
    }
    Ok(())
}

fn apply_volume(
    engine: Option<&Mpv>,
    snapshot: &mut PlaybackSnapshot,
    last_audible_volume: &mut f64,
    volume: f64,
) -> Result<()> {
    if let Some(engine) = engine {
        engine.set_volume(volume)?;
    }
    if volume > 0.0 {
        *last_audible_volume = volume;
    }
    snapshot.volume = volume;
    snapshot.volume_revision = snapshot.volume_revision.wrapping_add(1);
    Ok(())
}

fn require_track(snapshot: &PlaybackSnapshot) -> Result<()> {
    if snapshot.track.is_none()
        || matches!(
            snapshot.status,
            PlaybackStatus::Idle | PlaybackStatus::Ended | PlaybackStatus::Failed
        )
    {
        return Err(AppError::new(
            "no_active_track",
            "Play a track before using this control.",
            false,
        ));
    }
    Ok(())
}

fn refresh(engine: &Mpv, snapshot: &mut PlaybackSnapshot) {
    let events = engine.events();
    if events.failed && snapshot.track.is_some() {
        snapshot.status = PlaybackStatus::Failed;
        snapshot.error = Some(AppError::new(
            "stream_failed",
            "This song could not continue. Play it again to refresh its source, or choose a different quality.",
            true,
        ));
        return;
    }
    if events.ended && snapshot.track.is_some() {
        snapshot.status = PlaybackStatus::Ended;
        if let Some(duration) = snapshot.duration_seconds {
            snapshot.position_seconds = duration;
        }
        return;
    }
    if snapshot.track.is_none()
        || matches!(
            snapshot.status,
            PlaybackStatus::Failed | PlaybackStatus::Ended
        )
    {
        return;
    }
    if events.loaded || snapshot.status != PlaybackStatus::Loading {
        snapshot.status = if engine.flag("pause") == Some(true) {
            PlaybackStatus::Paused
        } else if engine.flag("paused-for-cache") == Some(true) {
            PlaybackStatus::Buffering
        } else {
            PlaybackStatus::Playing
        };
    }
    if let Some(position) = engine.number("time-pos") {
        snapshot.position_seconds = position.max(0.0);
    }
    if let Some(duration) = engine.number("duration").filter(|duration| *duration > 0.0) {
        snapshot.duration_seconds = Some(duration);
    }
    snapshot.video_track_selected = engine.number("vid").is_some_and(|id| id > 0.0);
    snapshot.decoded_sample_rate_hz = engine.number("audio-params/samplerate");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::atomic::AtomicUsize, time::Instant};
    use tokio::sync::watch;

    #[tokio::test]
    async fn mute_restores_the_last_nonzero_volume_without_starting_audio() {
        let player = Player::new(crate::test_audio::paths(), Arc::new(|_| {}));
        let original = player.volume(73.0).await.unwrap();
        let muted = player.toggle_mute().await.unwrap();
        assert_eq!(muted.volume, 0.0);
        assert!(muted.volume_revision > original.volume_revision);
        player.stop().await.unwrap();
        assert_eq!(player.toggle_mute().await.unwrap().volume, 73.0);
        player.volume(0.0).await.unwrap();
        assert_eq!(player.toggle_mute().await.unwrap().volume, 73.0);
        player.shutdown();
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    #[ignore = "requires the output-silencing CoreAudio render audit; run scripts/check-native-volume.mjs"]
    async fn native_output_gain_follows_volume_without_queued_audio_delay() {
        let audit = crate::test_audio::RenderAudit::new();
        let player = Player::new(crate::test_audio::paths(), Arc::new(|_| {}));
        let audio = crate::test_audio::render_probe_tone();
        let (track, mut format, source) = crate::test_audio::selection(&audio);
        format.sample_rate_hz = Some(44_100);
        format.channels = Some(2);
        let (_cancel, receiver) = watch::channel(0);
        // Stay at zero until the audit has actually wrapped the native render callback.
        player.volume(0.0).await.unwrap();
        player
            .load(track, format, source, receiver, 0)
            .await
            .unwrap();
        let start = Instant::now();
        while audit.blocks() < 5 {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "Native render audit did not attach; refusing to unmute the fixture"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        player.volume(100.0).await.unwrap();
        let start = Instant::now();
        while audit.peak() < 0.1 {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "No rendered tone after unmuting"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let mut timings = Vec::new();
        for volume in [0.0, 100.0, 0.0, 100.0] {
            let start = Instant::now();
            player.volume(volume).await.unwrap();
            loop {
                let observed = audit.peak();
                if (volume == 0.0 && observed == 0.0) || (volume > 0.0 && observed > 0.1) {
                    break;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(3),
                    "Rendered gain did not settle: {observed}"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            println!("Rendered volume {volume}: {:?}", start.elapsed());
            timings.push(start.elapsed());
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        println!(
            "Native render/queue quantum: {} frames; backend {}",
            audit.frames(),
            std::env::var("VESSEL_VOLUME_PROBE_BACKEND").unwrap_or_else(|_| "default".into())
        );
        player.stop().await.unwrap();
        player.shutdown();
        assert!(
            timings.iter().all(|time| *time < Duration::from_millis(50)),
            "Gain changes are delayed by queued audio: {timings:?}"
        );
    }

    #[tokio::test]
    #[ignore = "real libmpv volume latency during local silent audio; no network"]
    async fn native_volume_stays_responsive_without_flooding_playback_events() {
        let published = Arc::new(AtomicUsize::new(0));
        let marker = published.clone();
        let player = Player::new(
            crate::test_audio::paths(),
            Arc::new(move |_| {
                marker.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let audio = crate::test_audio::silence();
        let (track, format, source) = crate::test_audio::selection(&audio);
        let (_cancel, receiver) = watch::channel(0);
        player.volume(0.0).await.unwrap();
        player
            .load(track, format, source, receiver, 0)
            .await
            .unwrap();
        let start = Instant::now();
        while player.snapshot().unwrap().status != PlaybackStatus::Playing {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "{:?}",
                player.snapshot().unwrap()
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        published.store(0, Ordering::Relaxed);
        let mut timings = Vec::new();
        let sweep = Instant::now();
        for step in 0..100 {
            let start = Instant::now();
            player.volume(f64::from(step)).await.unwrap();
            timings.push(start.elapsed());
        }
        timings.sort();
        let updates = published.load(Ordering::Relaxed);
        println!(
            "Native volume: p95 {:?}; max {:?}; {updates} playback events in {:?}",
            timings[94],
            timings[99],
            sweep.elapsed()
        );
        player.stop().await.unwrap();
        player.shutdown();
        assert!(timings[94] < Duration::from_millis(16));
        assert!(
            updates < 10,
            "slider input must not flood full playback-state events: {updates}"
        );
    }
}
