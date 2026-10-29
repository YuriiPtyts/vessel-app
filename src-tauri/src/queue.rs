//! Authoritative queue/radio transitions, independent of the renderer and media engine.
use crate::{
    error::{AppError, Result},
    playback::{PlaybackSnapshot, PlaybackStatus},
    provider::Track,
    radio::{RadioBatch, RadioCursor},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    time::{Duration, Instant},
};

const MAX_TRACKS: usize = 100;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    pub revision: u64,
    pub tracks: Vec<Track>,
    pub current_index: Option<usize>,
    pub autoplay: bool,
    pub running: bool,
    pub generating: bool,
    pub transitioning: bool,
    pub preparing_id: Option<String>,
    pub seed_title: Option<String>,
    pub error: Option<AppError>,
}
#[derive(Serialize, Deserialize)]
struct SavedQueue {
    version: u8,
    tracks: Vec<Track>,
    current_index: Option<usize>,
    autoplay: bool,
}

pub(crate) struct RadioQueue {
    pub snapshot: QueueSnapshot,
    pub generation: u64,
    pub cursor: Option<RadioCursor>,
    pub seed_id: Option<String>,
    playback_id: Option<u64>,
    handled_end: Option<u64>,
    last_radio: Option<Instant>,
    prefetched: Option<(u64, String)>,
    path: Option<PathBuf>,
    writable: bool,
}

