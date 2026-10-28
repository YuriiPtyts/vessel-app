<script lang="ts">
  import { onMount } from 'svelte';
  import {
    ArrowRight,
    AudioLines,
    ChevronRight,
    Clock3,
    Heart,
    House,
    Library,
    LoaderCircle,
    Music2,
    Moon,
    Sun,
    ListMusic,
    RefreshCw,
    Search,
    Settings2,
    X,
  } from '@lucide/svelte';
  import Artwork from './lib/components/Artwork.svelte';
  import TrackList from './lib/components/TrackList.svelte';
  import PlayerBar from './lib/components/PlayerBar.svelte';
  import AppearancePanel from './lib/components/AppearancePanel.svelte';
  import NowPlayingPanel from './lib/components/NowPlayingPanel.svelte';
  import HomeView from './lib/components/HomeView.svelte';
  import QueuePanel from './lib/components/QueuePanel.svelte';
  import AccountPanel from './lib/components/AccountPanel.svelte';
  import {
    defaultAppearance,
    loadAppearance,
    normalizeAppearance,
    usesLightGlass,
    saveAppearance,
    usesOpaqueMaterial,
    type AppearancePreferences,
  } from './lib/appearance';
  import { createVolumeControl } from './lib/volume';
  import { unifiedMaterial } from './lib/material';
  import {
    cancelInspection,
    emptyPlayback,
    emptyQueue,
    guestAccount,
    errorMessage,
    getLibrary,
    getAccountLibrary,
    refreshAccountLibrary,
    emptyAccountLibrary,
    setAccountLike,
    browseAccountPlaylist,
    onAccountLibrary,
    getPlayback,
    getRuntimeReport,
    getHomeFeed,
    browseCollection,
    isCancelled,
    onPlayback,
    onQueue,
    onAccount,
    onSelection,
    getQueue,
    getAccount,
    getSelection,
    nextSong,
    previousSong,
    jumpQueue,
    removeQueue,
    clearQueue,
    setAutoplay,
    retryRadio,
    connectGoogle,
    cancelGoogle,
    signOut,
    playSong,
    playTrack,
    searchMusic,
    seekTrack,
    setMaterialMode,
    setPaused,
    setVolume,
    toggleMute,
    stopPlayback,
    type LocalLibrary,
    type AccountLibrarySnapshot,
    type PlaybackSnapshot,
    type RuntimeReport,
    type SearchKind,
    type SearchResults,
    type Track,
    type TrackInspection,
    type HomeFeed,
    type HomeItem,
    type CollectionPage,
    type QueueSnapshot,
    type AccountSnapshot,
    type PlaybackSelection,
  } from './lib/native';

  type View = 'home' | 'search' | 'liked' | 'recent' | 'collection';
  let view = $state<View>('home');
  let report = $state<RuntimeReport | null>(null);
  let checking = $state(true);
  let error = $state<string | null>(null);
  let library = $state<LocalLibrary>({ liked: [], recent: [] });
  let accountLibrary = $state(emptyAccountLibrary());
  let libraryLoading = $state(false);
  let libraryError = $state<string | null>(null);
  let libraryRequest = 0;
  let likeBusy = $state(false);
  let likeRevision = 0;
  let home = $state<HomeFeed | null>(null);
  let homeLoading = $state(false);
  let homeError = $state(false);
  let collection = $state<CollectionPage | null>(null);
  let collectionLoading = $state(false);
  let collectionMore = $state(false);
  let collectionFromAccount = $state(false);
  let collectionRevision = 0;
  let homeRevision = 0;
  let query = $state('');
  let kind = $state<SearchKind>('songs');
  let results = $state<SearchResults | null>(null);
  let searching = $state(false);
  let loadingMore = $state(false);
  let playback = $state<PlaybackSnapshot>(emptyPlayback);
  let inspection = $state<TrackInspection | null>(null);
  let pending = $state<Track | null>(null);
  let queue = $state<QueueSnapshot>(emptyQueue);
  let account = $state<AccountSnapshot>(guestAccount);
  let queueOpen = $state(false);
  let accountOpen = $state(false);
  let accountBusy = $state(false);
  let busy = $state(false);
  let settingsOpen = $state(false);
  let appearance = $state(defaultAppearance());
  let viewportWidth = $state(0);
  let artworkOverlayOpen = $state(false);
  let searchInput: HTMLInputElement;
  let searchRevision = 0;
  let playbackRevision = 0;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let lastHistoryId: string | undefined;
  const stateRevision = { playback: 0, queue: 0, selection: 0, account: 0 };
  let volumeInteraction = 0;
  const volumeWriter = createVolumeControl(
    async (value) => {
      if (!report) return;
      const snapshot = await setVolume(value);
      // A volume reply must not roll track/transport state back to an older snapshot.
      acceptVolume(snapshot);
    },
    (failure) => {
      error = errorMessage(failure);
    },
  );
  function volume(value: number) {
    volumeInteraction++;
    return volumeWriter(value);
  }

  function acceptVolume(snapshot: PlaybackSnapshot) {
    if (snapshot.volumeRevision >= playback.volumeRevision) {
      playback = { ...playback, volume: snapshot.volume, volumeRevision: snapshot.volumeRevision };
    }
  }

  function acceptPlayback(snapshot: PlaybackSnapshot) {
    playback =
      snapshot.volumeRevision < playback.volumeRevision
        ? { ...snapshot, volume: playback.volume, volumeRevision: playback.volumeRevision }
        : snapshot;
  }

  async function mute() {
    if (!report) return;
    const interaction = ++volumeInteraction;
    await volumeWriter.flush();
    if (interaction !== volumeInteraction) return;
    try {
      acceptVolume(await toggleMute());
    } catch (failure) {
      error = errorMessage(failure);
    }
  }

  const hasAccountCollection = $derived(account.mode === 'connected' || account.profile !== null);
  const likedIds = $derived(new Set(accountLibrary.likedIds));
  const likesDisabled = $derived(
    likeBusy || account.mode !== 'connected' || !accountLibrary.likesReady,
  );
  const likeDestination = $derived(
    account.mode === 'connected'
      ? 'YouTube Music · synced with your account'
      : 'Connect Google to like songs on YouTube Music',
  );
  const playing = $derived(playback.status === 'playing' || playback.status === 'buffering');
  const loadingTrack = $derived(
    pending ??
      (queue.transitioning && queue.currentIndex !== null
        ? queue.tracks[queue.currentIndex]
        : null),
  );
  const shownTrack = $derived(loadingTrack ?? playback.track);
  const artworkVisible = $derived(
    Boolean(
      shownTrack && ((appearance.showNowPlaying && viewportWidth >= 1180) || artworkOverlayOpen),
    ),
  );
  const currentIndex = $derived(queue.currentIndex ?? -1);
  const listTracks = $derived(
    view === 'liked'
      ? accountLibrary.liked
      : view === 'recent'
        ? library.recent
        : view === 'collection'
          ? (collection?.tracks ?? [])
          : (results?.tracks ?? []),
  );

  onMount(() => {
    let disposed = false;
    const subscriptions: (() => void)[] = [];
    async function register(subscription: Promise<() => void>) {
      const unlisten = await subscription;
      if (disposed) unlisten();
      else subscriptions.push(unlisten);
    }
    try {
      appearance = loadAppearance(
        localStorage,
        matchMedia('(prefers-reduced-transparency: reduce)').matches,
      );
    } catch {
      /* Use the default material if storage is unavailable. */
    }
    async function initialize() {
      try {
        const runtime = await getRuntimeReport();
        if (disposed) return;
        report = runtime;
        if (runtime) {
          await Promise.all([
            register(
              onPlayback((snapshot) => {
                if (disposed) return;
                stateRevision.playback++;
                acceptPlayback(snapshot);
                if (snapshot.status === 'playing' && snapshot.track?.id !== lastHistoryId) {
                  lastHistoryId = snapshot.track?.id;
                  void refreshLibrary();
                }
              }),
            ),
            register(
              onQueue((snapshot) => {
                if (!disposed) {
                  stateRevision.queue++;
                  updateQueue(snapshot);
                }
              }),
            ),
            register(
              onSelection((selection) => {
                if (!disposed) {
                  stateRevision.selection++;
                  inspection = selection;
                }
              }),
            ),
            register(
              onAccount((snapshot) => {
                if (!disposed) {
                  stateRevision.account++;
                  updateAccount(snapshot);
                }
              }),
            ),
            register(
              onAccountLibrary((snapshot) => {
                if (!disposed) updateAccountLibrary(snapshot);
              }),
            ),
          ]);
          if (disposed) return;
          await Promise.all([
            refreshSession(),
            refreshLibrary(),
            setMaterialMode(usesOpaqueMaterial(appearance)),
          ]);
          if (!disposed && view === 'home') void loadHome();
        }
      } catch (failure) {
        if (!disposed) error = errorMessage(failure);
      } finally {
        if (!disposed) checking = false;
      }
    }
    void initialize();
    return () => {
      disposed = true;
      subscriptions.forEach((unsubscribe) => unsubscribe());
      clearTimeout(searchTimer);
    };
  });

  async function refreshLibrary() {
    try {
      library = await getLibrary();
    } catch (failure) {
      error = errorMessage(failure);
    }
  }

  async function refreshSession() {
    const revision = { ...stateRevision };
    const [transport, upcoming, selection, profile] = await Promise.all([
      getPlayback(),
      getQueue(),
      getSelection(),
      getAccount(),
    ]);
    if (revision.playback === stateRevision.playback) acceptPlayback(transport);
    updateQueue(upcoming);
    if (revision.selection === stateRevision.selection) inspection = selection;
    if (revision.account === stateRevision.account) updateAccount(profile);
  }

  function updateAccountLibrary(snapshot: AccountLibrarySnapshot) {
    if (
      snapshot.accountRevision !== account.revision ||
      snapshot.revision < accountLibrary.revision
    )
      return;
    if (snapshot.connected && account.mode !== 'connected') return;
    accountLibrary = snapshot;
  }

  async function syncAccountLibrary(force = false) {
    if (!report || account.mode !== 'connected' || libraryLoading) return;
    const revision = account.revision;
    const request = ++libraryRequest;
    libraryLoading = true;
    libraryError = null;
    try {
      updateAccountLibrary(await getAccountLibrary());
      if (revision !== account.revision || request !== libraryRequest) return;
      updateAccountLibrary(await refreshAccountLibrary(revision, force));
    } catch (failure) {
      if (request === libraryRequest && revision === account.revision && !isCancelled(failure))
        libraryError = errorMessage(failure);
    } finally {
      if (request === libraryRequest) libraryLoading = false;
    }
  }

  function updateAccount(snapshot: AccountSnapshot) {
    if (snapshot.revision < account.revision) return;
    const changed = snapshot.revision !== account.revision;
    const connectionChanged = (snapshot.mode === 'connected') !== (account.mode === 'connected');
    account = snapshot;
    if (changed || connectionChanged) {
      libraryRequest++;
      libraryLoading = false;
      libraryError = null;
      accountLibrary = emptyAccountLibrary(snapshot.revision);
      likeRevision++;
      likeBusy = false;
      if (snapshot.mode === 'connected') void syncAccountLibrary();
    }
    if (changed) {
      homeRevision++;
      homeLoading = false;
      home = null;
      searchRevision++;
      results = null;
      searching = false;
      loadingMore = false;
      collectionRevision++;
      collection = null;
      collectionLoading = false;
      collectionMore = false;
      if (view === 'collection') view = 'home';
      if (view === 'home') void loadHome(true);
      else if (view === 'search' && query.trim().length >= 2) void runSearch();
    }
  }

  function updateQueue(snapshot: QueueSnapshot) {
    if (snapshot.revision >= queue.revision) queue = snapshot;
  }

  async function accountAction(action: () => Promise<AccountSnapshot>) {
    if (!report || accountBusy) return;
    accountBusy = true;
    const revision = stateRevision.account;
    try {
      const snapshot = await action();
      if (revision === stateRevision.account) updateAccount(snapshot);
    } catch (failure) {
      error = errorMessage(failure);
    } finally {
      accountBusy = false;
    }
  }

  async function queueAction(action: () => Promise<QueueSnapshot>) {
    if (!report) return;
    try {
      const snapshot = await action();
      updateQueue(snapshot);
    } catch (failure) {
      if (!isCancelled(failure)) error = errorMessage(failure);
    }
  }

  async function transport(action: () => Promise<PlaybackSelection | null>) {
    if (!report) return;
    const revision = ++playbackRevision;
    pending = null;
    const selectionRevision = stateRevision.selection;
    error = null;
    try {
      const selection = await action();
      if (
        selection &&
        revision === playbackRevision &&
        selectionRevision === stateRevision.selection
      ) {
        inspection = selection.inspection;
        acceptPlayback(selection.playback);
      }
    } catch (failure) {
      if (revision === playbackRevision && !isCancelled(failure)) error = errorMessage(failure);
    }
  }

  async function loadHome(refresh = false) {
    if (!report || homeLoading) return;
    homeLoading = true;
    const revision = ++homeRevision;
    homeError = false;
    try {
      const feed = await getHomeFeed(refresh);
      if (revision === homeRevision) home = feed;
    } catch (failure) {
      if (revision === homeRevision && !isCancelled(failure)) homeError = true;
    } finally {
      if (revision === homeRevision) homeLoading = false;
    }
  }

  async function openCollection(item: HomeItem, more = false, fromAccount = false) {
    if (!report) return;
    const revision = ++collectionRevision;
    view = 'collection';
    collectionFromAccount = fromAccount;
    error = null;
    if (more) collectionMore = true;
    else {
      collection = { item, tracks: [], hasMore: false };
      collectionLoading = true;
      collectionMore = false;
    }
    try {
      const page = fromAccount
        ? await browseAccountPlaylist(item.id, account.revision, more)
        : await browseCollection(item.id, more);
      if (revision === collectionRevision) collection = page;
    } catch (failure) {
      if (revision === collectionRevision && !isCancelled(failure)) error = errorMessage(failure);
    } finally {
      if (revision === collectionRevision) {
        collectionLoading = false;
        collectionMore = false;
      }
    }
  }

  function navigate(next: View) {
    view = next;
    settingsOpen = false;
    accountOpen = false;
    if (next === 'search') searchInput?.focus();
    if (next === 'home' && !home && !homeLoading && report) void loadHome();
    if (next === 'recent') void refreshLibrary();
    if (next === 'liked') void syncAccountLibrary();
  }

  function typing() {
    view = 'search';
    clearTimeout(searchTimer);
    if (query.trim().length >= 2) searchTimer = setTimeout(() => void runSearch(), 450);
    else {
      searchRevision++;
      results = null;
      searching = false;
      error = null;
    }
  }

  async function runSearch(more = false) {
    clearTimeout(searchTimer);
    if (!report || query.trim().length < 2) return;
    const revision = ++searchRevision;
    const text = query.trim();
    const filter = kind;
    view = 'search';
    error = null;
    if (more) loadingMore = true;
    else {
      searching = true;
      results = null;
    }
    try {
      const response = await searchMusic(text, filter, more);
      if (revision === searchRevision) results = response;
    } catch (failure) {
      if (revision === searchRevision && !isCancelled(failure)) error = errorMessage(failure);
    } finally {
      if (revision === searchRevision) {
        searching = false;
        loadingMore = false;
      }
    }
  }

  function find(text: string) {
    query = text;
    kind = 'songs';
    void runSearch();
  }
  function filter(next: SearchKind) {
    kind = next;
    if (query.trim().length >= 2) void runSearch();
  }

  async function control(action: () => Promise<PlaybackSnapshot>) {
    if (!report || busy) return;
    busy = true;
    error = null;
    try {
      acceptPlayback(await action());
    } catch (failure) {
      if (!isCancelled(failure)) error = errorMessage(failure);
    } finally {
      busy = false;
    }
  }

  async function select(track: Track, tracks = listTracks) {
    if (!report?.playbackDependenciesAvailable) return;
    if (
      playback.track?.id === track.id &&
      ['playing', 'paused', 'buffering'].includes(playback.status) &&
      !pending
    ) {
      await control(() => setPaused(playback.status !== 'paused'));
      return;
    }
    const revision = ++playbackRevision;
    pending = track;
    error = null;
    try {
      const context = ['collection', 'liked', 'recent'].includes(view)
        ? tracks.map((track) => track.id)
        : [];
      const selection = await playSong(track.id, context);
      if (revision === playbackRevision) {
        inspection = selection.inspection;
        acceptPlayback(selection.playback);
      }
    } catch (failure) {
      if (revision === playbackRevision && !isCancelled(failure)) error = errorMessage(failure);
    } finally {
      if (revision === playbackRevision) pending = null;
    }
  }

  async function cancel() {
    playbackRevision++;
    pending = null;
    try {
      await cancelInspection();
    } catch (failure) {
      error = errorMessage(failure);
    }
  }

  async function like(track: Track) {
    if (!report || likesDisabled) return;
    const revision = account.revision;
    const interaction = ++likeRevision;
    const liked = !likedIds.has(track.id);
    likeBusy = true;
    try {
      updateAccountLibrary(await setAccountLike(track.id, liked, revision));
    } catch (failure) {
      if (interaction === likeRevision && !isCancelled(failure)) error = errorMessage(failure);
    } finally {
      if (interaction === likeRevision) likeBusy = false;
    }
  }

  async function quality(id: string) {
    if (!inspection || pending || inspection.track.id !== playback.track?.id) return;
    const trackId = inspection.track.id;
    await control(() => playTrack(trackId, id));
  }

  async function updateAppearance(change: Partial<AppearancePreferences>, persist = true) {
    const previous = appearance;
    const next = normalizeAppearance({ ...appearance, ...change });
    appearance = next;
    if (change.showNowPlaying === false) artworkOverlayOpen = false;
    if (persist) {
      try {
        saveAppearance(localStorage, next);
      } catch {
        /* Session-only appearance is still available. */
      }
    }
    try {
      if (report && usesOpaqueMaterial(next) !== usesOpaqueMaterial(previous)) {
        await setMaterialMode(usesOpaqueMaterial(next));
      }
    } catch (failure) {
      appearance = {
        ...appearance,
        glassOpacity: previous.glassOpacity,
        reducedTransparency: previous.reducedTransparency,
      };
      try {
        saveAppearance(localStorage, appearance);
      } catch {
        /* Keep the working session state. */
      }
      error = errorMessage(failure);
    }
  }

  function closeArtwork() {
    artworkOverlayOpen = false;
    if (viewportWidth >= 1180) void updateAppearance({ showNowPlaying: false });
  }

  function toggleArtwork() {
    if (viewportWidth >= 1180) void updateAppearance({ showNowPlaying: !artworkVisible });
    else artworkOverlayOpen = !artworkOverlayOpen;
  }

  function keyboard(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      navigate('search');
      searchInput?.select();
      return;
    }
    if (event.code === 'Escape') {
      settingsOpen = false;
      accountOpen = false;
      queueOpen = false;
      artworkOverlayOpen = false;
      if (pending) void cancel();
    }
    if (event.repeat || event.metaKey || event.ctrlKey || event.altKey) return;
    if (
      event.target instanceof HTMLElement &&
      event.target.closest('input, select, textarea, button, a, [contenteditable]')
    )
      return;
    if (event.code === 'Space' && ['playing', 'paused', 'buffering'].includes(playback.status)) {
      event.preventDefault();
      void control(() => setPaused(playback.status !== 'paused'));
    }
    if (event.key.toLowerCase() === 'm') {
      event.preventDefault();
      void mute();
    }
    if (
      (event.code === 'ArrowLeft' || event.code === 'ArrowRight') &&
      ['playing', 'paused', 'buffering'].includes(playback.status)
    ) {
      event.preventDefault();
      void control(() =>
        seekTrack(Math.max(0, playback.positionSeconds + (event.code === 'ArrowRight' ? 5 : -5))),
      );
    }
  }

  async function visibility() {
    if (document.visibilityState === 'visible' && report) {
      try {
        await refreshSession();
        void syncAccountLibrary();
      } catch (failure) {
        error = errorMessage(failure);
      }
    }
  }
