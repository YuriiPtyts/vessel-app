use std::sync::{Arc, Mutex};

use tokio::sync::{Semaphore, watch};

use crate::{
    catalog::{MusicCatalog, SearchKind, SearchResults},
    error::{AppError, Result},
    library::{LibrarySnapshot, LocalLibrary},
    playback::{PlaybackSnapshot, PlaybackStatus, Player},
    provider::{self, ResolvedTrack, TrackInspection},
    runtime::RuntimePaths,
};

pub struct Application {
    paths: RuntimePaths,
    player: Player,
    resolved: Mutex<Option<ResolvedTrack>>,
    resolver_slot: Semaphore,
    cancellation: watch::Sender<u64>,
    catalog: MusicCatalog,
    library: Arc<LocalLibrary>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSelection {
    pub inspection: TrackInspection,
    pub playback: PlaybackSnapshot,
}

impl Application {
    pub fn new(
        paths: RuntimePaths,
        emit: impl Fn(PlaybackSnapshot) + Send + Sync + 'static,
    ) -> Result<Self> {
        Self::with_library(paths, Arc::new(LocalLibrary::memory()), emit)
    }

    pub fn with_library(
        paths: RuntimePaths,
        library: Arc<LocalLibrary>,
        emit: impl Fn(PlaybackSnapshot) + Send + Sync + 'static,
    ) -> Result<Self> {
        let (cancellation, _) = watch::channel(0);
        let history = library.clone();
        let last_played = Mutex::new(None::<String>);
        Ok(Self {
            player: Player::new(
                paths.clone(),
                Arc::new(move |snapshot| {
                    if let Ok(mut previous) = last_played.lock() {
                        if snapshot.status == PlaybackStatus::Playing
                            && let Some(track) = &snapshot.track
                            && previous.as_deref() != Some(&track.id)
                        {
                            // This actor is a control thread, not libmpv's realtime audio callback.
                            let _ = history.record(track.clone());
                            *previous = Some(track.id.clone());
                        } else if snapshot.status == PlaybackStatus::Idle {
                            *previous = None;
                        }
                    }
                    emit(snapshot);
                }),
            ),
            paths,
            resolved: Mutex::new(None),
            resolver_slot: Semaphore::new(1),
            cancellation,
            catalog: MusicCatalog::new()?,
            library,
        })
    }

    pub fn runtime_available(&self) -> bool {
        self.paths.available()
    }

    pub async fn inspect(&self, url: &str) -> Result<TrackInspection> {
        self.prepare(url).await.map(|(inspection, _)| inspection)
    }

    async fn prepare(&self, url: &str) -> Result<(TrackInspection, u64)> {
        let mut cancel = self.cancellation.subscribe();
        self.cancel_inspection();
        let generation = *cancel.borrow_and_update();
        let _slot = tokio::select! {
            slot = self.resolver_slot.acquire() => slot.map_err(|_| AppError::internal())?,
            _ = cancel.changed() => return Err(AppError::cancelled()),
        };
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
        Ok((inspection, generation))
    }

    pub async fn search(&self, query: &str, kind: SearchKind, more: bool) -> Result<SearchResults> {
        self.catalog.search(query, kind, more).await
    }

    pub async fn play_song(&self, video_id: &str) -> Result<PlaybackSelection> {
        if !crate::catalog::valid_video_id(video_id) {
            return Err(AppError::new(
                "invalid_track",
                "Choose a valid song from the music catalog.",
                false,
            ));
        }
        let metadata = self
            .catalog
            .find_track(video_id)
            .or_else(|| self.library.find(video_id));
        let (mut inspection, generation) = self
            .prepare(&format!("https://www.youtube.com/watch?v={video_id}"))
            .await?;
        {
            let mut current = self.resolved.lock().map_err(|_| AppError::internal())?;
            if *self.cancellation.borrow() != generation {
                return Err(AppError::cancelled());
            }
            let resolved = current
                .as_mut()
                .filter(|resolved| resolved.inspection.track.id == video_id)
                .ok_or_else(AppError::cancelled)?;
            if let Some(mut track) = metadata {
                track.duration_seconds =
                    inspection.track.duration_seconds.or(track.duration_seconds);
                inspection.track = track;
                resolved.inspection.track = inspection.track.clone();
            }
        }
        let playback = self
            .play(video_id, &inspection.recommended_format_id)
            .await?;
        Ok(PlaybackSelection {
            inspection,
            playback,
        })
    }

    pub fn library(&self) -> Result<LibrarySnapshot> {
        self.library.snapshot()
    }

    pub fn toggle_like(&self, video_id: &str) -> Result<LibrarySnapshot> {
        let track = self
            .catalog
            .find_track(video_id)
            .or_else(|| self.library.find(video_id))
            .or_else(|| {
                self.player
                    .snapshot()
                    .ok()?
                    .track
                    .filter(|track| track.id == video_id)
            })
            .ok_or_else(|| {
                AppError::new(
                    "unknown_track",
                    "Search for this track again before saving it.",
                    true,
                )
            })?;
        self.library.toggle_like(track)
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
                    "Choose a song before changing its audio quality.",
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
        self.catalog.cancel();
        self.player.shutdown();
    }
}
