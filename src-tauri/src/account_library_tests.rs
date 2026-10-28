use super::*;
use crate::account::{AccountProfile, CookieSession, MemoryVault, SessionCookie};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
};

const A: &str = "aaaaaaaaaaa";
const B: &str = "bbbbbbbbbbb";
const C: &str = "ccccccccccc";

struct Request {
    url: url::Url,
    body: Value,
    authenticated: bool,
}
struct Server {
    origin: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new(handler: impl Fn(&Request) -> (u16, Value) + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let recorded = requests.clone();
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let mut stream = stream.unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let (header_end, length) = loop {
                    let mut buffer = [0; 4096];
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|value| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        break (end + 4, length);
                    }
                };
                while bytes.len() < header_end + length {
                    let mut buffer = [0; 4096];
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
                let path = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let request = Request {
                    url: url::Url::parse(&format!("http://localhost{path}")).unwrap(),
                    body: serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap(),
                    authenticated: headers
                        .to_ascii_lowercase()
                        .contains("authorization: sapisidhash")
                        && headers.to_ascii_lowercase().contains("cookie:"),
                };
                let (status, body) = handler(&request);
                recorded.lock().unwrap().push(request);
                let body = serde_json::to_vec(&body).unwrap();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        Self {
            origin,
            requests,
            stop,
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

async fn account() -> Arc<Account> {
    let account = Account::new(Arc::new(MemoryVault::default()));
    let session = CookieSession::from_cookies(
        [("SAPISID", "dummy-session"), ("LOGIN_INFO", "dummy-login")]
            .into_iter()
            .map(|(name, value)| SessionCookie {
                domain: ".youtube.com".into(),
                path: "/".into(),
                name: name.into(),
                value: value.into(),
                secure: true,
                http_only: true,
                expires_at: None,
            })
            .collect(),
    )
    .unwrap();
    account
        .activate(
            account.begin(false),
            session,
            AccountProfile {
                name: "Fixture".into(),
                email: None,
                picture_url: None,
            },
            false,
        )
        .await
        .unwrap();
    account
}

fn song(id: &str) -> Value {
    json!({"musicResponsiveListItemRenderer": {
        "playlistItemData":{"videoId":id},
        "flexColumns":[
            {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":format!("Song {id}")}]}}},
            {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Fixture artist"}]}}}
        ]
    }})
}
fn songs(ids: &[&str]) -> Value {
    json!({"contents":{"musicPlaylistShelfRenderer":{"contents":ids.iter().map(|id| song(id)).collect::<Vec<_>>()}}})
}
fn playlist(id: &str) -> Value {
    json!({"musicTwoRowItemRenderer": {
        "title":{"runs":[{"text":format!("Playlist {id}")}]},
        "navigationEndpoint":{"browseEndpoint":{"browseId":id,"params":"private-params","browseEndpointContextSupportedConfigs":{"browseEndpointContextMusicConfig":{"pageType":"MUSIC_PAGE_TYPE_PLAYLIST"}}}}
    }})
}
fn playlists(ids: &[&str]) -> Value {
    json!({"contents":{"gridRenderer":{"items":ids.iter().map(|id| playlist(id)).collect::<Vec<_>>()}}})
}
fn normal(request: &Request) -> (u16, Value) {
    (
        200,
        match request.body["browseId"].as_str() {
            Some("FEmusic_liked_playlists") => playlists(&["VLLM", "VLsaved"]),
            Some("VLLM") => songs(&[A]),
            Some("VLsaved") => songs(&[B]),
            _ => json!({"responseContext":{}}),
        },
    )
}

#[test]
fn empty_libraries_are_valid_but_changed_payloads_do_not_erase_likes() {
    assert!(parse_liked_page(&songs(&[])).unwrap().ids.is_empty());
    assert!(
        parse_playlists_page(&playlists(&[]))
            .unwrap()
            .targets
            .is_empty()
    );
    for data in [
        json!({}),
        json!({"contents":{}}),
        json!({"continuationContents":{}}),
        json!({"onResponseReceivedActions":[]}),
    ] {
        assert!(parse_liked_page(&data).is_err());
        assert!(parse_playlists_page(&data).is_err());
    }
    let mut hidden = song(C);
    hidden["musicResponsiveListItemRenderer"]["musicItemRendererDisplayPolicy"] =
        json!("MUSIC_ITEM_RENDERER_DISPLAY_POLICY_GREY_OUT");
    let data =
        json!({"contents":{"musicPlaylistShelfRenderer":{"contents":[song(A), song(A), hidden]}}});
    let page = parse_liked_page(&data).unwrap();
    assert_eq!(page.tracks.len(), 1);
    assert_eq!(page.ids, BTreeSet::from([A.into(), C.into()]));
    let mut malformed = songs(&[A]);
    malformed["contents"]["musicPlaylistShelfRenderer"]["continuations"] =
        json!([{"nextContinuationData":{"continuation":"x".repeat(8193)}}]);
    assert!(parse_liked_page(&malformed).is_err());
}

#[tokio::test]
async fn authenticated_pagination_and_saved_playlist_cursors_stay_private() {
    let account = account().await;
    let server = Server::new(|request| {
        let continuation = request.body["continuation"].as_str();
        if continuation == Some("private-likes-next") {
            assert!(request.body.get("browseId").is_none());
            assert!(!request.url.query_pairs().any(|(name, _)| name == "ctoken"));
            return (
                200,
                json!({"onResponseReceivedActions":[{"appendContinuationItemsAction":{"continuationItems":[song(A),song(B)]}}]}),
            );
        }
        if request
            .url
            .query_pairs()
            .any(|(name, value)| name == "ctoken" && value == "private-playlists-next")
        {
            return (
                200,
                json!({"continuationContents":{"gridContinuation":{"items":[playlist("VLsaved"),playlist("VLsecond")]}}}),
            );
        }
        if continuation == Some("private-songs-next") {
            return (
                200,
                json!({"onResponseReceivedActions":[{"appendContinuationItemsAction":{"continuationItems":[song(B),song(C)]}}]}),
            );
        }
        let (status, mut data) = normal(request);
        match request.body["browseId"].as_str() {
            Some("VLLM") => data["contents"]["musicPlaylistShelfRenderer"]["contents"].as_array_mut().unwrap().push(json!({"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"private-likes-next","request":"CONTINUATION_REQUEST_TYPE_BROWSE"}}}})),
            Some("FEmusic_liked_playlists") => data["contents"]["gridRenderer"]["continuations"] = json!([{"nextContinuationData":{"continuation":"private-playlists-next"}}]),
            Some("VLsaved") => {
                assert_eq!(request.body["params"], "private-params");
                data["contents"]["musicPlaylistShelfRenderer"]["contents"].as_array_mut().unwrap().push(json!({"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"private-songs-next"}}}}));
            }
            _ => {}
        }
        (status, data)
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    let revision = account.snapshot().revision;
    let snapshot = library.refresh(&catalog, true, revision).await.unwrap();
    assert!(snapshot.likes_ready && snapshot.playlists_ready);
    assert_eq!(snapshot.liked_ids.len(), 2);
    assert_eq!(snapshot.playlists.len(), 2);
    let first = library
        .collection(&catalog, "VLsaved", false, revision)
        .await
        .unwrap();
    assert!(first.has_more);
    let second = library
        .collection(&catalog, "VLsaved", true, revision)
        .await
        .unwrap();
    assert_eq!(second.tracks.len(), 2);
    assert!(!second.has_more);
    assert!(library.find_track(C).is_some());
    let serialized = serde_json::to_string(&(snapshot, second)).unwrap();
    for secret in ["private-", "dummy-session", "navigationEndpoint"] {
        assert!(!serialized.contains(secret));
    }
    assert!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.authenticated)
    );
}

#[tokio::test]
async fn ratings_use_explicit_desired_state_and_failures_keep_last_good_likes() {
    let account = account().await;
    let reject = Arc::new(AtomicBool::new(false));
    let rejected = reject.clone();
    let server = Server::new(move |request| {
        if request.url.path().contains("/like/") {
            assert_eq!(request.body["target"]["videoId"], A);
            if rejected.load(Ordering::SeqCst) {
                return (200, json!({"responseContext":{},"status":"STATUS_FAILED"}));
            }
        }
        normal(request)
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    let revision = account.snapshot().revision;
    let original = library.refresh(&catalog, true, revision).await.unwrap();
    let track = original.liked[0].clone();
    let unliked = library
        .set_like(&catalog, track.clone(), false, revision)
        .await
        .unwrap();
    assert!(unliked.liked.is_empty());
    library
        .set_like(&catalog, track.clone(), false, revision)
        .await
        .unwrap();
    let liked = library
        .set_like(&catalog, track.clone(), true, revision)
        .await
        .unwrap();
    reject.store(true, Ordering::SeqCst);
    assert!(
        library
            .set_like(&catalog, track, false, revision)
            .await
            .is_err()
    );
    assert_eq!(library.snapshot().unwrap().liked_ids, liked.liked_ids);
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().ends_with("like/removelike"))
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().ends_with("like/like"))
            .count(),
        1
    );
}

#[tokio::test]
async fn partial_refresh_retains_good_data_and_retry_reconciles_remote_changes() {
    let account = account().await;
    let phase = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let state = phase.clone();
    let server = Server::new(move |request| {
        if request.body["browseId"] == "VLLM" {
            match state.load(Ordering::SeqCst) {
                1 => return (200, json!({"contents":{}})),
                2 => return (200, songs(&[B])),
                _ => {}
            }
        }
        normal(request)
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    let revision = account.snapshot().revision;
    library.refresh(&catalog, true, revision).await.unwrap();
    library.refresh(&catalog, false, revision).await.unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    phase.store(1, Ordering::SeqCst);
    let partial = library.refresh(&catalog, true, revision).await.unwrap();
    assert!(partial.likes_error.is_some());
    assert!(partial.playlists_ready && partial.likes_ready);
    assert_eq!(partial.liked_ids, [A]);
    phase.store(2, Ordering::SeqCst);
    let refreshed = library.refresh(&catalog, true, revision).await.unwrap();
    assert_eq!(refreshed.liked_ids, [B]);
    assert!(refreshed.likes_error.is_none());
}

#[tokio::test]
async fn repeated_continuations_fail_instead_of_publishing_an_incomplete_library() {
    let account = account().await;
    let server = Server::new(|request| {
        if request.body["browseId"] == "VLLM" {
            let mut data = songs(&[A]);
            data["contents"]["musicPlaylistShelfRenderer"]["continuations"] =
                json!([{"nextContinuationData":{"continuation":"repeating"}}]);
            return (200, data);
        }
        normal(request)
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    let snapshot = library
        .refresh(&catalog, true, account.snapshot().revision)
        .await
        .unwrap();
    assert!(!snapshot.likes_ready && snapshot.likes_error.is_some());
    assert!(snapshot.liked_ids.is_empty());
    assert!(snapshot.playlists_ready);
}

#[tokio::test]
async fn sign_out_discards_late_refresh_and_rejects_old_playlist_targets() {
    let account = account().await;
    let entered = Arc::new(tokio::sync::Notify::new());
    let signal = entered.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let server = Server::new(move |request| {
        if request.body["browseId"] == "VLLM" {
            signal.notify_one();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        normal(request)
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    let revision = account.snapshot().revision;
    let refresh = library.refresh(&catalog, true, revision);
    let switch = async {
        entered.notified().await;
        account.sign_out().await.unwrap();
        library.synchronize_account().unwrap();
        release.send(()).unwrap();
    };
    let (result, _) = tokio::join!(refresh, switch);
    assert!(result.is_err());
    let snapshot = library.snapshot().unwrap();
    assert!(!snapshot.connected && !snapshot.refreshing);
    assert!(snapshot.liked.is_empty() && snapshot.playlists.is_empty());
    assert!(
        library
            .collection(&catalog, "VLsaved", false, revision)
            .await
            .is_err()
    );
    assert!(library.refresh(&catalog, false, revision).await.is_err());
}

#[tokio::test]
async fn an_expired_authenticated_response_clears_account_library_state() {
    let account = account().await;
    let server = Server::new(|_| (401, json!({})));
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    assert!(
        library
            .refresh(&catalog, true, account.snapshot().revision)
            .await
            .is_err()
    );
    assert_eq!(account.snapshot().mode, AccountMode::Expired);
    assert!(!library.snapshot().unwrap().connected);
}

#[tokio::test]
async fn a_late_rating_acknowledgement_cannot_repopulate_a_signed_out_library() {
    let account = account().await;
    let entered = Arc::new(tokio::sync::Notify::new());
    let signal = entered.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let server = Server::new(move |request| {
        if request.url.path().contains("/like/") {
            signal.notify_one();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        normal(request)
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    let revision = account.snapshot().revision;
    let snapshot = library.refresh(&catalog, true, revision).await.unwrap();
    let rating = library.set_like(&catalog, snapshot.liked[0].clone(), false, revision);
    let switch = async {
        entered.notified().await;
        account.sign_out().await.unwrap();
        library.synchronize_account().unwrap();
        release.send(()).unwrap();
    };
    let (result, _) = tokio::join!(rating, switch);
    assert!(result.is_err());
    let snapshot = library.snapshot().unwrap();
    assert!(!snapshot.connected);
    assert!(snapshot.liked_ids.is_empty());
}

#[tokio::test]
async fn guests_cannot_send_account_library_requests() {
    let account = Account::new(Arc::new(MemoryVault::default()));
    let server = Server::new(|_| panic!("guest request must not reach transport"));
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account, Arc::new(|_| {}));
    assert_eq!(
        library.refresh(&catalog, true, 0).await.unwrap_err().code,
        "account_required"
    );
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn rating_dialogs_and_error_statuses_are_not_successful_writes() {
    for data in [
        json!({}),
        json!({"responseContext":{},"error":{}}),
        json!({"responseContext":{},"status":"STATUS_FAILED"}),
        json!({"responseContext":{},"actions":[{"confirmDialogRenderer":{}}]}),
    ] {
        assert!(validate_rating_response(&data).is_err());
    }
    assert!(validate_rating_response(&json!({"responseContext":{}})).is_ok());
    assert!(
        validate_rating_response(&json!({"responseContext":{},"status":"STATUS_SUCCEEDED"}))
            .is_ok()
    );
}

#[tokio::test]
async fn playlist_creation_and_additions_are_authenticated_acknowledged_and_owned() {
    let account = account().await;
    let server = Server::new(|request| {
        assert!(request.authenticated);
        match request.url.path() {
            "/youtubei/v1/playlist/create" => {
                assert_eq!(request.body["title"], "Evening");
                assert_eq!(request.body["privacyStatus"], "PRIVATE");
                assert_eq!(request.body["videoIds"], json!([A]));
                (200, json!({"responseContext":{},"playlistId":"PLcreated"}))
            }
            "/youtubei/v1/browse/edit_playlist" => {
                assert_eq!(request.body["playlistId"], "PLowned");
                assert_eq!(
                    request.body["actions"],
                    json!([{"action":"ACTION_ADD_VIDEO","addedVideoId":A}])
                );
                (
                    200,
                    json!({"responseContext":{},"status":"STATUS_SUCCEEDED"}),
                )
            }
            _ if request.body["browseId"] == "FEmusic_liked_playlists" => {
                let mut owned = playlist("VLPLowned");
                owned["musicTwoRowItemRenderer"]["menu"] = json!({"menuRenderer":{"items":[{"menuNavigationItemRenderer":{"navigationEndpoint":{"playlistEditorEndpoint":{"playlistId":"PLowned"}}}}]}});
                (
                    200,
                    json!({"contents":{"gridRenderer":{"items":[owned,playlist("VLPLsaved")]}}}),
                )
            }
            _ => normal(request),
        }
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    let revision = account.snapshot().revision;
    let snapshot = library.refresh(&catalog, true, revision).await.unwrap();
    assert_eq!(snapshot.editable_playlist_ids, ["VLPLowned"]);
    let track = snapshot.liked[0].clone();
    let created = library
        .create_playlist(
            &catalog,
            " Evening ",
            "",
            PlaylistPrivacy::Private,
            Some(track.clone()),
            revision,
        )
        .await
        .unwrap();
    assert_eq!(created.playlist.id, "VLPLcreated");
    assert!(
        created
            .library
            .editable_playlist_ids
            .contains(&created.playlist.id)
    );
    let target = library.target("VLPLcreated", revision).unwrap();
    assert_eq!(target.item.title, "Evening");
    let empty = parse_saved_collection(
        &json!({"contents":{"musicPlaylistShelfRenderer":{}}}),
        &target,
    )
    .unwrap()
    .0;
    assert!(empty.tracks.is_empty() && !empty.has_more);
    assert!(parse_saved_collection(&json!({"contents":{}}), &target).is_err());
    assert_eq!(
        library
            .add_to_playlist(&catalog, "VLPLsaved", track.clone(), revision)
            .await
            .unwrap_err()
            .code,
        "playlist_read_only"
    );
    assert!(
        library
            .add_to_playlist(&catalog, "VLPLowned", track, revision)
            .await
            .is_ok()
    );
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().contains("edit_playlist"))
            .count(),
        1
    );
    assert!(
        !serde_json::to_string(&created)
            .unwrap()
            .contains("private-params")
    );
}

#[tokio::test]
async fn playlist_writes_reject_bad_names_guests_and_unconfirmed_responses_without_retry() {
    for response in [
        json!({}),
        json!({"responseContext":{},"playlistId":"../../bad"}),
        json!({"responseContext":{},"playlistId":"PLbad","actions":[{"confirmDialogRenderer":{}}]}),
    ] {
        let account = account().await;
        let server = Server::new(move |_| (200, response.clone()));
        let catalog =
            MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
        let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
        let revision = account.snapshot().revision;
        assert!(
            library
                .create_playlist(
                    &catalog,
                    "<bad>",
                    "",
                    PlaylistPrivacy::Private,
                    None,
                    revision
                )
                .await
                .is_err()
        );
        assert!(server.requests.lock().unwrap().is_empty());
        assert_eq!(
            library
                .create_playlist(
                    &catalog,
                    "Name",
                    "",
                    PlaylistPrivacy::Private,
                    None,
                    revision
                )
                .await
                .unwrap_err()
                .code,
            "playlist_write_unconfirmed"
        );
        assert!(library.snapshot().unwrap().playlists.is_empty());
        assert_eq!(server.requests.lock().unwrap().len(), 1);
    }
    let account = Account::new(Arc::new(MemoryVault::default()));
    let server = Server::new(|_| panic!("guest must not send a playlist write"));
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account, Arc::new(|_| {}));
    assert_eq!(
        library
            .create_playlist(&catalog, "Name", "", PlaylistPrivacy::Private, None, 0)
            .await
            .unwrap_err()
            .code,
        "account_required"
    );
}

#[tokio::test]
async fn a_late_created_playlist_cannot_repopulate_a_signed_out_account() {
    let account = account().await;
    let entered = Arc::new(tokio::sync::Notify::new());
    let signal = entered.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let server = Server::new(move |_| {
        signal.notify_one();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        (200, json!({"responseContext":{},"playlistId":"PLlate"}))
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
    let creation = library.create_playlist(
        &catalog,
        "Late",
        "",
        PlaylistPrivacy::Private,
        None,
        account.snapshot().revision,
    );
    let switch = async {
        entered.notified().await;
        account.sign_out().await.unwrap();
        library.synchronize_account().unwrap();
        release.send(()).unwrap();
    };
    let (result, _) = tokio::join!(creation, switch);
    assert!(result.is_err());
    let snapshot = library.snapshot().unwrap();
    assert!(
        !snapshot.connected
            && snapshot.playlists.is_empty()
            && snapshot.editable_playlist_ids.is_empty()
    );
}

#[tokio::test]
async fn unconfirmed_song_additions_do_not_publish_success_or_retry_the_write() {
    for response in [
        json!({"responseContext":{}}),
        json!({"responseContext":{},"status":"STATUS_FAILED"}),
        json!({"responseContext":{},"status":"STATUS_SUCCEEDED","actions":[{"confirmDialogRenderer":{}}]}),
    ] {
        let account = account().await;
        let server = Server::new(move |request| {
            if request.url.path().ends_with("/create") {
                (200, json!({"responseContext":{},"playlistId":"PLcreated"}))
            } else {
                (200, response.clone())
            }
        });
        let catalog =
            MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
        let library = AccountLibrary::new(account.clone(), Arc::new(|_| {}));
        let revision = account.snapshot().revision;
        let created = library
            .create_playlist(
                &catalog,
                "Name",
                "",
                PlaylistPrivacy::Private,
                None,
                revision,
            )
            .await
            .unwrap();
        let track = parse_liked_page(&songs(&[A])).unwrap().tracks.remove(0);
        assert_eq!(
            library
                .add_to_playlist(&catalog, &created.playlist.id, track, revision)
                .await
                .unwrap_err()
                .code,
            "playlist_write_unconfirmed"
        );
        assert_eq!(
            library.snapshot().unwrap().revision,
            created.library.revision
        );
        assert_eq!(server.requests.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn lyrics_use_the_requested_song_without_auth_and_cache_only_public_text() {
    let account = Account::new(Arc::new(MemoryVault::default()));
    let server = Server::new(|request| {
        assert!(!request.authenticated);
        if request.url.path().ends_with("/next") {
            assert_eq!(request.body["videoId"], A);
            (
                200,
                json!({"contents":{"singleColumnMusicWatchNextResultsRenderer":{"tabbedRenderer":{"watchNextTabbedResultsRenderer":{"tabs":[{"tabRenderer":{"endpoint":{"browseEndpoint":{"browseId":"MPLYprivate"}}}}]}}}}}),
            )
        } else {
            assert_eq!(request.body["browseId"], "MPLYprivate");
            (
                200,
                json!({"contents":{"sectionListRenderer":{"contents":[{"musicDescriptionShelfRenderer":{"description":{"runs":[{"text":"Fixture line one\n"},{"text":"Fixture line two"}]},"footer":{"runs":[{"text":"Fixture source"}]}}}]}}}),
            )
        }
    });
    let catalog = MusicCatalog::with_account_test_server(account.clone(), server.origin.clone());
    let lyrics = catalog.lyrics(A).await.unwrap();
    assert_eq!(
        lyrics.text.as_deref(),
        Some("Fixture line one\nFixture line two")
    );
    assert_eq!(lyrics.source.as_deref(), Some("Fixture source"));
    assert_eq!(catalog.lyrics(A).await.unwrap().video_id, A);
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    assert!(
        !serde_json::to_string(&lyrics)
            .unwrap()
            .contains("MPLYprivate")
    );
}
