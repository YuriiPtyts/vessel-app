<script lang="ts">
  import { onMount } from 'svelte';
  import {
    ArrowRight,
    AudioLines,
    Check,
    ChevronRight,
    Clock3,
    Compass,
    Heart,
    House,
    Library,
    LoaderCircle,
    Music2,
    Search,
    Settings2,
    Sparkles,
    X,
  } from '@lucide/svelte';
  import Artwork from './lib/components/Artwork.svelte';
  import TrackList from './lib/components/TrackList.svelte';
  import PlayerBar from './lib/components/PlayerBar.svelte';
  import {
    cancelInspection,
    emptyPlayback,
    errorMessage,
    getLibrary,
    getPlayback,
    getRuntimeReport,
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
  } from './lib/native';

  type View = 'home' | 'search' | 'liked' | 'recent';
  let view = $state<View>('home');
  let report = $state<RuntimeReport | null>(null);
  let checking = $state(true);
  let error = $state<string | null>(null);
  let library = $state<LocalLibrary>({ liked: [], recent: [] });
  let discovery = $state<Track[]>([]);
  let discovering = $state(false);
  let discoveryError = $state(false);
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
  let opaque = $state(false);
  let searchInput: HTMLInputElement;
  let searchRevision = 0;
  let playbackRevision = 0;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let lastHistoryId: string | undefined;

  const likedIds = $derived(new Set(library.liked.map((track) => track.id)));
  const playing = $derived(playback.status === 'playing' || playback.status === 'buffering');
  const currentIndex = $derived(
    context.findIndex((track) => track.id === (pending?.id ?? playback.track?.id)),
  );
  const spotlight = $derived.by(() => {
    const albums = new Set<string>();
    return discovery
      .filter((track) => {
        const key = track.album ?? track.id;
        if (albums.has(key)) return false;
        albums.add(key);
        return true;
      })
      .slice(0, 5);
  });
  const listTracks = $derived(
    view === 'liked' ? library.liked : view === 'recent' ? library.recent : (results?.tracks ?? []),
  );

  onMount(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    try {
      opaque =
        localStorage.getItem('vessel:opaque') === 'true' ||
        matchMedia('(prefers-reduced-transparency: reduce)').matches;
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
          await Promise.all([refreshLibrary(), setMaterialMode(opaque)]);
          if (!disposed && view === 'home') void discover();
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

  async function discover() {
    discovering = true;
    discoveryError = false;
    try {
      discovery = (await searchMusic('Khruangbin')).tracks;
    } catch (failure) {
      if (!isCancelled(failure)) discoveryError = true;
    } finally {
      discovering = false;
    }
  }

  function navigate(next: View) {
    view = next;
    settingsOpen = false;
    if (next === 'search') searchInput?.focus();
    if (next === 'home' && !discovery.length && !discovering && report) void discover();
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

  async function material() {
    const next = !opaque;
    try {
      if (report) await setMaterialMode(next);
      opaque = next;
      try {
        localStorage.setItem('vessel:opaque', String(next));
      } catch {
        /* Appearance still works for this session. */
      }
    } catch (failure) {
      error = errorMessage(failure);
    }
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

<svelte:window onkeydown={keyboard} />
<svelte:document onvisibilitychange={visibility} />
<svelte:head><title>Vessel — Your music, a little lighter.</title></svelte:head>

<div class="app-frame" class:opaque>
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
        <section class="home-heading">
          <div>
            <p class="eyebrow">MAKE YOURSELF AT HOME</p>
            <h1>A good day for good music<span>.</span></h1>
          </div>
          <span class="quiet-badge"><span></span>Just music. Nothing extra.</span>
        </section>
        <section class="discovery-hero" aria-labelledby="hero-title">
          <div class="hero-copy">
            <span class="hero-kicker"><Sparkles size={13} /> A LITTLE DISCOVERY</span>
            <h2 id="hero-title">Less noise.<br />More <em>feeling.</em></h2>
            <p>The songs you came for.<br />The ones you haven't met yet.</p>
            <button class="primary-button" onclick={() => navigate('search')}
              ><Search size={15} />Find your music<ArrowRight size={15} /></button
            >
          </div>
          <div class="hero-art" aria-hidden="true">
            <div class="vinyl"><span></span></div>
            <div class="floating-sleeve sleeve-back">
              <Artwork url={spotlight[1]?.thumbnailUrl} />
            </div>
            <div class="floating-sleeve sleeve-front">
              <Artwork url={spotlight[0]?.thumbnailUrl} /><span class="sleeve-label"
                >a little out of the ordinary.</span
              >
            </div>
            <span class="hero-orbit orbit-one"></span><span class="hero-orbit orbit-two"></span>
          </div>
          <span class="hero-edition">THE EVERYDAY LISTENING EDITION — 001</span>
        </section>
        <section class="mood-section" aria-labelledby="mood-title">
          <div class="section-heading">
            <h2 id="mood-title">Find your frequency</h2>
            <span>Follow a feeling</span>
          </div>
          <div class="mood-grid">
            {#each [{ label: 'Slow mornings', query: 'mellow morning acoustic', tone: 'peach', icon: '☀' }, { label: 'In the zone', query: 'ambient focus instrumental', tone: 'lavender', icon: '◌' }, { label: 'After hours', query: 'late night jazz', tone: 'blue', icon: '☾' }, { label: 'A little energy', query: 'indie dance', tone: 'green', icon: '↗' }] as mood}
              <button class="mood-card {mood.tone}" onclick={() => find(mood.query)}
                ><span class="mood-symbol">{mood.icon}</span><span>{mood.label}</span><ArrowRight
                  size={14}
                /></button
              >
            {/each}
          </div>
        </section>
        <section class="spotlight-section" aria-labelledby="spotlight-title">
          <div class="section-heading">
            <div>
              <p class="eyebrow">ARTIST SPOTLIGHT</p>
              <h2 id="spotlight-title">A detour with Khruangbin</h2>
            </div>
            <button class="text-button" onclick={() => find('Khruangbin')}
              >Explore artist<ArrowRight size={14} /></button
            >
          </div>
          {#if discovering}<div class="album-grid" aria-label="Loading music">
              {#each [1, 2, 3, 4, 5] as card (card)}<div class="card-skeleton">
                  <div class="skeleton"></div>
                  <span class="skeleton"></span>
                </div>{/each}
            </div>
          {:else if spotlight.length}<div class="album-grid">
              {#each spotlight as track (track.id)}<button
                  class="music-card"
                  onclick={() => select(track, discovery)}
                  aria-label={`Play ${track.title}`}
                  ><span class="music-card-art"
                    ><Artwork url={track.thumbnailUrl} /><span class="card-play"
                      >{#if pending?.id === track.id}<LoaderCircle
                          size={20}
                          class="spin"
                        />{:else}<Music2 size={19} />{/if}</span
                    ></span
                  ><strong>{track.title}</strong><span>{track.artist}</span></button
                >{/each}
            </div>
          {:else}<div class="spotlight-empty">
              <Compass size={23} />
              <p>
                {discoveryError
                  ? 'Discovery is taking a little break. Your next favorite is still a search away.'
                  : 'A world of music is a search away.'}
              </p>
              {#if report}<button class="text-button" onclick={discover}
                  >Try discovery again<ArrowRight size={13} /></button
                >{/if}
            </div>{/if}
        </section>
        <div class="home-signoff">
          <AudioLines size={15} /><span>Your music, a little lighter.</span><span
            class="signoff-line"
          ></span><span>Vessel</span>
        </div>
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
            <div class="suggestion-chips">
              {#each ['Daft Punk', 'Sade', 'Tame Impala', 'Khruangbin'] as artist}<button
                  onclick={() => find(artist)}>{artist}<ArrowRight size={12} /></button
                >{/each}
            </div>
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
    onvolume={(value) => control(() => setVolume(value))}
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
  />

  {#if settingsOpen}<section class="appearance-popover" aria-label="Appearance settings">
      <div class="popover-heading">
        <h2>A little more you.</h2>
        <button
          class="icon-button"
          aria-label="Close appearance settings"
          onclick={() => {
            settingsOpen = false;
          }}><X size={15} /></button
        >
      </div>
      <button class="appearance-option" role="switch" aria-checked={opaque} onclick={material}
        ><span
          ><strong>Reduce transparency</strong><small>A solid backdrop, with the same calm.</small
          ></span
        ><span class="toggle" class:on={opaque}
          >{#if opaque}<Check size={11} />{/if}</span
        ></button
      >
    </section>{/if}
</div>
