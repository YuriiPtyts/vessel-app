use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use vessel_app::{
    application::Application,
    catalog::{MusicCatalog, SearchKind},
    playback::PlaybackStatus,
    runtime::RuntimePaths,
};

#[tokio::test]
#[ignore = "queries real guest recommendations and opens a recommended collection"]
async fn home_feed_uses_provider_shelves_and_opens_real_playlists() {
    let catalog = MusicCatalog::new().unwrap();
    let feed = catalog.home(false).await.unwrap();
    assert!(!feed.personalized);
    assert!(!feed.shelves.is_empty());
    for shelf in &feed.shelves {
        println!(
            "YouTube Home shelf: {} ({} items)",
            shelf.title,
            shelf.items.len()
        );
    }
    assert!(
        feed.shelves
            .iter()
            .flat_map(|shelf| &shelf.items)
            .any(|item| item.track.is_some())
    );
    let collection = feed
        .shelves
        .iter()
        .flat_map(|shelf| &shelf.items)
        .find(|item| item.track.is_none())
        .unwrap();
    let page = catalog.collection(&collection.id, false).await.unwrap();
    println!(
        "Recommended collection: {} ({} tracks)",
        page.item.title,
        page.tracks.len()
    );
    assert!(!page.tracks.is_empty());
    assert!(
        page.tracks
            .iter()
            .any(|track| track.duration_seconds.is_some())
    );
}

#[tokio::test]
#[ignore = "queries the real guest YouTube Music catalog"]
async fn music_search_returns_metadata_and_paginates_without_duplicates() {
    let catalog = MusicCatalog::new().unwrap();
    let start = Instant::now();
    let first = catalog
        .search("Daft Punk", SearchKind::Songs, false)
        .await
        .unwrap();
    println!(
        "Music catalog: {} songs in {:?}; more: {}",
        first.tracks.len(),
        start.elapsed(),
        first.has_more
    );
    assert!(!first.tracks.is_empty());
    assert!(first.tracks.iter().any(|track| track.album.is_some()));
    assert!(
        first
            .tracks
            .iter()
            .any(|track| track.thumbnail_url.is_some())
    );
    let artwork_url = first
        .tracks
        .iter()
        .find_map(|track| track.thumbnail_url.as_deref())
        .unwrap();
    let image = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .get(artwork_url)
        .send()
        .await
        .unwrap();
    assert!(image.status().is_success());
    assert!(
        image
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("image/")
    );
    if first.has_more {
        let next = catalog
            .search("Daft Punk", SearchKind::Songs, true)
            .await
            .unwrap();
        println!("After pagination: {} songs", next.tracks.len());
        assert!(next.tracks.len() > first.tracks.len());
        let ids: std::collections::BTreeSet<_> =
            next.tracks.iter().map(|track| &track.id).collect();
        assert_eq!(ids.len(), next.tracks.len());
    }
    let videos = catalog
        .search("Daft Punk", SearchKind::Videos, false)
        .await
        .unwrap();
    assert!(!videos.tracks.is_empty());
    println!("Music videos: {}", videos.tracks.len());
}

#[tokio::test]
#[ignore = "uses real catalog, stream resolution, and muted CoreAudio output"]
async fn a_search_result_plays_without_a_link_or_inspection_step() {
    let paths = std::env::var_os("VESSEL_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime"));
    let app = Application::new(RuntimePaths::new(paths), |_| {}).unwrap();
    app.volume(0.0).await.unwrap();
    let results = app
        .search("Daft Punk Instant Crush", SearchKind::Songs, false)
        .await
        .unwrap();
    let track = results.tracks.first().unwrap();
    let selection_started = Instant::now();
    let selected = app.play_song(&track.id).await.unwrap();
    println!(
        "Stream selection/load acknowledgement: {:?}",
        selection_started.elapsed()
    );
    assert_eq!(selected.inspection.track.title, track.title);
    assert_eq!(selected.inspection.track.thumbnail_url, track.thumbnail_url);
    let start = Instant::now();
    loop {
        let playback = app.snapshot().unwrap();
        assert_ne!(
            playback.status,
            PlaybackStatus::Failed,
            "{:?}",
            playback.error
        );
        if playback.status == PlaybackStatus::Playing && playback.position_seconds > 0.5 {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(30));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!(
        "Search-result click to decoded output: {:?}",
        selection_started.elapsed()
    );
    assert!(
        selection_started.elapsed() < Duration::from_secs(10),
        "first-play latency regressed"
    );
    assert!(app.snapshot().unwrap().decoded_sample_rate_hz.is_some());
    assert_eq!(app.toggle_like(&track.id).unwrap().liked[0].id, track.id);
    assert_eq!(app.library().unwrap().recent[0].id, track.id);
    app.stop().await.unwrap();
    let repeat_started = Instant::now();
    app.play_song(&track.id).await.unwrap();
    loop {
        let snapshot = app.snapshot().unwrap();
        assert_ne!(
            snapshot.status,
            PlaybackStatus::Failed,
            "{:?}",
            snapshot.error
        );
        if snapshot.status == PlaybackStatus::Playing && snapshot.position_seconds > 0.5 {
            break;
        }
        assert!(repeat_started.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!(
        "Cached repeat to decoded output: {:?}",
        repeat_started.elapsed()
    );
    assert!(repeat_started.elapsed() < Duration::from_secs(3));
    app.stop().await.unwrap();
    app.shutdown();
}
