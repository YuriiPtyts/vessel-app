//! A bounded local guest collection. It stores public metadata, never media URLs or credentials.
use std::{fs, io::Read, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, Result},
    provider::{Track, artwork_url, valid_track_id},
};

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub liked: Vec<Track>,
    pub recent: Vec<Track>,
}

pub struct LocalLibrary {
    path: Option<PathBuf>,
    data: Mutex<Result<LibrarySnapshot>>,
}

impl LocalLibrary {
    pub fn memory() -> Self {
        Self {
            path: None,
            data: Mutex::new(Ok(LibrarySnapshot::default())),
        }
    }

    pub fn open(path: PathBuf) -> Self {
        let data = read_library(&path);
        // A malformed/unreadable collection remains untouched; playback can still start.
        Self {
            path: Some(path),
            data: Mutex::new(data),
        }
    }

    pub fn snapshot(&self) -> Result<LibrarySnapshot> {
        self.data
            .lock()
            .map_err(|_| AppError::internal())?
            .as_ref()
            .cloned()
            .map_err(Clone::clone)
    }

    pub fn find(&self, id: &str) -> Option<Track> {
        let data = self.snapshot().ok()?;
        data.liked
            .iter()
            .chain(data.recent.iter())
            .find(|track| track.id == id)
            .cloned()
    }

    pub fn record(&self, track: Track) -> Result<LibrarySnapshot> {
        self.update(|data| {
            data.recent.retain(|current| current.id != track.id);
            data.recent.insert(0, track);
            data.recent.truncate(40);
            Ok(())
        })
    }

    pub fn toggle_like(&self, track: Track) -> Result<LibrarySnapshot> {
        self.update(|data| {
            if data.liked.iter().any(|current| current.id == track.id) {
                data.liked.retain(|current| current.id != track.id);
            } else {
                if data.liked.len() >= 500 {
                    return Err(AppError::new(
                        "collection_full",
                        "This early local collection supports 500 saved tracks.",
                        false,
                    ));
                }
                data.liked.insert(0, track);
            }
            Ok(())
        })
    }

    fn update(
        &self,
        change: impl FnOnce(&mut LibrarySnapshot) -> Result<()>,
    ) -> Result<LibrarySnapshot> {
        let mut guard = self.data.lock().map_err(|_| AppError::internal())?;
        let mut next = guard.as_ref().cloned().map_err(Clone::clone)?;
        change(&mut next)?;
        if let Some(path) = &self.path {
            let parent = path.parent().ok_or_else(library_error)?;
            fs::create_dir_all(parent).map_err(|_| library_error())?;
            let temporary = path.with_extension("json.tmp");
            let json = serde_json::to_vec(&next).map_err(|_| library_error())?;
            fs::write(&temporary, json).map_err(|_| library_error())?;
            fs::rename(&temporary, path).map_err(|_| library_error())?;
        }
        *guard = Ok(next.clone());
        Ok(next)
    }
}

fn read_library(path: &PathBuf) -> Result<LibrarySnapshot> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LibrarySnapshot::default());
        }
        Err(_) => return Err(library_error()),
    };
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| library_error())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(library_error());
    }
    let mut data: LibrarySnapshot = serde_json::from_slice(&bytes).map_err(|_| library_error())?;
    for list in [&mut data.liked, &mut data.recent] {
        list.retain(|track| valid_track_id(&track.id));
        for track in list.iter_mut() {
            track.thumbnail_url = track.thumbnail_url.as_deref().and_then(artwork_url);
        }
    }
    data.liked.truncate(500);
    data.recent.truncate(40);
    Ok(data)
}

fn library_error() -> AppError {
    AppError::new(
        "library_unavailable",
        "Your local collection could not be read or saved. Playback and search are still available.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            id: "khnokW3Mw24".into(),
            title: "Instant Crush".into(),
            artist: "Daft Punk".into(),
            duration_seconds: Some(338.0),
            album: None,
            thumbnail_url: None,
        }
    }

    #[test]
    fn collection_round_trips_and_history_does_not_duplicate_tracks() {
        let directory = std::env::temp_dir().join(format!("vessel-library-{}", std::process::id()));
        let path = directory.join("collection.json");
        let library = LocalLibrary::open(path.clone());
        library.record(track()).unwrap();
        library.record(track()).unwrap();
        library.toggle_like(track()).unwrap();
        let reopened = LocalLibrary::open(path).snapshot().unwrap();
        assert_eq!(reopened.recent.len(), 1);
        assert_eq!(reopened.liked.len(), 1);
        assert_eq!(library.toggle_like(track()).unwrap().liked.len(), 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_existing_collection_is_preserved() {
        let directory =
            std::env::temp_dir().join(format!("vessel-library-corrupt-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("collection.json");
        fs::write(&path, b"not-valid-json").unwrap();
        let library = LocalLibrary::open(path.clone());
        assert_eq!(library.snapshot().unwrap_err().code, "library_unavailable");
        assert!(library.toggle_like(track()).is_err());
        assert_eq!(fs::read(path).unwrap(), b"not-valid-json");
        fs::remove_dir_all(directory).unwrap();
    }
}
