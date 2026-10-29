use super::*;
use crate::account::{Account, AccountMode, AccountProfile, CookieSession, SessionCookie};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
};

struct Request {
    method: String,
    url: Url,
    body: Value,
    authorization: Option<String>,
}
struct Server {
    origin: String,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<Request>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(handler: impl Fn(&Request) -> (u16, Value) + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let serve = Arc::new(move |mut stream: TcpStream| {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let (end, length) = loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|chunk| chunk == b"\r\n\r\n") {
                    let text = std::str::from_utf8(&bytes[..end]).unwrap();
                    let length = text
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    break (end + 4, length);
                }
            };
            while bytes.len() < end + length {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
            }
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let mut line = headers.lines().next().unwrap().split_whitespace();
            let method = line.next().unwrap().into();
            let url = Url::parse(&format!("http://localhost{}", line.next().unwrap())).unwrap();
            let authorization = headers.lines().find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                    .map(|(_, value)| value.trim().to_owned())
            });
            let request = Request {
                method,
                url,
                body: if length == 0 {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes[end..end + length]).unwrap()
                },
                authorization,
            };
            let (status, body) = handler(&request);
            recorded.lock().unwrap().push(request);
            let bytes = if status == 204 {
                Vec::new()
            } else {
                serde_json::to_vec(&body).unwrap()
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            let _ = stream.write_all(&bytes);
        });
        let thread = std::thread::spawn(move || {
            let mut workers = Vec::new();
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let serve = serve.clone();
                workers.push(std::thread::spawn(move || serve(stream.unwrap())));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            origin,
            stop,
            requests,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.origin.trim_start_matches("http://"));
        self.thread.take().unwrap().join().unwrap();
    }
}
fn cookie() -> SessionCookie {
    SessionCookie {
        domain: ".soundcloud.com".into(),
        path: "/".into(),
        name: "oauth_token".into(),
        value: "fixture-soundcloud-token".into(),
        secure: true,
        http_only: true,
        expires_at: None,
    }
}
fn session() -> auth::Session {
    auth::Session {
        token: cookie().value,
        user_id: 42,
        profile: AccountProfile {
            name: "Cloud fixture".into(),
            email: None,
            picture_url: None,
        },
        cookies: vec![cookie()],
    }
}
fn song(id: u64) -> Value {
    json!({"id":id,"kind":"track","title":format!("Cloud song {id}"),"user":{"id":42,"username":"Fixture Creator","permalink_url":"https://soundcloud.com/fixture"},"duration":180000,"full_duration":180000,"policy":"ALLOW","streamable":true,"public":true,"permalink_url":format!("https://soundcloud.com/fixture/track-{id}"),"artwork_url":"https://i1.sndcdn.com/fixture-large.jpg","media":{"transcodings":[{"url":"https://api-v2.soundcloud.com/media/fixture","preset":"aac_160k","snipped":false,"format":{"protocol":"hls","mime_type":"audio/mp4"}}]}})
}
fn playlist(id: u64) -> Value {
    json!({"id":id,"kind":"playlist","title":format!("Playlist {id}"),"user":{"id":42,"username":"Fixture"},"track_count":2,"tracks":[{"id":1},{"id":2}]})
}
fn fixture(server: &Server) -> Arc<SoundCloud> {
    fixture_with_emit(server, Arc::new(|_| {}))
}
fn fixture_with_emit(
    server: &Server,
    emit: Arc<dyn Fn(AccountLibrarySnapshot) + Send + Sync>,
) -> Arc<SoundCloud> {
    let mut cloud = SoundCloud::new(Arc::new(MemoryVault::default()), emit).unwrap();
    let auth = cloud.auth.clone();
    Arc::get_mut(&mut cloud).unwrap().api = client::Api::fixture(auth, server.origin.clone());
    cloud
}
async fn connect(cloud: &Arc<SoundCloud>) -> u64 {
    let id = cloud.auth.begin(false);
    cloud.auth.activate(id, session(), false).await.unwrap();
    cloud.invalidate();
    cloud.account().revision
}

