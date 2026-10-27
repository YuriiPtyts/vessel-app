use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use vessel_app::{
    application::Application,
    playback::{PlaybackSnapshot, PlaybackStatus},
    runtime::RuntimePaths,
};

async fn wait_for(
    app: &Application,
    condition: impl Fn(&PlaybackSnapshot) -> bool,
) -> PlaybackSnapshot {
    let start = Instant::now();
    loop {
        let snapshot = app.snapshot().unwrap();
        assert_ne!(
            snapshot.status,
            PlaybackStatus::Failed,
            "{:?}",
            snapshot.error
        );
        if condition(&snapshot) {
            return snapshot;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "timed out waiting for playback: {:?}",
            snapshot.status
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
#[ignore = "uses YouTube and the real CoreAudio output, muted; run explicitly"]
async fn guest_stream_decodes_seeks_pauses_and_switches_quality() {
    let root = std::env::var_os("VESSEL_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime"));
    let app = Application::new(RuntimePaths::new(root), |_| {}).unwrap();
    app.volume(0.0).await.unwrap();
    let url = std::env::var("VESSEL_TEST_URL")
        .unwrap_or_else(|_| "https://music.youtube.com/watch?v=dQw4w9WgXcQ".into());
    let started = Instant::now();
    let inspection = app.inspect(&url).await.unwrap();
    println!(
        "Guest inspection: {} audio formats in {:?}",
        inspection.formats.len(),
        started.elapsed()
    );
    assert!(!inspection.formats.is_empty());
    app.play(&inspection.track.id, &inspection.recommended_format_id)
        .await
        .unwrap();
    let playing = wait_for(&app, |snapshot| {
        snapshot.status == PlaybackStatus::Playing && snapshot.position_seconds > 0.5
    })
    .await;
    println!(
        "First decoded output in {:?}; codec: {}; sample rate: {:?}",
        started.elapsed(),
        playing.active_format.as_ref().unwrap().codec,
        playing.decoded_sample_rate_hz
    );
    assert!(!playing.video_track_selected);
    assert!(playing.decoded_sample_rate_hz.is_some());

    app.pause(true).await.unwrap();
    let paused = wait_for(&app, |snapshot| snapshot.status == PlaybackStatus::Paused).await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!((app.snapshot().unwrap().position_seconds - paused.position_seconds).abs() < 0.5);
    app.seek(10.0).await.unwrap();
    wait_for(&app, |snapshot| {
        (snapshot.position_seconds - 10.0).abs() < 0.75
    })
    .await;

    if let Some(alternative) = inspection
        .formats
        .iter()
        .find(|format| format.codec != playing.active_format.as_ref().unwrap().codec)
    {
        app.play(&inspection.track.id, &alternative.id)
            .await
            .unwrap();
        let switched = wait_for(&app, |snapshot| {
            snapshot.status == PlaybackStatus::Paused && snapshot.decoded_sample_rate_hz.is_some()
        })
        .await;
        assert!((switched.position_seconds - 10.0).abs() < 1.0);
        println!(
            "Quality switched while paused: {}",
            switched.active_format.unwrap().codec
        );
    }
    app.pause(false).await.unwrap();
    wait_for(&app, |snapshot| {
        snapshot.status == PlaybackStatus::Playing && snapshot.position_seconds > 10.5
    })
    .await;
    app.stop().await.unwrap();
    assert_eq!(app.snapshot().unwrap().status, PlaybackStatus::Idle);
    app.shutdown();
}

#[tokio::test]
async fn invalid_input_never_starts_the_resolver_or_engine() {
    let app = Application::new(RuntimePaths::new("/nonexistent"), |_| {}).unwrap();
    assert_eq!(
        app.inspect("file:///etc/passwd").await.unwrap_err().code,
        "invalid_url"
    );
    assert_eq!(app.snapshot().unwrap().status, PlaybackStatus::Idle);
    assert_eq!(app.seek(f64::NAN).await.unwrap_err().code, "invalid_seek");
    assert_eq!(app.volume(101.0).await.unwrap_err().code, "invalid_volume");
    app.shutdown();
}

#[tokio::test]
#[ignore = "starts a real provider request then cancels it; run explicitly"]
async fn cancellation_releases_the_resolver_promptly() {
    let paths = RuntimePaths::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime"));
    let app = Application::new(paths, |_| {}).unwrap();
    let mut pending = Box::pin(app.inspect("https://music.youtube.com/watch?v=dQw4w9WgXcQ"));
    // Poll once so the request owns the resolver slot and has begun I/O. A fixed
    // sleep races successful direct resolution now that it can finish in <250ms.
    std::future::poll_fn(|context| {
        use std::{future::Future, task::Poll};
        assert!(pending.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
    app.cancel_inspection();
    let result = tokio::time::timeout(Duration::from_secs(2), pending)
        .await
        .unwrap();
    assert_eq!(result.unwrap_err().code, "cancelled");
    assert_eq!(
        app.inspect("invalid").await.unwrap_err().code,
        "invalid_url"
    );
    app.shutdown();
}
