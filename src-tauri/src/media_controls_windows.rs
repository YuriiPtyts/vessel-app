//! Windows System Media Transport Controls own no playback state; commands route to Rust.
use std::{cell::RefCell, sync::Arc};
use vessel_app::{
    playback::{PlaybackSnapshot, PlaybackStatus},
    provider::Track,
};
use windows::{
    Foundation::TypedEventHandler,
    Media::{
        MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
        SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    },
    Win32::{Foundation::HWND, System::WinRT::ISystemMediaTransportControlsInterop},
    core::{HSTRING, factory},
};

pub enum Action {
    Previous,
    Next,
    Play,
    Pause,
}

struct Controls {
    native: SystemMediaTransportControls,
    token: i64,
    last: Option<(PlaybackStatus, Option<Track>)>,
}
thread_local! {
    static CONTROLS: RefCell<Option<Controls>> = const { RefCell::new(None) };
}

pub fn install(
    window: HWND,
    dispatch: Arc<dyn Fn(Action) + Send + Sync>,
) -> windows::core::Result<()> {
    shutdown();
    let interop = factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
    // SAFETY: Tauri supplies its own live main-window HWND on the UI thread.
    let native: SystemMediaTransportControls = unsafe { interop.GetForWindow(window)? };
    native.SetIsEnabled(false)?;
    let handler = TypedEventHandler::<
        SystemMediaTransportControls,
        SystemMediaTransportControlsButtonPressedEventArgs,
    >::new(move |_, event| {
        if let Some(event) = event.as_ref() {
            let action = match event.Button()? {
                SystemMediaTransportControlsButton::Previous => Some(Action::Previous),
                SystemMediaTransportControlsButton::Next => Some(Action::Next),
                SystemMediaTransportControlsButton::Play => Some(Action::Play),
                SystemMediaTransportControlsButton::Pause => Some(Action::Pause),
                _ => None,
            };
            if let Some(action) = action {
                dispatch(action);
            }
        }
        Ok(())
    });
    let token = native.ButtonPressed(&handler)?;
    CONTROLS.with(|slot| {
        *slot.borrow_mut() = Some(Controls {
            native,
            token,
            last: None,
        })
    });
    Ok(())
}

pub fn update(snapshot: &PlaybackSnapshot) {
    CONTROLS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(controls) = slot.as_mut() else {
            return;
        };
        if controls
            .last
            .as_ref()
            .is_some_and(|(status, track)| *status == snapshot.status && *track == snapshot.track)
        {
            return;
        }
        let active = snapshot.track.is_some()
            && !matches!(
                snapshot.status,
                PlaybackStatus::Idle | PlaybackStatus::Ended | PlaybackStatus::Failed
            );
        let result = (|| -> windows::core::Result<()> {
            controls.native.SetIsEnabled(active)?;
            controls.native.SetIsNextEnabled(active)?;
            controls.native.SetIsPreviousEnabled(active)?;
            controls.native.SetIsPlayEnabled(active)?;
            controls.native.SetIsPauseEnabled(active)?;
            let updater = controls.native.DisplayUpdater()?;
            if let Some(track) = snapshot.track.as_ref().filter(|_| active) {
                updater.SetType(MediaPlaybackType::Music)?;
                let music = updater.MusicProperties()?;
                music.SetTitle(&HSTRING::from(track.title.as_str()))?;
                music.SetArtist(&HSTRING::from(track.artist.as_str()))?;
                music.SetAlbumTitle(&HSTRING::from(track.album.as_deref().unwrap_or("")))?;
                updater.Update()?;
                controls.native.SetPlaybackStatus(
                    if snapshot.status == PlaybackStatus::Playing {
                        MediaPlaybackStatus::Playing
                    } else {
                        MediaPlaybackStatus::Paused
                    },
                )?;
            } else {
                updater.ClearAll()?;
                updater.Update()?;
                controls
                    .native
                    .SetPlaybackStatus(MediaPlaybackStatus::Stopped)?;
            }
            Ok(())
        })();
        if result.is_ok() {
            controls.last = Some((snapshot.status, snapshot.track.clone()));
        }
    });
}

pub fn shutdown() {
    CONTROLS.with(|slot| {
        if let Some(controls) = slot.borrow_mut().take() {
            let _ = controls.native.SetIsEnabled(false);
            let _ = controls.native.RemoveButtonPressed(controls.token);
            if let Ok(updater) = controls.native.DisplayUpdater() {
                let _ = updater.ClearAll();
                let _ = updater.Update();
            }
        }
    });
}

#[cfg(feature = "native-diagnostics")]
pub fn available() -> bool {
    CONTROLS.with(|slot| slot.borrow().is_some())
}