#[test]
fn soundcloud_ids_artwork_and_restrictions_are_provider_scoped() {
    let parsed = parse_track(&song(12)).unwrap();
    assert_eq!(parsed.id, "sc:12");
    assert_eq!(parsed.duration_seconds, Some(180.0));
    assert!(parsed.thumbnail_url.is_some());
    assert!(!crate::catalog::valid_video_id(&parsed.id));
    for id in ["sc:0", "sc:-1", "sc:1/../../x", "sc:1?token=secret", "12"] {
        assert!(track_id(id).is_none());
    }
    for policy in ["BLOCK", "SNIP"] {
        let mut data = song(1);
        data["policy"] = json!(policy);
        assert!(parse_track(&data).is_none());
    }
    let mut preview = song(1);
    preview["access"] = json!("preview");
    assert!(parse_track(&preview).is_none());
    let mut transcoding = song(1)["media"]["transcodings"][0].clone();
    transcoding["snipped"] = json!(true);
    assert!(parse_transcoding(&transcoding).is_none());
    transcoding["snipped"] = json!(false);
    transcoding["format"]["protocol"] = json!("ctr-hls");
    assert!(parse_transcoding(&transcoding).is_none());
    assert!(
        public_page("https://soundcloud.com/artist/song?tracking=removed")
            .unwrap()
            .ends_with("/artist/song")
    );
    assert!(public_page("https://soundcloud.com.evil.test/artist/song").is_none());
    assert!(next_page(&json!({"next_href":"https://evil.test/?oauth_token=secret"})).is_err());
}

#[tokio::test]
async fn verified_search_and_streams_keep_tokens_and_signed_sources_out_of_public_results() {
    let server = Server::new(|request| {
        assert_eq!(
            request.authorization.as_deref(),
            Some("OAuth fixture-soundcloud-token")
        );
        assert!(
            request
                .url
                .query_pairs()
                .any(|(key, value)| key == "client_id" && value == "fixture-client")
        );
        (
            200,
            match request.url.path() {
                "/me" => json!({"id":42,"username":"Cloud fixture"}),
                "/search/tracks" => {
                    json!({"collection":[song(1)],"next_href":"https://api-v2.soundcloud.com/search/tracks?offset=25&client_id=private-client"})
                }
                "/tracks/1" => song(1),
                "/media/fixture" => {
                    json!({"url":"https://cf-hls-media.sndcdn.com/fixture/master.m3u8?Policy=private-signed-source"})
                }
                _ => panic!("unexpected fixture endpoint"),
            },
        )
    });
    let cloud = fixture(&server);
    let verified = cloud
        .verify("fixture-soundcloud-token", vec![cookie()])
        .await
        .unwrap();
    let id = cloud.auth.begin(false);
    cloud.auth.activate(id, verified, false).await.unwrap();
    cloud.invalidate();
    let search = cloud
        .search("fixture music", SearchKind::Songs, false)
        .await
        .unwrap();
    assert_eq!(search.tracks[0].id, "sc:1");
    assert!(search.has_more);
    let resolved = cloud.resolve("sc:1").await.unwrap();
    let public = serde_json::to_string(&(search, &resolved.inspection)).unwrap();
    for private in [
        "fixture-soundcloud-token",
        "private-client",
        "private-signed-source",
        "next_href",
    ] {
        assert!(!public.contains(private));
    }
    let (_, format, source) = resolved
        .source("sc:1", "hls_aac_160k", cloud.account().revision)
        .unwrap();
    assert_eq!(format.bitrate_kbps, Some(160.0));
    assert!(source.headers.is_empty());
    assert!(resolved.source("sc:1", "hls_aac_160k", 99).is_err());
    let count = server.requests.lock().unwrap().len();
    cloud.resolve("sc:1").await.unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), count);
}

#[tokio::test]
async fn connected_library_publishes_likes_without_waiting_for_slow_playlists() {
    let server = Server::new(|request| match request.url.path() {
        "/users/42/likes" => (200, json!({"collection":[{"track":song(3)}]})),
        "/users/42/playlists" => {
            std::thread::sleep(Duration::from_secs(2));
            (200, json!({"collection":[playlist(50)]}))
        }
        _ => panic!("unexpected account-library request"),
    });
    let (updates, mut receiver) =
        tokio::sync::watch::channel(AccountLibrarySnapshot::empty(0, 0, false));
    let cloud = fixture_with_emit(
        &server,
        Arc::new(move |snapshot| {
            updates.send_replace(snapshot);
        }),
    );
    let revision = connect(&cloud).await;
    let work = cloud.clone();
    let refresh = tokio::spawn(async move { work.refresh_library(true, revision).await });
    let ready = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            receiver.changed().await.unwrap();
            let snapshot = receiver.borrow_and_update().clone();
            if snapshot.likes_ready {
                break snapshot;
            }
        }
    })
    .await
    .expect("Connected SoundCloud likes must appear while playlists are still loading");
    assert_eq!(ready.liked_ids, ["sc:3"]);
    assert!(ready.refreshing);
    assert!(!ready.playlists_ready);
    let complete = refresh.await.unwrap().unwrap();
    assert!(complete.playlists_ready && !complete.refreshing);
    assert_eq!(complete.playlists[0].id, "sc:playlist:50");
}

