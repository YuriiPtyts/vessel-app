//! macOS media keys and Now Playing are native controls over the Rust player.
use block2::RcBlock;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;
use std::{cell::RefCell, sync::Arc, time::Instant};
use vessel_app::playback::{PlaybackSnapshot, PlaybackStatus};

#[link(name = "MediaPlayer", kind = "framework")]
unsafe extern "C" {
    static MPMediaItemPropertyTitle: *const AnyObject;
    static MPMediaItemPropertyArtist: *const AnyObject;
    static MPMediaItemPropertyAlbumTitle: *const AnyObject;
    static MPMediaItemPropertyPlaybackDuration: *const AnyObject;
    static MPNowPlayingInfoPropertyElapsedPlaybackTime: *const AnyObject;
    static MPNowPlayingInfoPropertyPlaybackRate: *const AnyObject;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Previous,
    Toggle,
    Next,
    Play,
    Pause,
}

struct Binding {
    command: Retained<AnyObject>,
    target: Retained<AnyObject>,
    #[cfg_attr(not(feature = "native-diagnostics"), allow(dead_code))]
    handler: RcBlock<dyn Fn(*mut AnyObject) -> isize>,
}

struct Controls {
    bindings: Vec<Binding>,
    last: Option<(PlaybackSnapshot, Instant)>,
}

thread_local! {
    static CONTROLS: RefCell<Option<Controls>> = const { RefCell::new(None) };
}

pub fn install(dispatch: Arc<dyn Fn(Action) + Send + Sync>) {
    assert!(objc2::MainThreadMarker::new().is_some());
    shutdown();
    // SAFETY: MediaPlayer is linked and all Objective-C state is owned on the
    // main thread. The framework copies each block; it captures only Send/Sync Rust state.
    let bindings = unsafe {
        let center: Retained<AnyObject> =
            msg_send![class!(MPRemoteCommandCenter), sharedCommandCenter];
        [
            Action::Previous,
            Action::Toggle,
            Action::Next,
            Action::Play,
            Action::Pause,
        ]
        .into_iter()
        .map(|action| {
            let command: Retained<AnyObject> = match action {
                Action::Previous => msg_send![&*center, previousTrackCommand],
                Action::Toggle => msg_send![&*center, togglePlayPauseCommand],
                Action::Next => msg_send![&*center, nextTrackCommand],
                Action::Play => msg_send![&*center, playCommand],
                Action::Pause => msg_send![&*center, pauseCommand],
            };
            let dispatch = dispatch.clone();
            let handler: RcBlock<dyn Fn(*mut AnyObject) -> isize> = RcBlock::new(move |_| {
                dispatch(action);
                0 // MPRemoteCommandHandlerStatusSuccess: queued on the Rust player.
            });
            let target: Retained<AnyObject> = msg_send![&*command, addTargetWithHandler: &*handler];
            let _: () = msg_send![&*command, setEnabled: false];
            Binding {
                command,
                target,
                handler,
            }
        })
        .collect()
    };
    CONTROLS.with(|slot| {
        *slot.borrow_mut() = Some(Controls {
            bindings,
            last: None,
        })
    });
}

