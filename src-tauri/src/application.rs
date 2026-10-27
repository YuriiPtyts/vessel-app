use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{Mutex as AsyncMutex, Semaphore, watch};

use crate::{
    account::{Account, AccountMode, AccountSnapshot, CookieSession, MemoryVault, SessionVault},
    catalog::{MusicCatalog, SearchKind, SearchResults},
    error::{AppError, Result},
    library::{LibrarySnapshot, LocalLibrary},
    playback::{PlaybackSnapshot, PlaybackStatus, Player},
    provider::{self, ResolvedTrack, Track, TrackInspection},
    queue::{QueueSnapshot, RadioQueue},
    runtime::RuntimePaths,
};

#[derive(Clone)]
pub enum AppEvent {
    Playback(PlaybackSnapshot),
    Queue(QueueSnapshot),
    Selection(Option<TrackInspection>),
    Account(AccountSnapshot),
}

pub struct Application {
    paths: RuntimePaths,
    player: Player,
    resolved: Mutex<Option<ResolvedTrack>>,
    resolver_slot: Semaphore,
    cancellation: watch::Sender<u64>,
    closing: watch::Sender<bool>,
    catalog: MusicCatalog,
    resolver: provider::Resolver,
    library: Arc<LocalLibrary>,
    account: Arc<Account>,
    queue: Mutex<RadioQueue>,
    events: Arc<dyn Fn(AppEvent) + Send + Sync>,
    transition: AsyncMutex<()>,
    radio_work: AsyncMutex<()>,
    radio_scheduled: AtomicBool,
    radio_cancel: watch::Sender<u64>,
    prefetch_cancel: watch::Sender<u64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSelection {
    pub inspection: TrackInspection,
    pub playback: PlaybackSnapshot,
}

impl Application {
    pub fn new(
        paths: RuntimePaths,
        emit: impl Fn(AppEvent) + Send + Sync + 'static,
    ) -> Result<Arc<Self>> {
        Self::with_library(paths, Arc::new(LocalLibrary::memory()), emit)
    }
    pub fn with_library(
        paths: RuntimePaths,
        library: Arc<LocalLibrary>,
        emit: impl Fn(AppEvent) + Send + Sync + 'static,
    ) -> Result<Arc<Self>> {
        Self::with_services(paths, library, None, Arc::new(MemoryVault::default()), emit)
    }
    pub fn desktop(
        paths: RuntimePaths,
        library: Arc<LocalLibrary>,
        queue_path: PathBuf,
        emit: impl Fn(AppEvent) + Send + Sync + 'static,
    ) -> Result<Arc<Self>> {
        #[cfg(target_os = "macos")]
        let vault: Arc<dyn SessionVault> = Arc::new(crate::account::KeychainVault);
        #[cfg(not(target_os = "macos"))]
        let vault: Arc<dyn SessionVault> = Arc::new(MemoryVault::default());
        Self::with_services(paths, library, Some(queue_path), vault, emit)
    }
    fn with_services(
        paths: RuntimePaths,
        library: Arc<LocalLibrary>,
        queue_path: Option<PathBuf>,
        vault: Arc<dyn SessionVault>,
        emit: impl Fn(AppEvent) + Send + Sync + 'static,
    ) -> Result<Arc<Self>> {
        let (cancellation, _) = watch::channel(0);
        let (closing, _) = watch::channel(false);
        let (radio_cancel, _) = watch::channel(0);
        let (prefetch_cancel, _) = watch::channel(0);
        let (updates, receiver) = watch::channel(PlaybackSnapshot::default());
        let events: Arc<dyn Fn(AppEvent) + Send + Sync> = Arc::new(emit);
        let playback_events = events.clone();
        let history = library.clone();
        let last_played = Mutex::new(None::<u64>);
        let account = Account::new(vault);
        let app = Arc::new(Self {
            player: Player::new(
                paths.clone(),
                Arc::new(move |snapshot| {
                    if let Ok(mut previous) = last_played.lock() {
                        if snapshot.status == PlaybackStatus::Playing
                            && previous.as_ref() != Some(&snapshot.playback_id)
                            && let Some(track) = &snapshot.track
                        {
                            let _ = history.record(track.clone());
                            *previous = Some(snapshot.playback_id);
                        } else if snapshot.status == PlaybackStatus::Idle {
                            *previous = None;
                        }
                    }
                    updates.send_replace(snapshot.clone());
                    playback_events(AppEvent::Playback(snapshot));
                }),
            ),
            resolver: provider::Resolver::new(paths.clone(), account.clone())?,
            paths,
            catalog: MusicCatalog::with_account(account.clone())?,
            account,
            library,
            resolved: Mutex::new(None),
            resolver_slot: Semaphore::new(1),
            cancellation,
            closing,
            radio_cancel,
            prefetch_cancel,
            events,
            queue: Mutex::new(RadioQueue::new(queue_path)),
            transition: AsyncMutex::new(()),
            radio_work: AsyncMutex::new(()),
            radio_scheduled: AtomicBool::new(false),
        });
        Self::monitor(
            Arc::downgrade(&app),
            receiver,
            app.account.subscribe(),
            app.closing.subscribe(),
        );
        Ok(app)
    }