#[tokio::test]
async fn connected_library_keeps_loaded_likes_and_reports_a_redacted_playlist_http_failure() {
    let server = Server::new(|request| match request.url.path() {
        "/users/42/likes" => (200, json!({"collection":[{"track":song(3)}]})),
        "/users/42/playlists" => (
            403,
            json!({"error":"https://private.test/?token=must-not-leak"}),
        ),
        _ => panic!("unexpected account-library request"),
    });
    let cloud = fixture(&server);
    let revision = connect(&cloud).await;
    let library = cloud.refresh_library(true, revision).await.unwrap();
    assert_eq!(library.liked_ids, ["sc:3"]);
    assert!(library.likes_ready && !library.refreshing && !library.playlists_ready);
    let error = library.playlists_error.unwrap();
    assert_eq!(error.code, "soundcloud_http");
    assert!(error.message.contains("403"));
    assert!(!error.message.contains("private.test") && !error.message.contains("must-not-leak"));
    assert_eq!(cloud.account().mode, AccountMode::Connected);
}

#[tokio::test]
async fn a_late_library_partition_cannot_repopulate_a_forgotten_account() {
    let server = Server::new(|request| match request.url.path() {
        "/users/42/likes" => (200, json!({"collection":[{"track":song(3)}]})),
        "/users/42/playlists" => {
            std::thread::sleep(Duration::from_secs(2));
            (200, json!({"collection":[playlist(50)]}))
        }
        _ => panic!("unexpected account-library request"),
    });
    let (updates, mut receiver) =
        tokio::sync::watch::channel(AccountLibrarySnapshot::empty(0, 0, false));
    let cloud = fixture_with_emit(
        &server,
        Arc::new(move |snapshot| {
            updates.send_replace(snapshot);
        }),
    );
    let revision = connect(&cloud).await;
    let work = cloud.clone();
    let refresh = tokio::spawn(async move { work.refresh_library(true, revision).await });
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            receiver.changed().await.unwrap();
            if receiver.borrow_and_update().likes_ready {
                break;
            }
        }
    })
    .await
    .unwrap();
    cloud.auth.sign_out().await.unwrap();
    cloud.invalidate();
    assert_eq!(refresh.await.unwrap().unwrap_err().code, "cancelled");
    let library = cloud.library().unwrap();
    assert!(!library.connected && library.liked.is_empty() && library.playlists.is_empty());
}

#[tokio::test]
async fn playlist_addition_preserves_complete_order_and_does_not_retry_failed_writes() {
    let server = Server::new(|request| {
        assert!(request.authorization.is_some());
        match (request.method.as_str(), request.url.path()) {
            ("GET", "/users/42/likes") => (200, json!({"collection":[{"track":song(3)}]})),
            ("GET", "/users/42/playlists") => (200, json!({"collection":[playlist(50)]})),
            ("GET", "/playlists/50") => (200, playlist(50)),
            ("PUT", "/playlists/50") => {
                assert_eq!(
                    request.body["playlist"]["tracks"],
                    json!([{"id":1},{"id":2},{"id":3}])
                );
                let mut data = playlist(50);
                data["tracks"] = request.body["playlist"]["tracks"].clone();
                (200, data)
            }
            ("POST", "/playlists") => {
                assert_eq!(request.body["playlist"]["sharing"], "private");
                let mut data = playlist(51);
                data["title"] = request.body["playlist"]["title"].clone();
                (200, data)
            }
            _ => panic!("unexpected endpoint"),
        }
    });
    let cloud = fixture(&server);
    let revision = connect(&cloud).await;
    let library = cloud.refresh_library(true, revision).await.unwrap();
    assert_eq!(library.editable_playlist_ids, ["sc:playlist:50"]);
    assert_eq!(library.liked_ids, ["sc:3"]);
    cloud
        .add_to_playlist("sc:playlist:50", "sc:3", revision)
        .await
        .unwrap();
    cloud
        .create_playlist(
            "New playlist",
            "",
            PlaylistPrivacy::Private,
            Some("sc:3"),
            revision,
        )
        .await
        .unwrap();
    assert!(
        cloud
            .create_playlist("Name", "", PlaylistPrivacy::Unlisted, None, revision)
            .await
            .is_err()
    );
    assert_eq!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.method == "PUT")
            .count(),
        1
    );
}

