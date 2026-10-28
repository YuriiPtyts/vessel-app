<script lang="ts">
  import { panelReveal } from '../motion';
  import { dismissOnOutside } from '../popover';
  import {
    Check,
    ChevronUp,
    Heart,
    LoaderCircle,
    ListMusic,
    Pause,
    Play,
    SkipBack,
    SkipForward,
    SlidersHorizontal,
    Square,
    Volume2,
    Volume1,
    VolumeX,
    X,
  } from '@lucide/svelte';
  import {
    formatLabel,
    trackTime,
    type PlaybackSnapshot,
    type Track,
    type TrackInspection,
  } from '../native';
  import Artwork from './Artwork.svelte';

  let {
    playback,
    pending,
    inspection,
    busy,
    transitioning,
    liked,
    likesDisabled = false,
    likeDestination = 'YouTube Music likes',
    canPrevious,
    canNext,
    onpause,
    onseek,
    onvolume,
    onmute,
    onstop,
    oncancel,
    onprevious,
    onnext,
    onlike,
    onquality,
    onartwork,
    artworkVisible,
    queueOpen,
    onqueue,
  }: {
    playback: PlaybackSnapshot;
    pending: Track | null;
    inspection: TrackInspection | null;
    busy: boolean;
    transitioning: boolean;
    liked: boolean;
    likesDisabled?: boolean;
    likeDestination?: string;
    canPrevious: boolean;
    canNext: boolean;
    onpause: () => void;
    onseek: (position: number) => Promise<void>;
    onvolume: (value: number) => Promise<void>;
    onmute: () => void;
    onstop: () => void;
    oncancel: () => void;
    onprevious: () => void;
    onnext: () => void;
    onlike: () => void;
    onquality: (id: string) => void;
    onartwork: () => void;
    artworkVisible: boolean;
    queueOpen: boolean;
    onqueue: () => void;
  } = $props();
  let seekDraft = $state<number | null>(null);
  let volumeDraft = $state<number | null>(null);
  let qualityOpen = $state(false);
  let volumeRevision = 0;
  let volumePointerDown = $state(false);
  let volumeKeyDown = $state(false);
  const active = $derived(['playing', 'paused', 'buffering'].includes(playback.status));
  const shown = $derived(pending ?? playback.track);
  const qualityReady = $derived(
    inspection && inspection.track.id === playback.track?.id && !pending && !transitioning,
  );

  async function seek(value: number) {
    await onseek(value);
    seekDraft = null;
  }
  async function volume(value: number) {
    const revision = ++volumeRevision;
    volumeDraft = value;
    await onvolume(value);
    if (revision === volumeRevision && !volumePointerDown && !volumeKeyDown) volumeDraft = null;
  }

  function finishVolumePointer() {
    if (!volumePointerDown) return;
    volumePointerDown = false;
    if (volumeDraft !== null) void volume(volumeDraft);
  }

  function finishVolumeKey() {
    if (!volumeKeyDown) return;
    volumeKeyDown = false;
    if (volumeDraft !== null) void volume(volumeDraft);
  }

  function finishVolumeGesture() {
    if (!volumePointerDown && !volumeKeyDown) return;
    volumePointerDown = false;
    volumeKeyDown = false;
    if (volumeDraft !== null) void volume(volumeDraft);
  }
</script>

<svelte:window
  onpointerup={finishVolumePointer}
  onpointercancel={finishVolumePointer}
  onkeyup={finishVolumeKey}
/>