impl RadioQueue {
    pub fn new(path: Option<PathBuf>) -> Self {
        let mut result = Self {
            snapshot: QueueSnapshot {
                revision: 0,
                tracks: Vec::new(),
                current_index: None,
                autoplay: true,
                running: false,
                generating: false,
                transitioning: false,
                preparing_id: None,
                seed_title: None,
                error: None,
            },
            generation: 0,
            cursor: None,
            seed_id: None,
            playback_id: None,
            handled_end: None,
            last_radio: None,
            prefetched: None,
            path,
            writable: true,
        };
        if let Some(path) = &result.path {
            match load(path) {
                Ok(Some(saved)) => {
                    result.snapshot.tracks = saved.tracks;
                    result.snapshot.current_index = saved
                        .current_index
                        .filter(|index| *index < result.snapshot.tracks.len());
                    result.snapshot.autoplay = saved.autoplay;
                }
                Ok(None) => {}
                Err(error) => {
                    result.writable = false;
                    result.snapshot.error = Some(error);
                }
            }
        }
        result
    }
    pub fn published_snapshot(&mut self) -> QueueSnapshot {
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        self.snapshot.clone()
    }
    pub fn current(&self) -> Option<Track> {
        self.snapshot
            .current_index
            .and_then(|index| self.snapshot.tracks.get(index))
            .cloned()
    }
    pub fn future_count(&self) -> usize {
        self.snapshot
            .current_index
            .map_or(self.snapshot.tracks.len(), |index| {
                self.snapshot.tracks.len().saturating_sub(index + 1)
            })
    }
    pub fn begin(&mut self, id: &str, track: Option<Track>, context: Vec<Track>) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.cursor = None;
        self.seed_id = Some(id.into());
        self.playback_id = None;
        self.handled_end = None;
        self.last_radio = None;
        self.prefetched = None;
        let mut ids = std::collections::BTreeSet::new();
        let mut tracks: Vec<_> = context
            .into_iter()
            .filter(|track| ids.insert(track.id.clone()))
            .take(MAX_TRACKS)
            .collect();
        let mut index = tracks.iter().position(|track| track.id == id);
        if index.is_none() {
            tracks.clear();
            if let Some(track) = track {
                tracks.push(track);
            }
        } else if let Some(current) = index
            && current > 20
        {
            tracks.drain(..current - 20);
            index = Some(20);
        }
        self.snapshot.tracks = tracks;
        self.snapshot.current_index =
            index.or_else(|| (!self.snapshot.tracks.is_empty()).then_some(0));
        self.snapshot.seed_title = self.current().map(|track| track.title);
        self.snapshot.running = true;
        self.snapshot.generating = false;
        self.snapshot.transitioning = true;
        self.snapshot.preparing_id = None;
        self.snapshot.error = None;
        self.generation
    }
    pub fn ensure_seed(&mut self, track: Track, generation: u64) -> Result<()> {
        self.check(generation)?;
        if self.snapshot.current_index.is_none() {
            self.snapshot.tracks.insert(0, track.clone());
            self.snapshot.tracks.truncate(MAX_TRACKS);
            self.snapshot.current_index = Some(0);
        } else if let Some(current) = self.snapshot.current_index
            && self.snapshot.tracks[current].id == track.id
        {
            self.snapshot.tracks[current] = track.clone();
        }
        if self.snapshot.seed_title.is_none() {
            self.snapshot.seed_title = Some(track.title);
        }
        Ok(())
    }
    pub fn bind(&mut self, generation: u64, playback: &PlaybackSnapshot) -> Result<()> {
        self.check(generation)?;
        if self.current().as_ref().map(|track| &track.id)
            != playback.track.as_ref().map(|track| &track.id)
        {
            return Err(AppError::cancelled());
        }
        self.playback_id = Some(playback.playback_id);
        self.snapshot.transitioning = false;
        self.snapshot.error = None;
        self.persist();
        Ok(())
    }
    pub fn cancel_work(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.snapshot.transitioning = false;
        self.snapshot.generating = false;
        self.snapshot.preparing_id = None;
        self.prefetched = None;
        self.generation
    }
    pub fn stop(&mut self) {
        self.cancel_work();
        self.snapshot.running = false;
        self.playback_id = None;
        self.persist();
    }
    pub fn clear(&mut self) {
        self.stop();
        self.snapshot.tracks.clear();
        self.snapshot.current_index = None;
        self.snapshot.seed_title = None;
        self.seed_id = None;
        self.cursor = None;
        self.snapshot.error = None;
        self.persist();
    }
    pub fn jump(&mut self, index: usize, expected_id: Option<&str>) -> Result<Track> {
        let track = self
            .snapshot
            .tracks
            .get(index)
            .filter(|track| expected_id.is_none_or(|id| track.id == id))
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    "queue_item_missing",
                    "This queue item is no longer available.",
                    true,
                )
            })?;
        self.snapshot.current_index = Some(index);
        self.snapshot.running = true;
        self.snapshot.transitioning = true;
        self.playback_id = None;
        self.handled_end = None;
        self.prefetched = None;
        if self.seed_id.is_none() {
            self.seed_id = Some(track.id.clone());
            self.snapshot.seed_title = Some(track.title.clone());
        }
        self.trim_history();
        self.persist();
        Ok(track)
    }
    pub fn remove(&mut self, index: usize, expected_id: &str) -> Result<()> {
        if self.snapshot.transitioning {
            return Err(AppError::new(
                "queue_busy",
                "Wait for the next song to finish loading before editing the queue.",
                true,
            ));
        }
        if self.snapshot.current_index == Some(index) {
            return Err(AppError::new(
                "current_queue_item",
                "Choose another song before removing the current item.",
                false,
            ));
        }
        if self
            .snapshot
            .tracks
            .get(index)
            .is_none_or(|track| track.id != expected_id)
        {
            return Err(AppError::new(
                "queue_item_missing",
                "This queue item is no longer available.",
                true,
            ));
        }
        self.snapshot.tracks.remove(index);
        if let Some(current) = self.snapshot.current_index
            && index < current
        {
            self.snapshot.current_index = Some(current - 1);
        }
        self.prefetched = None;
        self.snapshot.preparing_id = None;
        self.persist();
        Ok(())
    }
    pub fn set_autoplay(&mut self, enabled: bool) {
        self.snapshot.autoplay = enabled;
        self.last_radio = None;
        self.persist();
    }
    pub fn claim_end(&mut self, playback: &PlaybackSnapshot) -> Option<u64> {
        if playback.status != PlaybackStatus::Ended
            || !self.snapshot.running
            || (!self.snapshot.autoplay && self.future_count() == 0)
            || self.snapshot.transitioning
            || self.playback_id != Some(playback.playback_id)
            || self.handled_end == Some(playback.playback_id)
            || self.current().as_ref().map(|track| &track.id)
                != playback.track.as_ref().map(|track| &track.id)
        {
            return None;
        }
        self.handled_end = Some(playback.playback_id);
        self.snapshot.transitioning = true;
        Some(self.generation)
    }
    pub fn reserve_radio(&mut self, force: bool) -> Option<(u64, String, Option<RadioCursor>)> {
        if !self.snapshot.running
            || !self.snapshot.autoplay
            || self.snapshot.generating
            || (!force
                && (self.future_count() >= 8
                    || self
                        .last_radio
                        .is_some_and(|time| time.elapsed() < Duration::from_secs(20))))
        {
            return None;
        }
        let seed = self
            .seed_id
            .clone()
            .or_else(|| self.current().map(|track| track.id))?;
        self.snapshot.generating = true;
        self.last_radio = Some(Instant::now());
        Some((self.generation, seed, self.cursor.clone()))
    }
    pub fn append(&mut self, generation: u64, batch: RadioBatch) -> Result<()> {
        self.check(generation)?;
        self.snapshot.generating = false;
        if !self.snapshot.running || !self.snapshot.autoplay {
            return Ok(());
        }
        let mut ids: std::collections::BTreeSet<_> = self
            .snapshot
            .tracks
            .iter()
            .map(|track| track.id.clone())
            .collect();
        if let Some(seed) = &self.seed_id {
            ids.insert(seed.clone());
        }
        let before = self.snapshot.tracks.len();
        for track in batch.tracks {
            if ids.insert(track.id.clone()) && self.snapshot.tracks.len() < MAX_TRACKS {
                self.snapshot.tracks.push(track);
            }
        }
        self.cursor = batch.cursor;
        if before == self.snapshot.tracks.len() {
            self.snapshot.error = Some(crate::radio::radio_error());
        } else {
            self.snapshot.error = None;
        }
        self.persist();
        Ok(())
    }
    pub fn fail_radio(&mut self, generation: u64, error: AppError) {
        if self.generation == generation {
            self.snapshot.generating = false;
            if error.code != "cancelled" {
                self.snapshot.error = Some(error);
            }
        }
    }
    pub fn fail_transition(&mut self, generation: u64, error: AppError) {
        if self.generation == generation {
            self.snapshot.transitioning = false;
            self.snapshot.error = Some(error);
        }
    }
    pub fn prefetch_candidate(&mut self, playback: &PlaybackSnapshot) -> Option<(u64, Track)> {
        if playback.status != PlaybackStatus::Playing
            || !self.snapshot.running
            || self.snapshot.transitioning
            || self.playback_id != Some(playback.playback_id)
            || self.current().as_ref().map(|track| &track.id)
                != playback.track.as_ref().map(|track| &track.id)
            || playback.duration_seconds? - playback.position_seconds > 30.0
        {
            return None;
        }
        let track = self
            .snapshot
            .tracks
            .get(self.snapshot.current_index? + 1)?
            .clone();
        let key = (playback.playback_id, track.id.clone());
        if self.prefetched.as_ref() == Some(&key) {
            return None;
        }
        self.prefetched = Some(key);
        self.snapshot.preparing_id = Some(track.id.clone());
        Some((self.generation, track))
    }
    pub fn finish_prefetch(&mut self, generation: u64, id: &str) {
        if self.generation == generation && self.snapshot.preparing_id.as_deref() == Some(id) {
            self.snapshot.preparing_id = None;
        }
    }
    pub fn account_changed(&mut self) {
        self.cancel_work();
        self.handled_end = None;
        self.cursor = None;
        self.last_radio = None;
        self.snapshot.generating = false;
        self.prefetched = None;
        self.snapshot.preparing_id = None;
    }
    pub fn retry_radio(&mut self) {
        self.last_radio = None;
        if !self.snapshot.transitioning {
            self.handled_end = None;
        }
    }
    pub fn check(&self, generation: u64) -> Result<()> {
        if self.generation == generation {
            Ok(())
        } else {
            Err(AppError::cancelled())
        }
    }
    fn trim_history(&mut self) {
        if let Some(index) = self.snapshot.current_index
            && index > 20
        {
            self.snapshot.tracks.drain(..index - 20);
            self.snapshot.current_index = Some(20);
        }
    }
    fn persist(&mut self) {
        if !self.writable {
            return;
        }
        let Some(path) = &self.path else { return };
        let save = || -> Result<()> {
            fs::create_dir_all(path.parent().ok_or_else(storage_error)?)
                .map_err(|_| storage_error())?;
            let value = SavedQueue {
                version: 1,
                tracks: self.snapshot.tracks.clone(),
                current_index: self.snapshot.current_index,
                autoplay: self.snapshot.autoplay,
            };
            let temp = path.with_extension("json.tmp");
            fs::write(
                &temp,
                serde_json::to_vec(&value).map_err(|_| storage_error())?,
            )
            .map_err(|_| storage_error())?;
            fs::rename(temp, path).map_err(|_| storage_error())?;
            Ok(())
        };
        if let Err(error) = save() {
            self.snapshot.error = Some(error);
        }
    }
}

