use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use vessel_app::{
    application::{AppEvent, Application},
    playback::PlaybackStatus,
    runtime::RuntimePaths,
};

async fn wait_for(app: &Application, condition: impl Fn() -> bool, seconds: u64) {
    let start = Instant::now();
    while !condition() {
        let snapshot = app.snapshot().unwrap();
        assert_ne!(
            snapshot.status,
            PlaybackStatus::Failed,
            "{:?}",
            snapshot.error
        );
        assert!(
            start.elapsed() < Duration::from_secs(seconds),
            "timed out: {:?}, queue: {:?}",
            snapshot,
            app.queue().unwrap()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
#[ignore = "real YouTube radio, source prefetch, natural EOF/autoadvance and muted CoreAudio"]
async fn radio_prefetches_and_advances_at_natural_eof_without_the_renderer() {
    let paths = std::env::var_os("VESSEL_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime"));
    let prepared = Arc::new(Mutex::new((false, false)));
    let markers = prepared.clone();
    let ended_at = Arc::new(Mutex::new(None));
    let ended_marker = ended_at.clone();
    let app = Application::new(RuntimePaths::new(paths), move |event| match event {
        AppEvent::Queue(queue) => {
            let mut state = markers.lock().unwrap();
            if queue.preparing_id.is_some() {
                state.0 = true;
            } else if state.0 {
                state.1 = true;
            }
        }
        AppEvent::Playback(snapshot) if snapshot.status == PlaybackStatus::Ended => {
            *ended_marker.lock().unwrap() = Some(Instant::now());
        }
        _ => {}
    })
    .unwrap();
    app.volume(0.0).await.unwrap();
    app.warm_up().await;
    app.play_song("khnokW3Mw24").await.unwrap();
    wait_for(
        &app,
        || {
            app.snapshot().unwrap().status == PlaybackStatus::Playing
                && app.queue().unwrap().tracks.len() >= 10
        },
        30,
    )
    .await;
    let initial = app.snapshot().unwrap();
    let queue = app.queue().unwrap();
    let next_id = queue.tracks[queue.current_index.unwrap() + 1].id.clone();
    println!(
        "Rust radio queue: {} tracks, autoplay {}",
        queue.tracks.len(),
        queue.autoplay
    );
    app.seek(initial.duration_seconds.unwrap() - 27.0)
        .await
        .unwrap();
    let prefetch_started = Instant::now();
    wait_for(&app, || prepared.lock().unwrap().1, 25).await;
    println!("Next source prepared in {:?}", prefetch_started.elapsed());
    app.seek(initial.duration_seconds.unwrap() - 1.5)
        .await
        .unwrap();
    wait_for(
        &app,
        || {
            let snapshot = app.snapshot().unwrap();
            snapshot
                .track
                .as_ref()
                .is_some_and(|track| track.id == next_id)
                && snapshot.status == PlaybackStatus::Playing
                && snapshot.decoded_sample_rate_hz.is_some()
        },
        20,
    )
    .await;
    assert_ne!(app.snapshot().unwrap().playback_id, initial.playback_id);
    println!(
        "Natural EOF to next decoded source: {:?}",
        ended_at
            .lock()
            .unwrap()
            .expect("natural EOF event")
            .elapsed()
    );
    let advanced = app.snapshot().unwrap().playback_id;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        app.snapshot().unwrap().playback_id,
        advanced,
        "EOF must advance once"
    );
    app.stop().await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(app.snapshot().unwrap().status, PlaybackStatus::Idle);
    assert!(!app.queue().unwrap().running);
    assert!(app.queue().unwrap().tracks.len() >= 10);
    app.shutdown();
}
