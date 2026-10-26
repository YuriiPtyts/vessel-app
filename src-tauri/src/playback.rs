use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use serde::Serialize;
use tokio::sync::oneshot;

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
    pub status: PlaybackStatus,
    pub track: Option<Track>,
    pub active_format: Option<AudioFormat>,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub volume: f64,
    pub error: Option<AppError>,
    pub video_track_selected: bool,
    pub decoded_sample_rate_hz: Option<f64>,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Idle,
            track: None,
            active_format: None,
            position_seconds: 0.0,
            duration_seconds: None,
            volume: 40.0,
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
    Stop,
    Shutdown,
}

struct LoadRequest {
    track: Track,
    format: AudioFormat,
    source: StreamSource,
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
    ) -> Result<PlaybackSnapshot> {
        self.request(Operation::Load(Box::new(LoadRequest {
            track,
            format,
            source,
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
        if let Some(engine) = engine.as_ref() {
            refresh(engine, &mut snapshot);
        }
        match request {
            Ok(Request {
                operation: Operation::Shutdown,
                ..
            }) => break,
            Ok(request) => {
                let result = apply(request.operation, &paths, &mut engine, &mut snapshot);
                if let Err(error) = &result {
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
        if snapshot != previous {
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
            } = *request;
            if engine.is_none() {
                *engine = Some(Mpv::open(paths)?);
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
            if let Some(engine) = engine {
                engine.set("volume", &volume.to_string())?;
            }
            snapshot.volume = volume;
        }
        Operation::Stop => {
            if let Some(engine) = engine.as_ref() {
                engine.run(&["stop"])?;
            }
            let volume = snapshot.volume;
            *snapshot = PlaybackSnapshot {
                volume,
                ..PlaybackSnapshot::default()
            };
        }
        Operation::Shutdown => {}
    }
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
