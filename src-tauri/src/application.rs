use std::sync::{Arc, Mutex};

use tokio::sync::{Semaphore, watch};

use crate::{
    error::{AppError, Result},
    playback::{PlaybackSnapshot, Player},
    provider::{self, ResolvedTrack, TrackInspection},
    runtime::RuntimePaths,
};

pub struct Application {
    paths: RuntimePaths,
    player: Player,
    resolved: Mutex<Option<ResolvedTrack>>,
    resolver_slot: Semaphore,
    cancellation: watch::Sender<u64>,
}

impl Application {
    pub fn new(
        paths: RuntimePaths,
        emit: impl Fn(PlaybackSnapshot) + Send + Sync + 'static,
    ) -> Self {
        let (cancellation, _) = watch::channel(0);
        Self {
            player: Player::new(paths.clone(), Arc::new(emit)),
            paths,
            resolved: Mutex::new(None),
            resolver_slot: Semaphore::new(1),
            cancellation,
        }
    }

    pub fn runtime_available(&self) -> bool {
        self.paths.available()
    }

    pub async fn inspect(&self, url: &str) -> Result<TrackInspection> {
        let _slot = self.resolver_slot.try_acquire().map_err(|_| {
            AppError::new(
                "resolver_busy",
                "A track is already being inspected. Cancel it before starting another.",
                true,
            )
        })?;
        let mut cancel = self.cancellation.subscribe();
        let generation = *cancel.borrow_and_update();
        let resolved = tokio::select! {
            result = provider::resolve(&self.paths, url) => result?,
            _ = cancel.changed() => return Err(AppError::cancelled()),
        };
        let mut current = self.resolved.lock().map_err(|_| AppError::internal())?;
        if *cancel.borrow() != generation {
            return Err(AppError::cancelled());
        }
        let inspection = resolved.inspection.clone();
        *current = Some(resolved);
        Ok(inspection)
    }

    pub fn cancel_inspection(&self) {
        self.cancellation
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    pub async fn play(&self, track_id: &str, format_id: &str) -> Result<PlaybackSnapshot> {
        let (track, format, source) = self
            .resolved
            .lock()
            .map_err(|_| AppError::internal())?
            .as_ref()
            .ok_or_else(|| {
                AppError::new(
                    "track_not_inspected",
                    "Inspect a track to discover its available quality first.",
                    false,
                )
            })?
            .source(track_id, format_id)?;
        self.player.load(track, format, source).await
    }

    pub fn snapshot(&self) -> Result<PlaybackSnapshot> {
        self.player.snapshot()
    }
    pub async fn pause(&self, paused: bool) -> Result<PlaybackSnapshot> {
        self.player.pause(paused).await
    }
    pub async fn seek(&self, position: f64) -> Result<PlaybackSnapshot> {
        self.player.seek(position).await
    }
    pub async fn volume(&self, volume: f64) -> Result<PlaybackSnapshot> {
        self.player.volume(volume).await
    }
    pub async fn stop(&self) -> Result<PlaybackSnapshot> {
        self.cancel_inspection();
        self.player.stop().await
    }
    pub fn shutdown(&self) {
        self.cancel_inspection();
        self.player.shutdown();
    }
}