pub fn update(snapshot: &PlaybackSnapshot) {
    CONTROLS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(controls) = slot.as_mut() else {
            return;
        };
        if let Some((previous, time)) = &controls.last {
            let expected = previous.position_seconds
                + if previous.status == PlaybackStatus::Playing {
                    time.elapsed().as_secs_f64()
                } else {
                    0.0
                };
            if previous.track == snapshot.track
                && previous.status == snapshot.status
                && previous.duration_seconds == snapshot.duration_seconds
                && (expected - snapshot.position_seconds).abs() < 0.75
            {
                return;
            }
        }
        let active = snapshot.track.is_some()
            && !matches!(
                snapshot.status,
                PlaybackStatus::Idle | PlaybackStatus::Ended | PlaybackStatus::Failed
            );
        // SAFETY: this function is scheduled on the main thread. The dictionary
        // copies/retains its strings and numbers, and Now Playing copies the dictionary.
        unsafe {
            let center: Retained<AnyObject> =
                msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
            for binding in &controls.bindings {
                let _: () = msg_send![&*binding.command, setEnabled: active];
            }
            if let Some(track) = snapshot.track.as_ref().filter(|_| active) {
                let info: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
                for (key, text) in [
                    (MPMediaItemPropertyTitle, track.title.as_str()),
                    (MPMediaItemPropertyArtist, track.artist.as_str()),
                    (
                        MPMediaItemPropertyAlbumTitle,
                        track.album.as_deref().unwrap_or(""),
                    ),
                ] {
                    let text = NSString::from_str(text);
                    let _: () = msg_send![&*info, setObject: &*text, forKey: key];
                }
                let playing = snapshot.status == PlaybackStatus::Playing;
                for (key, value) in [
                    (
                        MPMediaItemPropertyPlaybackDuration,
                        snapshot.duration_seconds.unwrap_or(0.0),
                    ),
                    (
                        MPNowPlayingInfoPropertyElapsedPlaybackTime,
                        snapshot.position_seconds,
                    ),
                    (
                        MPNowPlayingInfoPropertyPlaybackRate,
                        if playing { 1.0 } else { 0.0 },
                    ),
                ] {
                    let value: Retained<AnyObject> =
                        msg_send![class!(NSNumber), numberWithDouble: value];
                    let _: () = msg_send![&*info, setObject: &*value, forKey: key];
                }
                let _: () = msg_send![&*center, setNowPlayingInfo: &*info];
                let state = if playing { 1_usize } else { 2_usize };
                let _: () = msg_send![&*center, setPlaybackState: state];
            } else {
                let _: () = msg_send![&*center, setNowPlayingInfo: std::ptr::null::<AnyObject>()];
                let _: () = msg_send![&*center, setPlaybackState: 3_usize];
            }
        }
        controls.last = Some((snapshot.clone(), Instant::now()));
    });
}

pub fn shutdown() {
    CONTROLS.with(|slot| {
        if let Some(controls) = slot.borrow_mut().take() {
            // SAFETY: main-thread registrations remain retained until removal.
            unsafe {
                for binding in controls.bindings {
                    let _: () = msg_send![&*binding.command, setEnabled: false];
                    let _: () = msg_send![&*binding.command, removeTarget: &*binding.target];
                }
                let center: Retained<AnyObject> =
                    msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
                let _: () = msg_send![&*center, setNowPlayingInfo: std::ptr::null::<AnyObject>()];
                let _: () = msg_send![&*center, setPlaybackState: 3_usize];
            }
        }
    });
}

#[cfg(feature = "native-diagnostics")]
pub fn diagnose(
    destination: &std::path::Path,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use std::sync::Mutex;
    let received = Arc::new(Mutex::new(Vec::new()));
    let captured = received.clone();
    install(Arc::new(move |action| {
        captured.lock().unwrap().push(action)
    }));
    let mut snapshot = PlaybackSnapshot {
        status: PlaybackStatus::Playing,
        track: Some(vessel_app::provider::Track {
            id: "native-media-fixture".into(),
            title: "Media keys fixture".into(),
            artist: "Fixture".into(),
            album: None,
            duration_seconds: Some(30.0),
            thumbnail_url: None,
        }),
        ..PlaybackSnapshot::default()
    };
    update(&snapshot);
    CONTROLS.with(|slot| {
        for binding in &slot.borrow().as_ref().unwrap().bindings {
            // SAFETY: the live native command exposes its enabled state; these
            // handlers ignore the event and route the same actions used by media keys.
            assert!(unsafe { msg_send![&*binding.command, isEnabled] });
            assert_eq!(binding.handler.call((std::ptr::null_mut(),)), 0);
        }
    });
    assert_eq!(
        *received.lock().unwrap(),
        [
            Action::Previous,
            Action::Toggle,
            Action::Next,
            Action::Play,
            Action::Pause
        ]
    );
    snapshot.status = PlaybackStatus::Paused;
    update(&snapshot);
    // SAFETY: inspect actual native Now Playing state, not a renderer mock.
    unsafe {
        let center: Retained<AnyObject> = msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
        let state: usize = msg_send![&*center, playbackState];
        assert_eq!(state, 2);
    }
    update(&PlaybackSnapshot::default());
    CONTROLS.with(|slot| {
        for binding in &slot.borrow().as_ref().unwrap().bindings {
            // SAFETY: read native state while its command remains retained.
            let enabled: bool = unsafe { msg_send![&*binding.command, isEnabled] };
            assert!(!enabled);
        }
    });
    shutdown();
    std::fs::write(
        destination,
        "{\"passed\":true,\"registeredCommands\":5,\"nativeNowPlaying\":true,\"physicalKeyboardTested\":false}\n",
    )?;
    Ok(())
}