<section class="floating-player" aria-label="Music player" data-material-overlay>
  <div class="now-playing">
    <button
      class="player-artwork-button"
      aria-label="Show now playing artwork"
      aria-expanded={artworkVisible}
      disabled={!shown}
      onclick={onartwork}><Artwork url={shown?.thumbnailUrl} className="player-art" /></button
    >
    <div class="now-playing-copy">
      <h2>{shown?.title ?? 'A little space for your music'}</h2>
      <p>
        {pending
          ? 'Getting your song ready…'
          : (playback.track?.artist ?? 'Find something you love')}
      </p>
    </div>
    <button
      class="icon-button player-heart"
      class:liked
      aria-label={liked ? 'Unsave current song' : 'Save current song'}
      aria-pressed={liked}
      disabled={!playback.track || likesDisabled}
      title={likeDestination}
      onclick={onlike}><Heart size={16} fill={liked ? 'currentColor' : 'none'} /></button
    >
  </div>
  <div class="player-center">
    <div class="transport-controls">
      <button
        class="icon-button"
        aria-label="Previous song"
        title="Previous song · restart after 3 seconds"
        disabled={!canPrevious || busy || transitioning}
        onclick={onprevious}><SkipBack size={17} fill="currentColor" /></button
      >
      <button
        class="main-transport"
        aria-label={playback.status === 'playing' || playback.status === 'buffering'
          ? 'Pause playback'
          : 'Resume playback'}
        disabled={!active || busy || transitioning}
        onclick={onpause}
      >
        {#if playback.status === 'loading' || pending || transitioning}<LoaderCircle
            size={20}
            class="spin"
          />
        {:else if playback.status === 'playing' || playback.status === 'buffering'}<Pause
            size={18}
            fill="currentColor"
          />
        {:else}<Play size={18} fill="currentColor" />{/if}
      </button>
      <button
        class="icon-button"
        aria-label="Next song"
        title="Next song"
        disabled={!canNext || busy || transitioning}
        onclick={onnext}><SkipForward size={17} fill="currentColor" /></button
      >
    </div>
    <div class="playback-timeline">
      <span>{trackTime(seekDraft ?? playback.positionSeconds)}</span><input
        type="range"
        aria-label="Playback position"
        min="0"
        max={playback.durationSeconds ?? 1}
        step="0.1"
        value={seekDraft ?? playback.positionSeconds}
        style:--range-progress={`${Math.min(100, Math.max(0, ((seekDraft ?? playback.positionSeconds) / (playback.durationSeconds || 1)) * 100))}%`}
        disabled={!active || !playback.durationSeconds || busy || transitioning}
        oninput={(event) => {
          seekDraft = event.currentTarget.valueAsNumber;
        }}
        onchange={(event) => seek(event.currentTarget.valueAsNumber)}
        aria-valuetext={trackTime(seekDraft ?? playback.positionSeconds)}
      /><span>{trackTime(playback.durationSeconds)}</span>
    </div>
  </div>
  <div class="player-accessories">
    <div class="player-volume">
      <button
        class="icon-button volume-mute"
        aria-label={(volumeDraft ?? playback.volume) === 0 ? 'Unmute audio' : 'Mute audio'}
        aria-pressed={(volumeDraft ?? playback.volume) === 0}
        title={(volumeDraft ?? playback.volume) === 0 ? 'Unmute (M)' : 'Mute (M)'}
        onclick={onmute}
      >
        {#if (volumeDraft ?? playback.volume) === 0}<VolumeX
            size={16}
          />{:else if (volumeDraft ?? playback.volume) < 50}<Volume1 size={16} />{:else}<Volume2
            size={16}
          />{/if}
      </button><input
        type="range"
        aria-label="Volume"
        min="0"
        max="100"
        step="1"
        value={volumeDraft ?? playback.volume}
        style:--range-progress={`${volumeDraft ?? playback.volume}%`}
        aria-valuetext={`${Math.round(volumeDraft ?? playback.volume)} percent`}
        title={`${Math.round(volumeDraft ?? playback.volume)}%`}
        onpointerdown={() => {
          volumePointerDown = true;
        }}
        onkeydown={() => {
          volumeKeyDown = true;
        }}
        onblur={finishVolumeGesture}
        oninput={(event) => {
          void volume(event.currentTarget.valueAsNumber);
        }}
      />
    </div>
    <div class="quality-anchor">
      <button
        class="quality-button"
        data-popover-trigger="quality"
        aria-label="Audio quality"
        aria-expanded={qualityOpen}
        onclick={() => {
          qualityOpen = !qualityOpen;
        }}
        ><SlidersHorizontal size={15} /><span
          >{playback.activeFormat
            ? playback.activeFormat.codec === 'opus'
              ? 'OPUS'
              : 'AAC'
            : 'AUTO'}</span
        ><ChevronUp size={12} /></button
      >
      {#if qualityOpen}
        <div
          class="quality-popover"
          role="region"
          aria-label="Audio quality options"
          data-material-overlay
          use:dismissOnOutside={{
            onDismiss: () => {
              qualityOpen = false;
            },
            trigger: '[data-popover-trigger="quality"]',
          }}
          in:panelReveal
        >
          <div class="popover-heading">
            <div>
              <span class="eyebrow">YOUR SOUND</span>
              <h3>Audio quality</h3>
            </div>
            <button
              class="icon-button"
              aria-label="Close quality options"
              onclick={() => {
                qualityOpen = false;
              }}><X size={15} /></button
            >
          </div>
          {#if qualityReady && inspection}
            {#each inspection.formats as format (format.id)}
              <button
                class="quality-option"
                class:selected={format.id === playback.activeFormat?.id}
                disabled={busy}
                onclick={() => onquality(format.id)}
                ><span>{formatLabel(format)}</span
                >{#if format.id === playback.activeFormat?.id}<Check size={15} />{/if}</button
              >
            {/each}
            <p>Available source formats. Bitrates are estimates; no lossless source is verified.</p>
          {:else}<p>Play a song to see the quality available for that source.</p>{/if}
        </div>
      {/if}
    </div>
    <button
      class="icon-button queue-trigger"
      data-popover-trigger="queue"
      class:selected={queueOpen}
      aria-label="Show play queue"
      aria-expanded={queueOpen}
      title="Up next and autoplay radio"
      onclick={onqueue}><ListMusic size={17} /></button
    >
    <button
      class="icon-button stop-button"
      aria-label="Stop playback"
      disabled={(!playback.track && !pending) || busy}
      onclick={onstop}><Square size={13} /></button
    >
  </div>
  {#if pending}
    <div class="connection-status" role="status">
      <LoaderCircle size={12} class="spin" /><span
        >Getting <strong>{pending.title}</strong> ready…</span
      ><button aria-label="Cancel song loading" onclick={oncancel}><X size={13} /></button>
    </div>
  {/if}
</section>
