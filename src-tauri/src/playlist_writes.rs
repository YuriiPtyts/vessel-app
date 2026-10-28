//! Explicit, acknowledged account writes. Never automatically retry a mutation.
use super::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PlaylistPrivacy {
    Private,
    Unlisted,
    Public,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistMutation {
    pub playlist: HomeItem,
    pub library: AccountLibrarySnapshot,
}

impl AccountLibrary {
    pub async fn create_playlist(
        &self,
        catalog: &MusicCatalog,
        title: &str,
        description: &str,
        privacy: PlaylistPrivacy,
        initial_track: Option<Track>,
        revision: u64,
    ) -> Result<PlaylistMutation> {
        let title = title.trim();
        if !(1..=100).contains(&title.chars().count())
            || title
                .chars()
                .any(|character| character.is_control() || matches!(character, '<' | '>'))
            || description.chars().count() > 5_000
            || description
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(AppError::new(
                "invalid_playlist",
                "Use a playlist name of 1–100 characters without < or >, and a description of up to 5,000 characters.",
                false,
            ));
        }
        self.synchronize_account()?;
        self.check_revision(revision)?;
        let mut cancel = self.cancellation.subscribe();
        let _work = tokio::select! { lock = self.work.lock() => lock, _ = cancel.changed() => return Err(AppError::cancelled()) };
        self.check_revision(revision)?;
        if self
            .state
            .lock()
            .map_err(|_| AppError::internal())?
            .public
            .playlists
            .len()
            >= MAX_PLAYLISTS
        {
            return Err(library_limit());
        }
        let mut body = json!({"title":title,"description":description,"privacyStatus":privacy});
        if let Some(track) = &initial_track {
            if !valid_video_id(&track.id) {
                return Err(write_error());
            }
            body["videoIds"] = json!([track.id]);
        }
        let data = tokio::select! {
            result = catalog.account_post("playlist/create", body, None, revision) => result.map_err(mutation_error)?,
            _ = cancel.changed() => return Err(AppError::cancelled()),
        };
        validate_write(&data, false)?;
        let id = data
            .get("playlistId")
            .and_then(Value::as_str)
            .filter(|id| valid_playlist_id(id))
            .ok_or_else(write_error)?;
        self.check_revision(revision)?;
        let item = HomeItem {
            id: format!("VL{id}"),
            kind: HomeItemKind::Playlist,
            title: title.into(),
            subtitle: "Your YouTube Music playlist".into(),
            thumbnail_url: initial_track
                .as_ref()
                .and_then(|track| track.thumbnail_url.clone()),
            track: None,
        };
        let snapshot = {
            let mut state = self.state.lock().map_err(|_| AppError::internal())?;
            if state.public.account_revision != revision {
                return Err(AppError::cancelled());
            }
            state.targets.insert(
                item.id.clone(),
                CollectionTarget {
                    item: item.clone(),
                    params: None,
                },
            );
            state
                .public
                .playlists
                .retain(|playlist| playlist.id != item.id);
            state.public.playlists.insert(0, item.clone());
            if !state.public.editable_playlist_ids.contains(&item.id) {
                state.public.editable_playlist_ids.push(item.id.clone());
            }
            state.refreshed = None;
            state.public.revision += 1;
            state.public.clone()
        };
        (self.emit)(snapshot.clone());
        Ok(PlaylistMutation {
            playlist: item,
            library: snapshot,
        })
    }

    pub async fn add_to_playlist(
        &self,
        catalog: &MusicCatalog,
        id: &str,
        track: Track,
        revision: u64,
    ) -> Result<PlaylistMutation> {
        self.synchronize_account()?;
        self.check_revision(revision)?;
        let mut cancel = self.cancellation.subscribe();
        let _work = tokio::select! { lock = self.work.lock() => lock, _ = cancel.changed() => return Err(AppError::cancelled()) };
        let _collection = tokio::select! { lock = self.collection_work.lock() => lock, _ = cancel.changed() => return Err(AppError::cancelled()) };
        self.check_revision(revision)?;
        let item = {
            let state = self.state.lock().map_err(|_| AppError::internal())?;
            if !state
                .public
                .editable_playlist_ids
                .iter()
                .any(|editable| editable == id)
            {
                return Err(AppError::new(
                    "playlist_read_only",
                    "Choose one of your own YouTube Music playlists, or create a new one.",
                    false,
                ));
            }
            state
                .targets
                .get(id)
                .map(|target| target.item.clone())
                .ok_or_else(write_error)?
        };
        let raw_id = id
            .strip_prefix("VL")
            .filter(|id| valid_playlist_id(id))
            .ok_or_else(write_error)?;
        if !valid_video_id(&track.id) {
            return Err(write_error());
        }
        let body = json!({"playlistId":raw_id,"actions":[{"action":"ACTION_ADD_VIDEO","addedVideoId":track.id}]});
        let data = tokio::select! {
            result = catalog.account_post("browse/edit_playlist", body, None, revision) => result.map_err(mutation_error)?,
            _ = cancel.changed() => return Err(AppError::cancelled()),
        };
        validate_write(&data, true)?;
        self.check_revision(revision)?;
        let snapshot = {
            let mut state = self.state.lock().map_err(|_| AppError::internal())?;
            if state.public.account_revision != revision {
                return Err(AppError::cancelled());
            }
            if state
                .collection
                .as_ref()
                .is_some_and(|collection| collection.page.item.id == id)
            {
                state.collection = None;
            }
            state.refreshed = None;
            state.public.revision += 1;
            state.public.clone()
        };
        (self.emit)(snapshot.clone());
        Ok(PlaylistMutation {
            playlist: item,
            library: snapshot,
        })
    }
}

fn valid_playlist_id(id: &str) -> bool {
    id.starts_with("PL")
        && (3..=128).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_write(data: &Value, status_required: bool) -> Result<()> {
    let mut gated = false;
    walk(data, &mut |value| {
        gated |= value.get("confirmDialogRenderer").is_some()
            || value.get("signinPromoRenderer").is_some()
            || value.get("showEngagementPanelEndpoint").is_some();
    });
    let status = data.get("status").and_then(Value::as_str);
    if gated
        || data.get("error").is_some()
        || !data.get("responseContext").is_some_and(Value::is_object)
        || status.is_some_and(|status| status != "STATUS_SUCCEEDED")
        || (status_required && status != Some("STATUS_SUCCEEDED"))
    {
        return Err(write_error());
    }
    Ok(())
}

fn mutation_error(error: AppError) -> AppError {
    if matches!(
        error.code,
        "cancelled" | "account_required" | "session_expired"
    ) {
        error
    } else {
        write_error()
    }
}

fn write_error() -> AppError {
    AppError::new(
        "playlist_write_unconfirmed",
        "YouTube Music did not confirm the playlist change. Check or refresh the playlist before retrying; the song may already be there.",
        true,
    )
}
