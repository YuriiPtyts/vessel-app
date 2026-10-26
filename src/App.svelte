<script lang="ts">
  import { onMount } from 'svelte';
  import { AudioLines, Check, ChevronRight, Link, Music2, Pause, Play, RefreshCw, Square, Volume2, X } from '@lucide/svelte';
  import {
    cancelInspection, emptyPlayback, errorMessage, formatLabel, getPlayback, getRuntimeReport,
    inspectTrack, onPlayback, playTrack, seekTrack, setPaused, setVolume, stopPlayback,
    type PlaybackSnapshot, type RuntimeReport, type TrackInspection,
  } from './lib/native';

  let report = $state<RuntimeReport | null>(null);
  let checking = $state(true);
  let nativeError = $state<string | null>(null);
  let error = $state<string | null>(null);
  let url = $state('');
  let inspection = $state<TrackInspection | null>(null);
  let inspecting = $state(false);
  let cancelling = $state(false);
  let busy = $state(false);
  let selectedFormat = $state('auto');
  let playback = $state<PlaybackSnapshot>(emptyPlayback);
  let seekDraft = $state<number | null>(null);
  let volumeDraft = $state<number | null>(null);

  const ready = $derived(Boolean(report?.playbackDependenciesAvailable));
  const active = $derived(['playing', 'paused', 'buffering'].includes(playback.status));
  const selectedId = $derived(selectedFormat === 'auto' ? inspection?.recommendedFormatId : selectedFormat);
  const sameSelection = $derived(inspection?.track.id === playback.track?.id && selectedId === playback.activeFormat?.id);
  const selectionLabel = $derived(sameSelection && playback.status === 'playing' ? 'Playing'
    : sameSelection && playback.status === 'paused' ? 'Resume'
    : inspection?.track.id === playback.track?.id && active ? 'Apply quality' : 'Play track');

  onMount(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    async function initialize() {
      try {
        const runtime = await getRuntimeReport();
        if (disposed) return;
        report = runtime;
        if (runtime) {
          const unlisten = await onPlayback((snapshot) => { playback = snapshot; });
          if (disposed) { unlisten(); return; }
          unsubscribe = unlisten;
          playback = await getPlayback();
        }
      } catch (error) {
        if (!disposed) nativeError = errorMessage(error);
      } finally {
        if (!disposed) checking = false;
      }
    }
    void initialize();
    return () => { disposed = true; unsubscribe?.(); };
  });

  async function inspect(event: SubmitEvent) {
    event.preventDefault();
    if (!ready || inspecting) return;
    inspecting = true;
    error = null;
    inspection = null;
    try {
      inspection = await inspectTrack(url);
      selectedFormat = 'auto';
    } catch (failure) {
      if (!(typeof failure === 'object' && failure !== null && 'code' in failure && failure.code === 'cancelled')) {
        error = errorMessage(failure);
      }
    } finally {
      inspecting = false;
      cancelling = false;
    }
  }

  async function cancel() {
    cancelling = true;
    try { await cancelInspection(); }
    catch (failure) { error = errorMessage(failure); cancelling = false; }
  }

  async function control(action: () => Promise<PlaybackSnapshot>) {
    if (busy) return;
    busy = true;
    error = null;
    try { playback = await action(); }
    catch (failure) { error = errorMessage(failure); }
    finally { busy = false; }
  }

  async function playSelection() {
    if (!inspection || !selectedId) return;
    const trackId = inspection.track.id;
    const formatId = selectedId;
    if (sameSelection && playback.status === 'paused') await control(() => setPaused(false));
    else await control(() => playTrack(trackId, formatId));
  }

  async function seek(value: number) {
    await control(() => seekTrack(value));
    seekDraft = null;
  }

  async function volume(value: number) {
    await control(() => setVolume(value));
    volumeDraft = null;
  }

  function keyboard(event: KeyboardEvent) {
    if (event.repeat || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.target instanceof HTMLElement && event.target.closest('input, select, textarea, button, a, [contenteditable]')) return;
    if (event.code === 'Space' && active && !busy) {
      event.preventDefault();
      void control(() => setPaused(playback.status !== 'paused'));
    }
    if ((event.code === 'ArrowLeft' || event.code === 'ArrowRight') && active && !busy) {
      event.preventDefault();
      void seek(Math.max(0, playback.positionSeconds + (event.code === 'ArrowRight' ? 5 : -5)));
    }
    if (event.code === 'Escape' && inspecting && !cancelling) void cancel();
  }

  function time(seconds: number | null) {
    if (seconds === null || !Number.isFinite(seconds)) return '—:—';
    const value = Math.max(0, Math.floor(seconds));
    return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}`;
  }

  async function visibility() {
    if (document.visibilityState === 'visible' && report) {
      try { playback = await getPlayback(); }
      catch (failure) { nativeError = errorMessage(failure); }
    }
  }
</script>

<svelte:window onkeydown={keyboard} />
<svelte:document onvisibilitychange={visibility} />

<svelte:head><title>Vessel — Playback Lab</title></svelte:head>

<main class="mx-auto flex min-h-screen max-w-4xl flex-col px-7 py-9 sm:px-14 sm:py-12">
  <header class="flex items-center justify-between gap-4">
    <div class="flex items-center gap-3 font-medium tracking-tight">
      <span class="brand-mark"><AudioLines size={21} strokeWidth={1.6} /></span>
      <span class="text-xl">Vessel</span>
    </div>
    <span class="guest-badge"><span class="status-dot"></span>Guest listening</span>
  </header>

  <section class="mt-12" aria-labelledby="lab-title">
    <p class="eyebrow accent">Playback lab / 002</p>
    <h1 id="lab-title" class="mt-3 text-3xl leading-tight font-medium tracking-tight sm:text-4xl">Start with a song.</h1>
    <p class="muted mt-4 max-w-lg text-sm leading-relaxed">Paste a YouTube Music link. Discover the actual audio formats, choose your quality, and listen without signing in.</p>
  </section>

  {#if checking || nativeError || !report || !ready}
    <aside class="notice mt-6" aria-live="polite">
      {#if checking}Connecting to the native player…
      {:else if nativeError}{nativeError}
      {:else if !report}Browser preview. Launch <code>npm run tauri dev</code> for native playback.
      {:else}Playback dependencies are missing. Run <code>npm run prepare:native</code> and rebuild the desktop app.{/if}
    </aside>
  {/if}

  <form class="mt-7" onsubmit={inspect}>
    <label class="eyebrow" for="track-url">Track link</label>
    <div class="url-row mt-2">
      <Link size={17} class="muted shrink-0" aria-hidden="true" />
      <input id="track-url" type="url" bind:value={url} required placeholder="https://music.youtube.com/watch?v=…" autocomplete="off" spellcheck="false" disabled={!ready || inspecting} />
      {#if inspecting}
        <button type="button" class="subtle-button" onclick={cancel} disabled={cancelling} aria-label="Cancel track inspection"><X size={17} aria-hidden="true" />{cancelling ? 'Cancelling…' : 'Cancel'}</button>
      {:else}
        <button class="primary-button" disabled={!ready || !url.trim()}><ChevronRight size={17} aria-hidden="true" />Inspect</button>
      {/if}
    </div>
    {#if inspecting}
      <p class="muted mt-3 flex items-center gap-2 text-xs" role="status"><RefreshCw size={13} aria-hidden="true" />Resolving formats and waiting for the provider to make them available. This can take 10–20 seconds.</p>
    {/if}
  </form>

  {#if error}
    <p class="notice error mt-5" role="alert">{error}</p>
  {/if}

  {#if inspection}
    <section class="panel mt-6" aria-labelledby="selection-title">
      <div class="flex items-start gap-4">
        <div class="track-art"><Music2 size={26} strokeWidth={1.4} aria-hidden="true" /></div>
        <div class="min-w-0">
          <p class="eyebrow accent">Ready to listen · {time(inspection.track.durationSeconds)}</p>
          <h2 id="selection-title" class="mt-2 text-base leading-snug font-medium">{inspection.track.title}</h2>
          <p class="muted mt-1 text-sm">{inspection.track.artist}</p>
        </div>
      </div>
      <div class="quality-row mt-6">
        <div class="min-w-0 flex-1">
          <label for="quality" class="eyebrow">Audio quality</label>
          <select id="quality" class="quality-select mt-2" bind:value={selectedFormat} disabled={busy}>
            <option value="auto">Best available · provider-ranked</option>
            {#each inspection.formats as format (format.id)}
              <option value={format.id}>{formatLabel(format)} · #{format.id}</option>
            {/each}
          </select>
        </div>
        <button class="primary-button play-selection" onclick={playSelection} disabled={busy || playback.status === 'loading' || (sameSelection && playback.status === 'playing')}>
          {#if sameSelection && playback.status === 'playing'}<Check size={16} aria-hidden="true" />{:else}<Play size={16} fill="currentColor" aria-hidden="true" />{/if}
          {selectionLabel}
        </button>
      </div>
      <p class="muted mt-3 text-xs leading-relaxed">Bitrates are source-reported estimates. Codec differences matter; a higher number alone does not establish better sound. No verified lossless source is available.</p>
    </section>
  {:else if !inspecting}
    <section class="empty-state mt-8" aria-label="Waiting for a track">
      <AudioLines size={30} strokeWidth={1.3} aria-hidden="true" />
      <p class="mt-4 text-sm">Your music, with a little more breathing room.</p>
      <p class="muted mt-2 text-xs leading-relaxed">Audio stays in the native player.<br />No video decoding. No account required for available public tracks.</p>
    </section>
  {/if}

  <section class="player-panel mt-8" aria-labelledby="player-title">
    <div class="flex items-center gap-4">
      <button class="transport" aria-label={playback.status === 'paused' ? 'Resume playback' : 'Pause playback'} disabled={!active || busy} onclick={() => control(() => setPaused(playback.status !== 'paused'))}>
        {#if playback.status === 'paused'}<Play size={22} fill="currentColor" aria-hidden="true" />{:else}<Pause size={22} fill="currentColor" aria-hidden="true" />{/if}
      </button>
      <div class="min-w-0 flex-1">
        <p class="eyebrow" role="status">{playback.status === 'idle' ? 'Native player' : playback.status}</p>
        <h2 id="player-title" class="mt-1 truncate text-sm font-medium">{playback.track?.title ?? 'Nothing playing yet'}</h2>
        {#if playback.activeFormat}<p class="muted mt-1 text-xs">{formatLabel(playback.activeFormat)}</p>{/if}
      </div>
      <button class="icon-button" aria-label="Stop playback" disabled={!playback.track || busy} onclick={() => control(stopPlayback)}><Square size={17} aria-hidden="true" /></button>
    </div>

    <div class="mt-5 flex items-center gap-3">
      <span class="timestamp">{time(seekDraft ?? playback.positionSeconds)}</span>
      <input class="seek-slider" type="range" aria-label="Playback position" min="0" max={playback.durationSeconds ?? 1} step="0.1" value={seekDraft ?? playback.positionSeconds} disabled={!active || !playback.durationSeconds || busy}
        oninput={(event) => { seekDraft = event.currentTarget.valueAsNumber; }}
        onchange={(event) => seek(event.currentTarget.valueAsNumber)} aria-valuetext={time(seekDraft ?? playback.positionSeconds)} />
      <span class="timestamp">{time(playback.durationSeconds)}</span>
    </div>
    <div class="muted mt-4 flex items-center justify-between gap-4 text-xs">
      <p>{playback.decodedSampleRateHz ? `${playback.decodedSampleRateHz / 1000} kHz decoded · Audio only` : 'Rust-managed playback'}</p>
      <div class="flex items-center gap-2"><Volume2 size={15} aria-hidden="true" /><input class="volume-slider" type="range" aria-label="Volume" min="0" max="100" step="1" value={volumeDraft ?? playback.volume} disabled={!ready || busy}
        oninput={(event) => { volumeDraft = event.currentTarget.valueAsNumber; }} onchange={(event) => volume(event.currentTarget.valueAsNumber)} /><span class="w-7 text-right">{Math.round(volumeDraft ?? playback.volume)}%</span></div>
    </div>
    {#if playback.error}<p class="notice error mt-4" role="alert">{playback.error.message}</p>{/if}
  </section>

  <footer class="muted mt-auto flex flex-wrap items-center justify-between gap-3 pt-8 text-[11px] leading-relaxed">
    <p>Early playback slice · Account support and automatic recovery are next.</p>
    <p>Space to pause · ← → to seek</p>
  </footer>
</main>
