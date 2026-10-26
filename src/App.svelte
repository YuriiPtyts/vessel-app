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
  import {
    defaultAppearance,
    loadAppearance,
    normalizeAppearance,
    saveAppearance,
    type AppearancePreferences,
  } from './lib/appearance';
  import { createVolumeControl } from './lib/volume';
  import {
    cancelInspection,
    emptyPlayback,
    errorMessage,
    getLibrary,
    getPlayback,
    getRuntimeReport,
    getHomeFeed,
    browseCollection,
    isCancelled,
    onPlayback,
    playSong,
    playTrack,
    searchMusic,
    seekTrack,
    setMaterialMode,
    setPaused,
    setVolume,
    stopPlayback,
    toggleLike,
    type LocalLibrary,
    type PlaybackSnapshot,
    type RuntimeReport,
    type SearchKind,
    type SearchResults,
    type Track,
    type TrackInspection,
    type HomeFeed,
    type HomeItem,
    type CollectionPage,
  } from './lib/native';

  type View = 'home' | 'search' | 'liked' | 'recent' | 'collection';
  let view = $state<View>('home');
  let report = $state<RuntimeReport | null>(null);
  let checking = $state(true);
  let error = $state<string | null>(null);
  let library = $state<LocalLibrary>({ liked: [], recent: [] });
  let home = $state<HomeFeed | null>(null);
  let homeLoading = $state(false);
  let homeError = $state(false);
  let collection = $state<CollectionPage | null>(null);
  let collectionLoading = $state(false);
  let collectionMore = $state(false);
  let collectionRevision = 0;
  let query = $state('');
  let kind = $state<SearchKind>('songs');
  let results = $state<SearchResults | null>(null);
  let searching = $state(false);
  let loadingMore = $state(false);
  let playback = $state<PlaybackSnapshot>(emptyPlayback);
  let inspection = $state<TrackInspection | null>(null);
  let pending = $state<Track | null>(null);
  let context = $state<Track[]>([]);
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
  const volume = createVolumeControl(
    async (value) => {
      if (!report) return;
      const snapshot = await setVolume(value);
      // A volume reply must not roll track/transport state back to an older snapshot.
      playback = { ...playback, volume: snapshot.volume };
    },
    (failure) => {
      error = errorMessage(failure);
    },
  );

  const likedIds = $derived(new Set(library.liked.map((track) => track.id)));
  const playing = $derived(playback.status === 'playing' || playback.status === 'buffering');
  const shownTrack = $derived(playback.track ?? pending);
  const artworkVisible = $derived(
    Boolean(
      shownTrack && ((appearance.showNowPlaying && viewportWidth >= 1180) || artworkOverlayOpen),
    ),
  );
  const currentIndex = $derived(
    context.findIndex((track) => track.id === (pending?.id ?? playback.track?.id)),
  );
  const listTracks = $derived(
    view === 'liked'
      ? library.liked
      : view === 'recent'
        ? library.recent
        : view === 'collection'
          ? (collection?.tracks ?? [])
          : (results?.tracks ?? []),
  );

  onMount(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
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
          const unlisten = await onPlayback((snapshot) => {
            playback = snapshot;
            if (snapshot.status === 'playing' && snapshot.track?.id !== lastHistoryId) {
              lastHistoryId = snapshot.track?.id;
              void refreshLibrary();
            }
          });
          if (disposed) {
            unlisten();
            return;
          }
          unsubscribe = unlisten;
          playback = await getPlayback();
          await Promise.all([refreshLibrary(), setMaterialMode(appearance.reducedTransparency)]);
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
      unsubscribe?.();
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

  async function loadHome(refresh = false) {
    if (!report || homeLoading) return;
    homeLoading = true;
    homeError = false;
    try {
      home = await getHomeFeed(refresh);
    } catch (failure) {
      if (!isCancelled(failure)) homeError = true;
    } finally {
      homeLoading = false;
    }
  }

  async function openCollection(item: HomeItem, more = false) {
    if (!report) return;
    const revision = ++collectionRevision;
    view = 'collection';
    error = null;
    if (more) collectionMore = true;
    else {
      collection = { item, tracks: [], hasMore: false };
      collectionLoading = true;
    }
    try {
      const page = await browseCollection(item.id, more);
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
    if (next === 'search') searchInput?.focus();
    if (next === 'home' && !home && !homeLoading && report) void loadHome();
    if (next === 'liked' || next === 'recent') void refreshLibrary();
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
      playback = await action();
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
    context = tracks;
    error = null;
    try {
      const selection = await playSong(track.id);
      if (revision === playbackRevision) {
        inspection = selection.inspection;
        playback = selection.playback;
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
    if (!report) return;
    try {
      library = await toggleLike(track.id);
    } catch (failure) {
      error = errorMessage(failure);
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
      if (report && next.reducedTransparency !== previous.reducedTransparency) {
        await setMaterialMode(next.reducedTransparency);
      }
    } catch (failure) {
      appearance = { ...appearance, reducedTransparency: previous.reducedTransparency };
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
        playback = await getPlayback();
      } catch (failure) {
        error = errorMessage(failure);
      }
    }
  }
</script>

<svelte:window onkeydown={keyboard} bind:innerWidth={viewportWidth} />
<svelte:document onvisibilitychange={visibility} />
<svelte:head><title>Vessel — Your music, a little lighter.</title></svelte:head>

<div
  class="app-frame accent-{appearance.accent}"
  class:opaque={appearance.reducedTransparency}
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
        ><Heart size={18} /><span>Liked songs</span>{#if library.liked.length}<small
            >{library.liked.length}</small
          >{/if}</button
      >
      <button
        class:active={view === 'recent'}
        onclick={() => navigate('recent')}
        aria-current={view === 'recent' ? 'page' : undefined}
        title="Recently played"><Clock3 size={18} /><span>Recently played</span></button
      >
    </nav>
    <div class="sidebar-bottom">
      <div class="native-note">
        <span class="little-star">✳</span>
        <p>A little lighter.<br /><span>A little more music.</span></p>
      </div>
      <button
        class="settings-trigger"
        aria-label="Appearance settings"
        aria-expanded={settingsOpen}
        onclick={() => {
          settingsOpen = !settingsOpen;
        }}
        title="Appearance settings"><Settings2 size={17} /><span>Appearance</span></button
      >
      <div class="session-label"><span></span><span>Guest session</span></div>
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
      <div class="guest-avatar" title="Listening without an account" aria-label="Guest session">
        G
      </div>
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
            onplay={(track) => void select(track)}
            onlike={(track) => void like(track)}
            onartist={find}
          />{#if collection.hasMore}<div class="load-more">
              <button
                class="secondary-button"
                disabled={collectionMore}
                onclick={() => {
                  if (collection) void openCollection(collection.item, true);
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
            <p class="eyebrow">YOUR COLLECTION · ON THIS MAC</p>
            <h1>{view === 'liked' ? 'Liked songs' : 'Recently played'}</h1>
            <p>
              {listTracks.length} songs · {view === 'liked'
                ? 'Good music, kept close.'
                : 'Pick up a familiar feeling.'}
            </p>
          </div>
        </section>
        {#if listTracks.length}<TrackList
            tracks={listTracks}
            activeId={playback.track?.id}
            {playing}
            pendingId={pending?.id}
            {likedIds}
            onplay={(track) => void select(track)}
            onlike={(track) => void like(track)}
            onartist={find}
          />
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
      loading={Boolean(pending && !playback.track)}
      liked={likedIds.has(shownTrack.id)}
      onclose={closeArtwork}
      onlike={() => void like(shownTrack)}
    />
  {/if}

  <PlayerBar
    {playback}
    {pending}
    {inspection}
    {busy}
    liked={Boolean(playback.track && likedIds.has(playback.track.id))}
    canPrevious={currentIndex > 0}
    canNext={currentIndex >= 0 && currentIndex < context.length - 1}
    onpause={() => void control(() => setPaused(playback.status !== 'paused'))}
    onseek={(position) => control(() => seekTrack(position))}
    onvolume={volume}
    onstop={() => {
      void cancel();
      void control(stopPlayback);
    }}
    oncancel={() => void cancel()}
    onprevious={() => {
      if (currentIndex > 0) void select(context[currentIndex - 1], context);
    }}
    onnext={() => {
      if (currentIndex >= 0 && currentIndex < context.length - 1)
        void select(context[currentIndex + 1], context);
    }}
    onlike={() => {
      if (playback.track) void like(playback.track);
    }}
    onquality={(id) => void quality(id)}
    onartwork={toggleArtwork}
    {artworkVisible}
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
</div>
