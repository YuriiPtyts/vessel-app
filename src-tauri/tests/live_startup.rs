use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use vessel_app::{application::Application, playback::PlaybackStatus, runtime::RuntimePaths};

#[tokio::test]
#[ignore = "real first-selection latency; network, bundled runtime and muted native output"]
async fn first_selection_reaches_decoded_output_within_two_seconds() {
    let root = std::env::var_os("VESSEL_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime"));
    let app = Application::new(RuntimePaths::new(root), |_| {}).unwrap();
    app.volume(0.0).await.unwrap();
    app.warm_up().await;
    let start = Instant::now();
    app.play_song("khnokW3Mw24").await.unwrap();
    let resolved = start.elapsed();
    loop {
        let playback = app.snapshot().unwrap();
        assert_ne!(
            playback.status,
            PlaybackStatus::Failed,
            "{:?}",
            playback.error
        );
        if playback.status == PlaybackStatus::Playing
            && playback.decoded_sample_rate_hz.is_some()
            && playback.position_seconds > 0.05
        {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(20));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let elapsed = start.elapsed();
    app.stop().await.unwrap();
    app.shutdown();
    println!(
        "First selection: resolution/load ack {resolved:?}; decoded output {elapsed:?}; native startup {:?}",
        elapsed.saturating_sub(resolved)
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "first-selection latency is still too slow: {elapsed:?}"
    );
}