#[tokio::test]
async fn incomplete_playlist_reads_are_never_written_back_and_failed_like_acknowledgements_stay_errors()
 {
    let server = Server::new(|request| match request.url.path() {
        "/users/42/likes" => (200, json!({"collection":[]})),
        "/users/42/playlists" => (200, json!({"collection":[playlist(50)]})),
        "/playlists/50" => {
            assert_eq!(request.method, "GET");
            let mut data = playlist(50);
            data["track_count"] = json!(100);
            (200, data)
        }
        "/users/42/track_likes/3" => (200, json!({"errors":[{"error_message":"fixture failure"}]})),
        _ => panic!("unexpected request"),
    });
    let cloud = fixture(&server);
    let revision = connect(&cloud).await;
    cloud.refresh_library(true, revision).await.unwrap();
    assert!(
        cloud
            .add_to_playlist("sc:playlist:50", "sc:3", revision)
            .await
            .is_err()
    );
    assert!(
        cloud
            .set_like("sc:3", parse_track(&song(3)).unwrap(), true, revision)
            .await
            .is_err()
    );
    assert!(cloud.library().unwrap().liked_ids.is_empty());
    assert!(
        !server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| request.method == "PUT" && request.url.path().contains("playlists"))
    );
}

#[tokio::test]
async fn independent_vaults_keep_youtube_connected_when_soundcloud_is_forgotten() {
    let google_vault = Arc::new(MemoryVault::default());
    let youtube = Account::new(google_vault.clone());
    let google = CookieSession::from_cookies(vec![SessionCookie {
        domain: ".youtube.com".into(),
        path: "/".into(),
        name: "SAPISID".into(),
        value: "fixture-google-token".into(),
        secure: true,
        http_only: true,
        expires_at: None,
    }])
    .unwrap();
    youtube
        .activate(
            youtube.begin(false),
            google,
            AccountProfile {
                name: "YT fixture".into(),
                email: None,
                picture_url: None,
            },
            true,
        )
        .await
        .unwrap();
    let original = google_vault.load().unwrap();
    let cloud_vault = Arc::new(MemoryVault::default());
    let cloud = auth::Auth::new(cloud_vault.clone());
    let id = cloud.begin(false);
    cloud.activate(id, session(), true).await.unwrap();
    assert!(cloud.saved().await.unwrap().is_some());
    cloud.sign_out().await.unwrap();
    assert!(cloud_vault.load().unwrap().is_none());
    assert_eq!(google_vault.load().unwrap(), original);
    assert_eq!(youtube.snapshot().mode, AccountMode::Connected);
    assert!(youtube.request_session().1.is_some());
    let id = cloud.begin(false);
    cloud.fail(id, unavailable());
    cloud.cancel(id);
    assert_eq!(cloud.snapshot().mode, AccountMode::Unavailable);
    assert!(cloud.snapshot().error.is_some());
}

#[test]
fn browser_cookie_candidates_do_not_accept_google_or_unrelated_credentials() {
    let mut wrong = cookie();
    wrong.domain = ".youtube.com".into();
    assert!(auth::token_from_cookies(vec![wrong]).is_none());
    let mut wrong = cookie();
    wrong.domain = "soundcloud.com.evil.test".into();
    assert!(auth::token_from_cookies(vec![wrong]).is_none());
    assert!(auth::token_from_cookies(vec![cookie()]).is_some());
}