    fn monitor(
        weak: Weak<Self>,
        mut playback: watch::Receiver<PlaybackSnapshot>,
        mut account: watch::Receiver<AccountSnapshot>,
        mut closing: watch::Receiver<bool>,
    ) {
        tauri::async_runtime::spawn(async move {
            let mut account_revision = account.borrow().revision;
            loop {
                tokio::select! {
                    change = playback.changed() => {
                        if change.is_err() { break; }
                        let snapshot = playback.borrow_and_update().clone();
                        let Some(app) = weak.upgrade() else { break };
                        app.handle_playback(&snapshot);
                    }
                    change = account.changed() => {
                        if change.is_err() { break; }
                        let status = account.borrow_and_update().clone();
                        let Some(app) = weak.upgrade() else { break };
                        if status.revision != account_revision {
                            account_revision = status.revision;
                            app.catalog.invalidate();
                            app.resolver.invalidate();
                            app.cancel_radio();
                            app.cancel_inspection();
                            if let Ok(mut queue) = app.queue.lock() { queue.account_changed(); }
                            if status.mode == AccountMode::Expired
                                && app.snapshot().is_ok_and(|snapshot| snapshot.account_source)
                            {
                                let _ = app.stop().await;
                            }
                            app.emit_queue();
                            app.schedule_radio();
                            if let Ok(snapshot) = app.snapshot() { app.handle_playback(&snapshot); }
                        }
                        (app.events)(AppEvent::Account(status));
                    }
                    _ = closing.changed() => break,
                }
            }
        });
    }

    fn handle_playback(self: &Arc<Self>, snapshot: &PlaybackSnapshot) {
        let advance = self
            .queue
            .lock()
            .ok()
            .and_then(|mut queue| queue.claim_end(snapshot));
        if let Some(generation) = advance {
            self.emit_queue();
            let request = Arc::downgrade(self);
            tauri::async_runtime::spawn(async move {
                if let Some(app) = request.upgrade() {
                    let _ = app.advance(generation, None).await;
                }
            });
        } else if snapshot.status == PlaybackStatus::Playing {
            self.schedule_radio();
            self.schedule_prefetch(snapshot);
        }
    }

    fn emit_queue(&self) {
        if let Ok(snapshot) = self.queue() {
            (self.events)(AppEvent::Queue(snapshot));
        }
    }
    pub fn queue(&self) -> Result<QueueSnapshot> {
        self.queue
            .lock()
            .map(|mut queue| queue.published_snapshot())
            .map_err(|_| AppError::internal())
    }
    pub fn account(&self) -> AccountSnapshot {
        self.account.snapshot()
    }
    pub fn runtime_available(&self) -> bool {
        self.paths.available()
    }
    pub fn inspection(&self) -> Result<Option<TrackInspection>> {
        let id = self.snapshot()?.track.map(|track| track.id);
        Ok(self
            .resolved
            .lock()
            .map_err(|_| AppError::internal())?
            .as_ref()
            .filter(|track| Some(&track.inspection.track.id) == id.as_ref())
            .map(|track| track.inspection.clone()))
    }
    pub async fn inspect(&self, url: &str) -> Result<TrackInspection> {
        self.prepare(url).await.map(|(inspection, _)| inspection)
    }
    async fn prepare(&self, url: &str) -> Result<(TrackInspection, u64)> {
        let mut cancel = self.cancellation.subscribe();
        self.cancel_inspection();
        let generation = *cancel.borrow_and_update();
        let _slot = tokio::select! {
            slot = self.resolver_slot.acquire() => slot.map_err(|_| AppError::internal())?,
            _ = cancel.changed() => return Err(AppError::cancelled()),
        };
        let resolved = tokio::select! {
            result = self.resolver.resolve(url) => result?,
            _ = cancel.changed() => return Err(AppError::cancelled()),
        };
        let mut current = self.resolved.lock().map_err(|_| AppError::internal())?;
        if *cancel.borrow() != generation {
            return Err(AppError::cancelled());
        }
        let inspection = resolved.inspection.clone();
        *current = Some(resolved);
        Ok((inspection, generation))
    }
    pub async fn search(&self, query: &str, kind: SearchKind, more: bool) -> Result<SearchResults> {
        self.catalog.search(query, kind, more).await
    }
    pub async fn home(&self, refresh: bool) -> Result<crate::home::HomeFeed> {
        self.catalog.home(refresh).await
    }
    pub async fn collection(&self, id: &str, more: bool) -> Result<crate::home::CollectionPage> {
        self.catalog.collection(id, more).await
    }