fn load(path: &PathBuf) -> Result<Option<SavedQueue>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(storage_error()),
    };
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| storage_error())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(storage_error());
    }
    let mut saved: SavedQueue = serde_json::from_slice(&bytes).map_err(|_| storage_error())?;
    if saved.version != 1 {
        return Err(storage_error());
    }
    let mut ids = std::collections::BTreeSet::new();
    if saved
        .tracks
        .iter()
        .any(|track| !crate::provider::valid_track_id(&track.id) || !ids.insert(&track.id))
    {
        return Err(storage_error());
    }
    saved.tracks.truncate(MAX_TRACKS);
    for track in &mut saved.tracks {
        track.thumbnail_url = track
            .thumbnail_url
            .as_deref()
            .and_then(crate::provider::artwork_url);
    }
    Ok(Some(saved))
}
fn storage_error() -> AppError {
    AppError::new(
        "queue_storage",
        "The saved queue could not be read or written. Listening can continue in this session.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            artist: "Fixture".into(),
            album: None,
            thumbnail_url: None,
            duration_seconds: Some(10.0),
        }
    }
    #[test]
    fn only_a_matching_natural_end_advances_once_and_stop_never_advances() {
        let mut queue = RadioQueue::new(None);
        let song = track("khnokW3Mw24");
        let generation = queue.begin(&song.id, Some(song.clone()), vec![]);
        let mut playback = PlaybackSnapshot {
            track: Some(song),
            playback_id: 7,
            status: PlaybackStatus::Playing,
            ..Default::default()
        };
        queue.bind(generation, &playback).unwrap();
        playback.status = PlaybackStatus::Ended;
        assert_eq!(queue.claim_end(&playback), Some(generation));
        assert!(queue.claim_end(&playback).is_none());
        queue.stop();
        assert!(queue.claim_end(&playback).is_none());
    }
    #[test]
    fn stale_radio_responses_and_seed_duplicates_cannot_replace_a_new_queue() {
        let mut queue = RadioQueue::new(None);
        let first = track("khnokW3Mw24");
        let old = queue.begin(&first.id, Some(first.clone()), vec![]);
        let next = track("d7woYjGWs6k");
        let generation = queue.begin(&next.id, Some(next.clone()), vec![]);
        assert!(
            queue
                .append(
                    old,
                    RadioBatch {
                        tracks: vec![first],
                        cursor: None
                    }
                )
                .is_err()
        );
        queue
            .append(
                generation,
                RadioBatch {
                    tracks: vec![next, track("LxgnIhjQFuE")],
                    cursor: None,
                },
            )
            .unwrap();
        assert_eq!(queue.snapshot.tracks.len(), 2);
        queue.set_autoplay(false);
        let mut playback = PlaybackSnapshot {
            track: queue.current(),
            playback_id: 2,
            status: PlaybackStatus::Playing,
            ..Default::default()
        };
        queue.bind(generation, &playback).unwrap();
        playback.status = PlaybackStatus::Ended;
        // Disabling radio still finishes the explicitly queued songs.
        assert!(queue.claim_end(&playback).is_some());
    }

    #[test]
    fn disabling_radio_stops_at_the_end_of_the_explicit_queue() {
        let mut queue = RadioQueue::new(None);
        let song = track("khnokW3Mw24");
        let generation = queue.begin(&song.id, Some(song.clone()), vec![]);
        let mut playback = PlaybackSnapshot {
            track: Some(song),
            playback_id: 3,
            status: PlaybackStatus::Playing,
            ..Default::default()
        };
        queue.bind(generation, &playback).unwrap();
        queue.set_autoplay(false);
        playback.status = PlaybackStatus::Ended;
        assert!(queue.claim_end(&playback).is_none());
        queue.set_autoplay(true);
        assert_eq!(queue.claim_end(&playback), Some(generation));
        queue.account_changed();
        assert!(
            queue.check(generation).is_err(),
            "account changes cancel a claimed EOF transition"
        );
        assert_eq!(
            queue.claim_end(&playback),
            Some(queue.generation),
            "the new account context can resume a cancelled EOF transition"
        );
    }

    #[test]
    fn queue_round_trips_without_resuming_and_malformed_data_is_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("queue.json");
        let mut queue = RadioQueue::new(Some(path.clone()));
        let song = track("khnokW3Mw24");
        let context = vec![song.clone(), track("d7woYjGWs6k")];
        let generation = queue.begin(&song.id, Some(song.clone()), context);
        queue
            .bind(
                generation,
                &PlaybackSnapshot {
                    track: Some(song),
                    playback_id: 5,
                    status: PlaybackStatus::Playing,
                    ..Default::default()
                },
            )
            .unwrap();
        queue.set_autoplay(false);
        let restored = RadioQueue::new(Some(path.clone()));
        assert_eq!(restored.snapshot.tracks.len(), 2);
        assert_eq!(restored.snapshot.current_index, Some(0));
        assert!(!restored.snapshot.running);
        assert!(!restored.snapshot.autoplay);
        let persisted = fs::read_to_string(&path).unwrap();
        assert!(!persisted.contains("playbackId"));
        assert!(!persisted.contains("continuation"));
        fs::write(&path, "unrecognized old queue").unwrap();
        let mut malformed = RadioQueue::new(Some(path.clone()));
        assert!(malformed.snapshot.error.is_some());
        malformed.clear();
        assert_eq!(fs::read_to_string(path).unwrap(), "unrecognized old queue");
    }

    #[test]
    fn starting_late_in_a_collection_preserves_the_remaining_order() {
        let context: Vec<_> = (0..100)
            .map(|index| track(&format!("{index:011}")))
            .collect();
        let song = context[93].clone();
        let mut queue = RadioQueue::new(None);
        queue.begin(&song.id, Some(song.clone()), context);
        assert_eq!(queue.snapshot.current_index, Some(20));
        assert_eq!(queue.future_count(), 6);
        assert_eq!(queue.snapshot.tracks[21].id, format!("{:011}", 94));
    }

    #[test]
    fn an_outdated_rendered_index_cannot_play_or_remove_a_different_song() {
        let mut queue = RadioQueue::new(None);
        let first = track("khnokW3Mw24");
        let second = track("d7woYjGWs6k");
        let third = track("LxgnIhjQFuE");
        let generation = queue.begin(
            &first.id,
            Some(first.clone()),
            vec![first.clone(), second.clone(), third.clone()],
        );
        queue
            .bind(
                generation,
                &PlaybackSnapshot {
                    track: Some(first),
                    playback_id: 1,
                    status: PlaybackStatus::Playing,
                    ..Default::default()
                },
            )
            .unwrap();
        queue.remove(1, &second.id).unwrap();
        assert!(queue.remove(1, &second.id).is_err());
        assert!(queue.jump(1, Some(&second.id)).is_err());
        assert_eq!(queue.snapshot.tracks[1].id, third.id);
    }
}