#[tokio::test]
async fn cancellation_during_soundcloud_vault_storage_rolls_back_without_connecting() {
    struct Vault {
        value: Mutex<Option<Vec<u8>>>,
        entered: tokio::sync::Notify,
        gate: (Mutex<bool>, std::sync::Condvar),
    }
    impl SessionVault for Vault {
        fn load(&self) -> Result<Option<Vec<u8>>> {
            Ok(self.value.lock().unwrap().clone())
        }
        fn save(&self, bytes: &[u8]) -> Result<()> {
            self.entered.notify_one();
            let mut ready = self.gate.0.lock().unwrap();
            while !*ready {
                ready = self.gate.1.wait(ready).unwrap();
            }
            *self.value.lock().unwrap() = Some(bytes.into());
            Ok(())
        }
        fn delete(&self) -> Result<()> {
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }
    let vault = Arc::new(Vault {
        value: Mutex::new(None),
        entered: tokio::sync::Notify::new(),
        gate: (Mutex::new(false), std::sync::Condvar::new()),
    });
    let account = auth::Auth::new(vault.clone());
    let id = account.begin(false);
    let actor = account.clone();
    let task = tokio::spawn(async move { actor.activate(id, session(), true).await });
    vault.entered.notified().await;
    account.cancel(id);
    *vault.gate.0.lock().unwrap() = true;
    vault.gate.1.notify_one();
    assert!(task.await.unwrap().is_err());
    assert!(vault.load().unwrap().is_none());
    assert_eq!(account.snapshot().mode, AccountMode::Guest);
}

#[tokio::test]
#[ignore = "real public SoundCloud likes/playlist pagination and parsing; no credentials or audio"]
async fn live_soundcloud_public_library_pages_load_and_parse() {
    let cloud = SoundCloud::new(Arc::new(MemoryVault::default()), Arc::new(|_| {})).unwrap();
    let data = cloud
        .api
        .request(
            Method::GET,
            "search/users",
            &[("q", "chillhopmusic".into()), ("limit", "10".into())],
            None,
            0,
        )
        .await
        .unwrap();
    let users = rows(&data).unwrap();
    let user = users
        .iter()
        .find(|user| {
            user.get("likes_count")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0)
                && user
                    .get("playlist_count")
                    .and_then(Value::as_u64)
                    .is_some_and(|count| count > 0)
        })
        .unwrap()
        .get("id")
        .and_then(Value::as_u64)
        .unwrap();
    let likes_path = format!("users/{user}/likes");
    let playlists_path = format!("users/{user}/playlists");
    let (likes, playlists) = tokio::join!(
        cloud.pages(&likes_path, 0, 10_000),
        cloud.pages(&playlists_path, 0, 1_000),
    );
    let (likes, playlists) = (likes.unwrap(), playlists.unwrap());
    let tracks = likes
        .iter()
        .filter_map(|value| parse_track(value.get("track").unwrap_or(value)))
        .count();
    let targets = playlists.iter().filter_map(parse_target).count();
    assert!(tracks > 0 && targets > 0);
    println!(
        "SoundCloud public library: {} like rows, {tracks} playable tracks, {} playlist rows, {targets} parsed playlists; no credentials used",
        likes.len(),
        playlists.len()
    );
}

#[tokio::test]
#[ignore = "real SoundCloud search/discovery/HLS resolution and muted native decoding"]
async fn live_soundcloud_guest_search_and_native_playback() {
    let root = std::env::var_os("VESSEL_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime"));
    let directory = tempfile::tempdir().unwrap();
    let app = crate::application::Application::soundcloud_desktop(
        crate::runtime::RuntimePaths::new(root),
        Arc::new(crate::library::LocalLibrary::memory()),
        directory.path().join("queue.json"),
        true,
        |_| {},
    )
    .unwrap();
    app.volume(0.0).await.unwrap();
    app.autoplay(false).await.unwrap();
    let feed = app.home(false).await.unwrap();
    assert!(!feed.shelves.is_empty());
    let discovery = feed
        .shelves
        .iter()
        .flat_map(|shelf| &shelf.items)
        .find(|item| item.id.starts_with("sc:playlist:"))
        .unwrap();
    let collection = app.collection(&discovery.id, false).await.unwrap();
    assert!(!collection.tracks.is_empty());
    println!(
        "SoundCloud discovery collection loaded {} playable tracks",
        collection.tracks.len()
    );
    let search = app
        .search("nujabes", SearchKind::Songs, false)
        .await
        .unwrap();
    assert!(!search.tracks.is_empty());
    assert!(
        search
            .tracks
            .iter()
            .all(|track| track.id.starts_with("sc:"))
    );
    let related = app
        .soundcloud_service()
        .unwrap()
        .radio(&search.tracks[0].id, None)
        .await
        .unwrap();
    assert!(!related.tracks.is_empty());
    println!("SoundCloud related tracks loaded: {}", related.tracks.len());
    let start = Instant::now();
    let selection = app.play_song(&search.tracks[0].id).await.unwrap();
    for _ in 0..100 {
        let snapshot = app.snapshot().unwrap();
        if snapshot.position_seconds > 0.2 && snapshot.decoded_sample_rate_hz.is_some() {
            println!(
                "SoundCloud live: {} shelves, {} search tracks, {} formats, decoded output in {:.3}s",
                feed.shelves.len(),
                search.tracks.len(),
                selection.inspection.formats.len(),
                start.elapsed().as_secs_f64()
            );
            app.pause(true).await.unwrap();
            app.seek(10.0).await.unwrap();
            app.deactivate().await.unwrap();
            assert_eq!(
                app.snapshot().unwrap().status,
                crate::playback::PlaybackStatus::Paused
            );
            app.activate_provider();
            app.pause(false).await.unwrap();
            app.stop().await.unwrap();
            app.shutdown();
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    app.shutdown();
    panic!("SoundCloud native decoding did not advance");
}