    pub async fn play_song(self: &Arc<Self>, id: &str) -> Result<PlaybackSelection> {
        self.play_song_with_context(id, Vec::new()).await
    }
    pub async fn play_song_with_context(
        self: &Arc<Self>,
        id: &str,
        context: Vec<String>,
    ) -> Result<PlaybackSelection> {
        if !crate::catalog::valid_video_id(id) {
            return Err(AppError::new(
                "invalid_track",
                "Choose a valid song from the music catalog.",
                false,
            ));
        }
        self.cancel_inspection();
        self.cancel_radio();
        let metadata = self
            .catalog
            .find_track(id)
            .or_else(|| self.library.find(id));
        let context: Vec<_> = context
            .iter()
            .take(100)
            .filter_map(|id| {
                self.catalog
                    .find_track(id)
                    .or_else(|| self.library.find(id))
            })
            .collect();
        let generation = {
            self.queue.lock().map_err(|_| AppError::internal())?.begin(
                id,
                metadata.clone(),
                context,
            )
        };
        self.emit_queue();
        self.schedule_radio();
        let _transition = self.transition.lock().await;
        let result = self.load_track(id, metadata, generation).await;
        if let Err(error) = &result {
            if let Ok(mut queue) = self.queue.lock() {
                queue.fail_transition(generation, error.clone());
            }
            self.emit_queue();
        }
        result
    }
    async fn load_track(
        &self,
        id: &str,
        metadata: Option<Track>,
        generation: u64,
    ) -> Result<PlaybackSelection> {
        self.queue
            .lock()
            .map_err(|_| AppError::internal())?
            .check(generation)?;
        let (mut inspection, selection_generation) = self
            .prepare(&format!("https://www.youtube.com/watch?v={id}"))
            .await?;
        {
            self.queue
                .lock()
                .map_err(|_| AppError::internal())?
                .check(generation)?;
            let mut current = self.resolved.lock().map_err(|_| AppError::internal())?;
            if *self.cancellation.borrow() != selection_generation {
                return Err(AppError::cancelled());
            }
            let resolved = current
                .as_mut()
                .filter(|resolved| resolved.inspection.track.id == id)
                .ok_or_else(AppError::cancelled)?;
            if let Some(mut track) = metadata {
                track.duration_seconds =
                    inspection.track.duration_seconds.or(track.duration_seconds);
                inspection.track = track;
                resolved.inspection.track = inspection.track.clone();
            }
        }
        self.queue
            .lock()
            .map_err(|_| AppError::internal())?
            .ensure_seed(inspection.track.clone(), generation)?;
        let playback = self.play(id, &inspection.recommended_format_id).await?;
        self.queue
            .lock()
            .map_err(|_| AppError::internal())?
            .bind(generation, &playback)?;
        (self.events)(AppEvent::Selection(Some(inspection.clone())));
        self.emit_queue();
        Ok(PlaybackSelection {
            inspection,
            playback,
        })
    }