</script>

<svelte:window
  onkeydown={keyboard}
  onfocus={() => void visibility()}
  bind:innerWidth={viewportWidth}
/>
<svelte:document onvisibilitychange={visibility} />
<svelte:head><title>Vessel — Your music, a little lighter.</title></svelte:head>

<div
  use:unifiedMaterial
  class="app-frame accent-{appearance.accent}"
  aria-busy={checking}
  class:opaque={usesOpaqueMaterial(appearance)}
  class:light-glass={usesLightGlass(appearance)}
  class:contrast-ink={!usesOpaqueMaterial(appearance) && appearance.glassOpacity < 70}
  class:sidebar-compact={appearance.sidebarCollapsed}
  class:density-compact={appearance.density === 'compact'}
  class:with-now-playing={artworkVisible && viewportWidth >= 1180}
  style:--glass-opacity={appearance.glassOpacity / 100}
>
  <div class="window-drag-region" data-tauri-drag-region></div>
  <aside class="sidebar" aria-label="Main navigation">
    <div class="brand">
      <span class="brand-symbol"><AudioLines size={22} strokeWidth={1.7} /></span><span
        >vessel<span class="brand-period">.</span></span
      >
    </div>
    <nav>
      <p class="sidebar-label">YOUR MUSIC</p>
      <button
        class:active={view === 'home'}
        onclick={() => navigate('home')}
        aria-current={view === 'home' ? 'page' : undefined}
        title="Listen now"><House size={18} /><span>Listen now</span></button
      >
      <button
        class:active={view === 'search'}
        onclick={() => navigate('search')}
        aria-current={view === 'search' ? 'page' : undefined}
        title="Search"><Search size={18} /><span>Search</span><kbd>⌘ K</kbd></button
      >
      <p class="sidebar-label collection-label">YOUR COLLECTION</p>
      <button
        class:active={view === 'liked'}
        onclick={() => navigate('liked')}
        aria-current={view === 'liked' ? 'page' : undefined}
        title="Liked songs"
        ><Heart size={18} /><span>Liked songs</span>{#if accountLibrary.likesReady}<small
            >{accountLibrary.likedIds.length}</small
          >{/if}</button
      >
      <button
        class:active={view === 'recent'}
        onclick={() => navigate('recent')}
        aria-current={view === 'recent' ? 'page' : undefined}
        title="Recently played"><Clock3 size={18} /><span>Recently played</span></button
      >
    </nav>
    {#if hasAccountCollection}
      <section class="sidebar-playlists" aria-label="Saved playlists">
        <div class="account-playlists-heading">
          <p class="sidebar-label">YOUTUBE MUSIC PLAYLISTS</p>
          <button
            class="icon-button"
            aria-label="Refresh YouTube Music library"
            title="Refresh YouTube Music library"
            disabled={account.mode !== 'connected' || libraryLoading || accountLibrary.refreshing}
            onclick={() => void syncAccountLibrary(true)}
            ><RefreshCw
              size={13}
              class={libraryLoading || accountLibrary.refreshing ? 'spin' : ''}
            /></button
          >
        </div>
        <nav aria-label="YouTube Music playlists">
          {#each accountLibrary.playlists as item (item.id)}
            <button
              class:active={view === 'collection' &&
                collectionFromAccount &&
                collection?.item.id === item.id}
              aria-current={view === 'collection' &&
              collectionFromAccount &&
              collection?.item.id === item.id
                ? 'page'
                : undefined}
              title={item.title}
              onclick={() => void openCollection(item, false, true)}
              ><ListMusic size={18} /><span>{item.title}</span></button
            >
          {/each}
          {#if account.mode !== 'connected'}<p class="sidebar-library-status">
              Reconnect your account to sync.
            </p>
          {:else if libraryLoading || accountLibrary.refreshing}<p
              class="sidebar-library-status"
              role="status"
            >
              Syncing your library…
            </p>
          {:else if libraryError || accountLibrary.playlistsError}<p
              class="sidebar-library-status"
              role="status"
            >
              Playlists could not sync. Refresh to retry.
            </p>
          {:else if accountLibrary.playlistsReady && !accountLibrary.playlists.length}<p
              class="sidebar-library-status"
            >
              Saved playlists will appear here.
            </p>{/if}
        </nav>
      </section>
    {/if}
    <div class="sidebar-bottom">
      <div class="native-note">
        <span class="little-star">✳</span>
        <p>A little lighter.<br /><span>A little more music.</span></p>
      </div>
      <button
        class="settings-trigger"
        data-popover-trigger="appearance"
        aria-label="Appearance settings"
        aria-expanded={settingsOpen}
        onclick={() => {
          settingsOpen = !settingsOpen;
          accountOpen = false;
          queueOpen = false;
        }}
        title="Appearance settings"><Settings2 size={17} /><span>Appearance</span></button
      >
      <button
        class="session-label"
        data-popover-trigger="account"
        aria-label="Account settings"
        onclick={() => {
          accountOpen = !accountOpen;
          settingsOpen = false;
          queueOpen = false;
        }}
        ><span></span><span
          >{account.profile?.name ??
            (account.mode === 'signingIn' ? 'Connecting Google…' : 'Guest session')}</span
        ></button
      >
    </div>
  </aside>

  <div class="main-stage">
    <header class="topbar">
      <span class="page-breadcrumb"
        >{view === 'home'
          ? 'Listen now'
          : view === 'search'
            ? 'Search'
            : 'Your collection'}<ChevronRight size={12} /></span
      >
      <form
        class="search-field"
        onsubmit={(event) => {
          event.preventDefault();
          void runSearch();
        }}
        role="search"
      >
        <Search size={16} aria-hidden="true" /><input
          type="search"
          bind:this={searchInput}
          bind:value={query}
          oninput={typing}
          placeholder="Songs, artists, a feeling…"
          aria-label="Search music"
          maxlength="160"
          autocomplete="off"
          spellcheck="false"
        />
        {#if query}<button
            type="button"
            aria-label="Clear search"
            onclick={() => {
              query = '';
              typing();
              searchInput.focus();
            }}><X size={13} /></button
          >{:else}<kbd>⌘ K</kbd>{/if}
      </form>
      <button
        class="theme-mode-trigger icon-button"
        aria-label={appearance.colorMode === 'light'
          ? 'Switch to dark appearance'
          : 'Switch to light appearance'}
        title={appearance.colorMode === 'light'
          ? 'Switch to dark appearance'
          : 'Switch to light appearance'}
        onclick={() =>
          void updateAppearance({ colorMode: appearance.colorMode === 'light' ? 'dark' : 'light' })}
        >{#if appearance.colorMode === 'light'}<Moon size={17} />{:else}<Sun
            size={17}
          />{/if}</button
      >
      <button
        class="guest-avatar"
        data-popover-trigger="account"
        title={account.profile?.name ?? 'Listening without an account'}
        aria-label="Open account settings"
        aria-expanded={accountOpen}
        onclick={() => {
          accountOpen = !accountOpen;
          settingsOpen = false;
          queueOpen = false;
        }}
      >
        {account.profile?.name.slice(0, 1).toUpperCase() ?? 'G'}
      </button>
    </header>

    <main class="content-scroll" id="main-content">
      {#if !checking && !report}<div class="notice preview-notice">
          Browser preview · Open the desktop app to search and listen.
        </div>{/if}
      {#if error || playback.error}
        <div class="notice error-notice" role="alert">
          <span>{error ?? playback.error?.message}</span><button
            class="icon-button"
            aria-label="Dismiss message"
            onclick={() => {
              error = null;
              playback = { ...playback, error: null };
            }}><X size={15} /></button
          >
        </div>
      {/if}

      {#if view === 'home'}
        <HomeView
          feed={home}
          loading={homeLoading}
          failed={homeError}
          connected={Boolean(report)}
          pendingId={pending?.id}
          onrefresh={() => void loadHome(true)}
          onplay={(track, tracks) => void select(track, tracks)}
          onbrowse={(item) => void openCollection(item)}
        />
      {:else if view === 'collection' && collection}
        <section class="provider-collection-heading">
          <Artwork url={collection.item.thumbnailUrl} className="collection-page-art" size={256} />
          <div>
            <button class="text-button" onclick={() => navigate('home')}
              >YouTube Music<ChevronRight size={12} /></button
            >
            <p class="eyebrow">{collection.item.kind}</p>
            <h1>{collection.item.title}</h1>
            <p>{collection.item.subtitle}</p>
          </div>
        </section>
        {#if collectionLoading}<div class="empty-collection" role="status">
            <LoaderCircle size={24} class="spin" />
            <p>Loading songs from YouTube Music…</p>
          </div>
        {:else if collection.tracks.length}<TrackList
            tracks={collection.tracks}
            activeId={playback.track?.id}
            {playing}
            pendingId={pending?.id}
            {likedIds}
            {likesDisabled}
            {likeDestination}
            onplay={(track) => void select(track)}
            onlike={(track) => void like(track)}
            onartist={find}
          />{#if collection.hasMore}<div class="load-more">
              <button
                class="secondary-button"
                disabled={collectionMore}
                onclick={() => {
                  if (collection) void openCollection(collection.item, true, collectionFromAccount);
                }}>{collectionMore ? 'Loading songs…' : 'Show more songs'}</button
              >
            </div>{/if}
        {:else}<div class="empty-collection">
            <Music2 size={30} />
            <h2>No playable songs were returned.</h2>
            <p>Try another collection or refresh Listen now.</p>
          </div>{/if}
      {:else if view === 'search'}
        <section class="page-heading">
          <p class="eyebrow">THE MUSIC YOU'RE LOOKING FOR</p>
          <h1>
            {results
              ? `Results for “${results.query}”`
              : searching
                ? 'Finding your music…'
                : 'What sounds good today?'}
          </h1>
          <p>Search the YouTube Music catalog. Press play. Make yourself at home.</p>
        </section>
        <div class="result-toolbar">
          <div class="filter-pills" aria-label="Search category">
            <button
              class:chosen={kind === 'songs'}
              aria-pressed={kind === 'songs'}
              onclick={() => filter('songs')}><Music2 size={14} />Songs</button
            ><button
              class:chosen={kind === 'videos'}
              aria-pressed={kind === 'videos'}
              onclick={() => filter('videos')}><AudioLines size={14} />Music videos</button
            >
          </div>
          {#if results}<span class="result-count"
              >{results.tracks.length} {results.kind === 'songs' ? 'songs' : 'music videos'}</span
            >{/if}
        </div>
        {#if searching}<div class="search-skeleton" role="status" aria-label="Searching music">
            {#each [1, 2, 3, 4, 5, 6] as row (row)}<div>
                <span class="skeleton"></span>
                <p class="skeleton"></p>
                <small class="skeleton"></small>
              </div>{/each}
          </div>
        {:else if results?.tracks.length}<TrackList
            tracks={results.tracks}
            activeId={playback.track?.id}
            {playing}
            pendingId={pending?.id}
            {likedIds}
            {likesDisabled}
            {likeDestination}
            onplay={(track) => void select(track)}
            onlike={(track) => void like(track)}
            onartist={find}
          />{#if results.hasMore}<div class="load-more">
              <button
                class="secondary-button"
                disabled={loadingMore}
                onclick={() => runSearch(true)}
                >{#if loadingMore}<LoaderCircle size={14} class="spin" />{/if}{loadingMore
                  ? 'Finding more…'
                  : 'Show more songs'}</button
              >
            </div>{/if}
        {:else if results}<div class="empty-collection">
            <Search size={32} strokeWidth={1.3} />
            <h2>No matches this time.</h2>
            <p>Try a different song title or artist name.</p>
          </div>
        {:else}<div class="search-welcome">
            <span class="search-welcome-orbit"><Search size={32} strokeWidth={1.2} /></span>
            <h2>There's a song for that.</h2>
            <p>
              A favorite artist. A half-remembered lyric.<br />Start with whatever's on your mind.
            </p>
            <button class="secondary-button" onclick={() => navigate('home')}
              >Explore YouTube Music<ArrowRight size={14} /></button
            >
          </div>{/if}
      {:else}
        <section class="collection-heading">
          <span class="collection-icon" class:heart={view === 'liked'}
            >{#if view === 'liked'}<Heart size={34} strokeWidth={1.3} />{:else}<Clock3
                size={34}
                strokeWidth={1.3}
              />{/if}</span
          >
          <div>
            <p class="eyebrow">
              YOUR COLLECTION · {view === 'liked' ? 'YOUTUBE MUSIC' : 'ON THIS DEVICE'}
            </p>
            <h1>
              {view === 'liked' ? 'Liked songs' : 'Recently played'}
            </h1>
            <p>
              {listTracks.length} songs · {view === 'liked'
                ? 'Your YouTube Music favorites.'
                : 'Pick up a familiar feeling.'}
            </p>
          </div>
        </section>
        {#if view === 'liked' && account.mode === 'connected'}
          <div class="account-library-toolbar">
            <span role="status"
              >{likeBusy
                ? 'Saving changes to YouTube Music…'
                : libraryLoading || accountLibrary.refreshing
                  ? 'Syncing your library…'
                  : account.mode !== 'connected'
                    ? 'Reconnect your Google account to sync likes.'
                    : accountLibrary.likesReady
                      ? `${accountLibrary.likedIds.length} account likes`
                      : 'Your account likes have not synced yet.'}</span
            >
            <button
              class="secondary-button"
              disabled={account.mode !== 'connected' || libraryLoading || accountLibrary.refreshing}
              onclick={() => void syncAccountLibrary(true)}
              ><RefreshCw size={14} />Refresh likes</button
            >
          </div>
          {#if libraryError || accountLibrary.likesError}<div
              class="notice error-notice"
              role="alert"
            >
              {libraryError ?? accountLibrary.likesError?.message}
            </div>{/if}
        {/if}
        {#if listTracks.length}<TrackList
            tracks={listTracks}
            activeId={playback.track?.id}
            {playing}
            pendingId={pending?.id}
            {likedIds}
            {likesDisabled}
            {likeDestination}
            onplay={(track) => void select(track)}
            onlike={(track) => void like(track)}
            onartist={find}
          />
        {:else if view === 'liked' && !accountLibrary.likesReady}<div
            class="empty-collection"
            role="status"
          >
            {#if libraryLoading || accountLibrary.refreshing}<LoaderCircle
                size={28}
                class="spin"
              />{:else}<Heart size={28} />{/if}
            <h2>
              {libraryLoading || accountLibrary.refreshing
                ? 'Bringing your favorites home…'
                : account.mode === 'connected'
                  ? 'Your account likes are not available yet.'
                  : 'Connect to see your liked songs.'}
            </h2>
            <p>
              {account.error?.message ?? 'Your liked songs come from your YouTube Music account.'}
            </p>
            {#if account.mode !== 'connected'}<button
                class="primary-button"
                disabled={accountBusy ||
                  !report ||
                  account.mode === 'checking' ||
                  account.mode === 'signingIn'}
                onclick={() => void accountAction(connectGoogle)}>Connect Google</button
              >{/if}
          </div>
        {:else}<div class="empty-collection">
            <Library size={32} strokeWidth={1.3} />
            <h2>
              {view === 'liked'
                ? 'Make room for your favorites.'
                : 'Your listening story starts here.'}
            </h2>
            <p>
              {view === 'liked'
                ? 'Tap the heart beside a song to keep it here.'
                : 'The songs you listen to will find a home here.'}
            </p>
            <button class="secondary-button" onclick={() => navigate('search')}
              >Find a song<ArrowRight size={14} /></button
            >
          </div>{/if}
      {/if}
    </main>
  </div>

  {#if artworkVisible && shownTrack}
    <NowPlayingPanel
      track={shownTrack}
      {playback}
      loading={Boolean(loadingTrack)}
      liked={likedIds.has(shownTrack.id)}
      {likesDisabled}
      {likeDestination}
      onclose={closeArtwork}
      onlike={() => void like(shownTrack)}
    />
  {/if}

  <PlayerBar
    {playback}
    pending={loadingTrack}
    {inspection}
    {busy}
    liked={Boolean(playback.track && likedIds.has(playback.track.id))}
    {likesDisabled}
    {likeDestination}
    transitioning={queue.transitioning}
    canPrevious={currentIndex >= 0}
    canNext={currentIndex >= 0 &&
      (currentIndex < queue.tracks.length - 1 || (queue.autoplay && queue.running))}
    onpause={() => void control(() => setPaused(playback.status !== 'paused'))}
    onseek={(position) => control(() => seekTrack(position))}
    onvolume={volume}
    onmute={() => void mute()}
    onstop={() => {
      void cancel();
      void control(stopPlayback);
    }}
    oncancel={() => void cancel()}
    onprevious={() => void transport(previousSong)}
    onnext={() => void transport(nextSong)}
    onlike={() => {
      if (playback.track) void like(playback.track);
    }}
    onquality={(id) => void quality(id)}
    onartwork={toggleArtwork}
    {artworkVisible}
    {queueOpen}
    onqueue={() => {
      queueOpen = !queueOpen;
      accountOpen = false;
      settingsOpen = false;
    }}
  />

  {#if settingsOpen}<AppearancePanel
      preferences={appearance}
      onchange={(change, persist) => void updateAppearance(change, persist)}
      onclose={() => {
        settingsOpen = false;
      }}
      onreset={() => {
        artworkOverlayOpen = false;
        void updateAppearance(
          defaultAppearance(matchMedia('(prefers-reduced-transparency: reduce)').matches),
        );
      }}
    />{/if}
  {#if queueOpen}<QueuePanel
      {queue}
      onclose={() => {
        queueOpen = false;
      }}
      onjump={(index, videoId) => void transport(() => jumpQueue(index, videoId))}
      onremove={(index, videoId) => void queueAction(() => removeQueue(index, videoId))}
      onclear={() => {
        void cancel();
        void queueAction(clearQueue);
      }}
      onautoplay={(enabled) => void queueAction(() => setAutoplay(enabled))}
      onretry={() => void queueAction(retryRadio)}
    />{/if}
  {#if accountOpen}<AccountPanel
      {account}
      available={Boolean(report)}
      busy={accountBusy}
      onclose={() => {
        accountOpen = false;
      }}
      onconnect={() => void accountAction(connectGoogle)}
      oncancel={() => void accountAction(cancelGoogle)}
      onsignout={() => {
        void cancel();
        void accountAction(signOut);
      }}
    />{/if}
</div>
