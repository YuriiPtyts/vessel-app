<script lang="ts">
  import { Check, LoaderCircle, Music2, Play, Radio, Trash2, X } from '@lucide/svelte';
  import { trackTime, type QueueSnapshot } from '../native';
  import Artwork from './Artwork.svelte';

  let {
    queue,
    onclose,
    onjump,
    onremove,
    onclear,
    onautoplay,
    onretry,
  }: {
    queue: QueueSnapshot;
    onclose: () => void;
    onjump: (index: number, videoId: string) => void;
    onremove: (index: number, videoId: string) => void;
    onclear: () => void;
    onautoplay: (enabled: boolean) => void;
    onretry: () => void;
  } = $props();
</script>

<section class="music-popover queue-popover" aria-label="Play queue">
  <div class="popover-heading">
    <div>
      <p class="eyebrow">LET THE MUSIC FIND YOU</p>
      <h2>Up next.</h2>
    </div>
    <button class="icon-button" aria-label="Close play queue" onclick={onclose}
      ><X size={17} /></button
    >
  </div>
  <button
    class="appearance-option radio-option"
    role="switch"
    aria-checked={queue.autoplay}
    onclick={() => onautoplay(!queue.autoplay)}
  >
    <span
      ><strong><Radio size={15} />Autoplay radio</strong><small
        >Similar songs from YouTube Music, after your queue.</small
      ></span
    >
    <span class="toggle" class:on={queue.autoplay}
      >{#if queue.autoplay}<Check size={11} />{/if}</span
    >
  </button>
  {#if queue.seedTitle}<p class="queue-seed">Radio from <strong>{queue.seedTitle}</strong></p>{/if}
  {#if queue.error}<div class="queue-error" role="status">
      <p>{queue.error.message}</p>
      {#if queue.autoplay && queue.running}<button
          class="text-button"
          disabled={queue.generating}
          onclick={onretry}>Retry radio</button
        >{/if}
    </div>{/if}
  {#if queue.generating}<p class="queue-progress" role="status">
      <LoaderCircle size={13} class="spin" />Finding your next songs…
    </p>{/if}
  {#if queue.tracks.length}
    <ol class="queue-tracks">
      {#each queue.tracks as track, index (track.id)}
        <li
          class:current={index === queue.currentIndex}
          class:past={queue.currentIndex !== null && index < queue.currentIndex}
        >
          <button
            class="queue-track"
            aria-label={`Play queued ${track.title}`}
            aria-current={index === queue.currentIndex ? 'true' : undefined}
            disabled={queue.transitioning || (queue.running && index === queue.currentIndex)}
            onclick={() => onjump(index, track.id)}
          >
            <Artwork url={track.thumbnailUrl} className="queue-art" size={96} />
            <span class="queue-copy"
              ><strong>{track.title}</strong><small>{track.artist}</small></span
            >
            {#if index === queue.currentIndex && queue.transitioning}<LoaderCircle
                size={14}
                class="spin"
              />
            {:else if index === queue.currentIndex && queue.running}<Music2 size={14} />
            {:else if queue.preparingId === track.id}<LoaderCircle size={13} class="spin" />
            {:else}<span class="queue-duration">{trackTime(track.durationSeconds)}</span><Play
                class="queue-play"
                size={13}
              />{/if}
          </button>
          <button
            class="icon-button queue-remove"
            aria-label={`Remove ${track.title} from queue`}
            disabled={index === queue.currentIndex || queue.transitioning}
            onclick={() => onremove(index, track.id)}><X size={13} /></button
          >
        </li>
      {/each}
    </ol>
    <div class="queue-footer">
      <span>{queue.tracks.length} songs{!queue.running ? ' · Queue saved' : ''}</span><button
        class="text-button"
        onclick={onclear}><Trash2 size={13} />Clear queue</button
      >
    </div>
  {:else}<div class="queue-empty">
      <Radio size={28} strokeWidth={1.3} />
      <p>Start with a song.</p>
      <small>YouTube Music will take it from there.</small>
    </div>{/if}
</section>