    fn schedule_radio(self: &Arc<Self>) {
        let needed = self.queue.lock().is_ok_and(|queue| {
            queue.snapshot.running
                && queue.snapshot.autoplay
                && queue.future_count() < 8
                && !queue.snapshot.generating
        });
        if !needed || self.radio_scheduled.swap(true, Ordering::SeqCst) {
            return;
        }
        let request = Arc::downgrade(self);
        tauri::async_runtime::spawn(async move {
            if let Some(app) = request.upgrade() {
                let generation = app.queue.lock().map(|queue| queue.generation).unwrap_or(0);
                let _ = app.refill(generation, false).await;
                app.radio_scheduled.store(false, Ordering::SeqCst);
            }
        });
    }
    async fn refill(&self, generation: u64, force: bool) -> Result<()> {
        let _work = self.radio_work.lock().await;
        let request = {
            let mut queue = self.queue.lock().map_err(|_| AppError::internal())?;
            queue.check(generation)?;
            queue.reserve_radio(force)
        };
        let Some((generation, seed, cursor)) = request else {
            return Ok(());
        };
        self.emit_queue();
        let mut cancel = self.radio_cancel.subscribe();
        cancel.borrow_and_update();
        let result = tokio::select! {
            result = self.catalog.radio(&seed, cursor.as_ref()) => result,
            _ = cancel.changed() => Err(AppError::cancelled()),
        };
        let mut queue = self.queue.lock().map_err(|_| AppError::internal())?;
        let result = match result {
            Ok(batch) => queue.append(generation, batch),
            Err(error) => {
                queue.fail_radio(generation, error.clone());
                Err(error)
            }
        };
        drop(queue);
        self.emit_queue();
        result
    }
    fn schedule_prefetch(self: &Arc<Self>, snapshot: &PlaybackSnapshot) {
        let candidate = self
            .queue
            .lock()
            .ok()
            .and_then(|mut queue| queue.prefetch_candidate(snapshot));
        let Some((generation, track)) = candidate else {
            return;
        };
        self.emit_queue();
        let request = Arc::downgrade(self);
        tauri::async_runtime::spawn(async move {
            if let Some(app) = request.upgrade() {
                let mut cancel = app.prefetch_cancel.subscribe();
                cancel.borrow_and_update();
                let work = async {
                    let _slot = app
                        .resolver_slot
                        .acquire()
                        .await
                        .map_err(|_| AppError::internal())?;
                    app.queue
                        .lock()
                        .map_err(|_| AppError::internal())?
                        .check(generation)?;
                    app.resolver
                        .resolve(&format!("https://www.youtube.com/watch?v={}", track.id))
                        .await
                        .map(|_| ())
                };
                let _ = tokio::select! {
                    result = work => result,
                    _ = cancel.changed() => Err(AppError::cancelled()),
                };
                if let Ok(mut queue) = app.queue.lock() {
                    queue.finish_prefetch(generation, &track.id);
                }
                app.emit_queue();
            }
        });
    }
    fn cancel_radio(&self) {
        self.radio_cancel
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
    fn cancel_prefetch(&self) {
        self.prefetch_cancel
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    async fn advance(
        self: &Arc<Self>,
        generation: u64,
        target: Option<(usize, String)>,
    ) -> Result<Option<PlaybackSelection>> {
        let _transition = self.transition.lock().await;
        self.queue
            .lock()
            .map_err(|_| AppError::internal())?
            .check(generation)?;
        for _ in 0..3 {
            let mut index = target.as_ref().map(|(index, _)| *index).or_else(|| {
                self.queue
                    .lock()
                    .ok()?
                    .snapshot
                    .current_index
                    .map(|index| index + 1)
            });
            let needs_more = {
                let queue = self.queue.lock().map_err(|_| AppError::internal())?;
                index.is_none_or(|index| index >= queue.snapshot.tracks.len())
            };
            if needs_more {
                let _ = self.refill(generation, true).await;
                index = target.as_ref().map(|(index, _)| *index).or_else(|| {
                    self.queue
                        .lock()
                        .ok()?
                        .snapshot
                        .current_index
                        .map(|index| index + 1)
                });
            }
            let next = {
                let mut queue = self.queue.lock().map_err(|_| AppError::internal())?;
                queue.check(generation)?;
                index
                    .filter(|index| *index < queue.snapshot.tracks.len())
                    .map(|index| queue.jump(index, target.as_ref().map(|(_, id)| id.as_str())))
                    .transpose()?
            };
            let Some(track) = next else {
                let error = crate::radio::radio_error();
                if let Ok(mut queue) = self.queue.lock() {
                    queue.fail_transition(generation, error.clone());
                }
                self.emit_queue();
                return Err(error);
            };
            self.emit_queue();
            match self
                .load_track(&track.id, Some(track.clone()), generation)
                .await
            {
                Ok(selection) => {
                    self.schedule_radio();
                    return Ok(Some(selection));
                }
                Err(error) if error.code == "cancelled" => return Err(error),
                Err(error) => {
                    if let Ok(mut queue) = self.queue.lock() {
                        queue.fail_transition(generation, error);
                    }
                    self.emit_queue();
                    if target.is_some() {
                        break;
                    }
                }
            }
        }
        Err(AppError::new(
            "queue_unavailable",
            "Several queued songs could not play. Choose another song or retry radio.",
            true,
        ))
    }
    pub async fn next(self: &Arc<Self>) -> Result<Option<PlaybackSelection>> {
        self.cancel_inspection();
        self.cancel_radio();
        let generation = {
            self.queue
                .lock()
                .map_err(|_| AppError::internal())?
                .cancel_work()
        };
        self.advance(generation, None).await
    }
    pub async fn previous(self: &Arc<Self>) -> Result<Option<PlaybackSelection>> {
        let snapshot = self.snapshot()?;
        let index = self.queue()?.current_index.unwrap_or(0);
        if (snapshot.position_seconds > 3.0 || index == 0)
            && matches!(
                snapshot.status,
                PlaybackStatus::Playing | PlaybackStatus::Paused | PlaybackStatus::Buffering
            )
        {
            self.seek(0.0).await?;
            return Ok(None);
        }
        let index = self
            .queue
            .lock()
            .map_err(|_| AppError::internal())?
            .snapshot
            .current_index
            .unwrap_or(0)
            .saturating_sub(1);
        self.jump_queue(index, None).await.map(Some)
    }
    pub async fn jump_queue(
        self: &Arc<Self>,
        index: usize,
        expected_id: Option<&str>,
    ) -> Result<PlaybackSelection> {
        let (generation, id) = {
            let mut queue = self.queue.lock().map_err(|_| AppError::internal())?;
            let id = queue
                .snapshot
                .tracks
                .get(index)
                .filter(|track| expected_id.is_none_or(|id| track.id == id))
                .map(|track| track.id.clone())
                .ok_or_else(|| {
                    AppError::new(
                        "queue_item_missing",
                        "The queue changed. Choose this song again.",
                        true,
                    )
                })?;
            (queue.cancel_work(), id)
        };
        self.cancel_inspection();
        self.cancel_radio();
        self.advance(generation, Some((index, id)))
            .await?
            .ok_or_else(AppError::internal)
    }
    pub fn remove_queue(&self, index: usize, expected_id: &str) -> Result<QueueSnapshot> {
        self.cancel_prefetch();
        self.queue
            .lock()
            .map_err(|_| AppError::internal())?
            .remove(index, expected_id)?;
        self.emit_queue();
        self.queue()
    }
    pub async fn clear_queue(&self) -> Result<QueueSnapshot> {
        self.stop().await?;
        self.queue.lock().map_err(|_| AppError::internal())?.clear();
        *self.resolved.lock().map_err(|_| AppError::internal())? = None;
        self.resolver.invalidate();
        self.emit_queue();
        self.queue()
    }
    pub async fn autoplay(self: &Arc<Self>, enabled: bool) -> Result<QueueSnapshot> {
        if !enabled {
            self.cancel_radio();
        }
        self.queue
            .lock()
            .map_err(|_| AppError::internal())?
            .set_autoplay(enabled);
        self.emit_queue();
        if enabled {
            let snapshot = self.snapshot()?;
            let advance = self
                .queue
                .lock()
                .ok()
                .and_then(|mut queue| queue.claim_end(&snapshot));
            if let Some(generation) = advance {
                let _ = self.advance(generation, None).await;
            } else {
                self.schedule_radio();
            }
        }
        self.queue()
    }
    pub async fn retry_radio(self: &Arc<Self>) -> Result<QueueSnapshot> {
        let generation = {
            let mut queue = self.queue.lock().map_err(|_| AppError::internal())?;
            queue.retry_radio();
            queue.generation
        };
        self.refill(generation, true).await?;
        self.handle_playback(&self.snapshot()?);
        self.queue()
    }

    pub fn library(&self) -> Result<LibrarySnapshot> {
        self.library.snapshot()
    }
    pub fn toggle_like(&self, id: &str) -> Result<LibrarySnapshot> {
        let track = self
            .catalog
            .find_track(id)
            .or_else(|| self.library.find(id))
            .or_else(|| self.snapshot().ok()?.track.filter(|track| track.id == id))
            .ok_or_else(|| {
                AppError::new(
                    "unknown_track",
                    "Search for this track again before saving it.",
                    true,
                )
            })?;
        self.library.toggle_like(track)
    }
    pub fn cancel_inspection(&self) {
        self.cancel_prefetch();
        self.cancellation
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    pub fn begin_sign_in(&self) -> u64 {
        self.account.begin(false)
    }
    pub fn cancel_sign_in(&self, id: u64) {
        self.account.cancel(id);
    }
    pub fn cancel_account_connection(&self) {
        self.account.cancel_current();
    }
    pub(crate) fn fail_sign_in(&self, id: u64, error: AppError) {
        self.account.fail(id, error);
    }
    pub(crate) async fn complete_sign_in(
        &self,
        id: u64,
        session: CookieSession,
    ) -> Result<AccountSnapshot> {
        let result = async {
            let profile = self.catalog.verify_account(&session).await?;
            self.account.activate(id, session, profile, true).await
        }
        .await;
        if let Err(error) = &result {
            self.account.fail(id, error.clone());
        }
        result
    }
    pub async fn sign_out(&self) -> Result<AccountSnapshot> {
        let snapshot = self.account.sign_out().await?;
        self.stop().await?;
        *self.resolved.lock().map_err(|_| AppError::internal())? = None;
        (self.events)(AppEvent::Selection(None));
        self.resolver.invalidate();
        self.catalog.invalidate();
        Ok(snapshot)
    }
    async fn restore_account(&self) {
        let Some(id) = self.account.begin_restore() else {
            return;
        };
        match self.account.saved().await {
            Ok(Some((session, _))) => {
                let result = async {
                    let profile = self.catalog.verify_account(&session).await?;
                    self.account.activate(id, session, profile, false).await
                }
                .await;
                if let Err(error) = result {
                    self.account.fail(id, error);
                }
            }
            Ok(None) => self.account.cancel(id),
            Err(error) => self.account.fail(id, error),
        }
    }
    pub async fn warm_up(&self) {
        let mut cancel = self.closing.subscribe();
        if *cancel.borrow_and_update() {
            return;
        }
        tokio::select! {_=async{let _=tokio::join!(self.resolver.warm_up(),self.player.warm_up(),self.restore_account());}=>{},_=cancel.changed()=>{}}
    }
    pub async fn play(&self, id: &str, format_id: &str) -> Result<PlaybackSnapshot> {
        let (track, format, source) = self
            .resolved
            .lock()
            .map_err(|_| AppError::internal())?
            .as_ref()
            .ok_or_else(|| {
                AppError::new(
                    "track_not_inspected",
                    "Choose a song before changing its audio quality.",
                    false,
                )
            })?
            .source(id, format_id, self.account().revision)?;
        let mut cancel = self.cancellation.subscribe();
        let generation = *cancel.borrow_and_update();
        tokio::select! {result=source.wait_ready()=>result?,_=cancel.changed()=>return Err(AppError::cancelled())}
        self.player
            .load(track, format, source, cancel, generation)
            .await
    }
    pub fn snapshot(&self) -> Result<PlaybackSnapshot> {
        self.player.snapshot()
    }
    pub async fn pause(&self, paused: bool) -> Result<PlaybackSnapshot> {
        self.player.pause(paused).await
    }
    pub async fn seek(&self, position: f64) -> Result<PlaybackSnapshot> {
        self.player.seek(position).await
    }
    pub async fn volume(&self, volume: f64) -> Result<PlaybackSnapshot> {
        self.player.volume(volume).await
    }
    pub async fn toggle_mute(&self) -> Result<PlaybackSnapshot> {
        self.player.toggle_mute().await
    }
    pub async fn stop(&self) -> Result<PlaybackSnapshot> {
        self.cancel_inspection();
        self.cancel_radio();
        if let Ok(mut queue) = self.queue.lock() {
            queue.stop();
        }
        self.emit_queue();
        self.player.stop().await
    }
    pub fn shutdown(&self) {
        self.closing.send_replace(true);
        self.cancel_inspection();
        self.cancel_radio();
        self.catalog.cancel();
        self.player.shutdown();
    }
}
