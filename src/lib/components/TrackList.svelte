<script lang="ts">
  import { Clock3, Heart, LoaderCircle, Pause, Play, AudioLines } from '@lucide/svelte';
  import { trackTime, type Track } from '../native';
  import Artwork from './Artwork.svelte';

  let {
    tracks,
    activeId,
    playing,
    pendingId,
    likedIds,
    likesDisabled = false,
    likeDestination = 'Saved only on this device',
    onplay,
    onlike,
    onartist,
  }: {
    tracks: Track[];
    activeId?: string;
    playing: boolean;
    pendingId?: string;
    likedIds: Set<string>;
    likesDisabled?: boolean;
    likeDestination?: string;
    onplay: (track: Track) => void;
    onlike: (track: Track) => void;
    onartist: (artist: string) => void;
  } = $props();
</script>

<table class="track-table">
  <thead
    ><tr
      ><th class="number-cell">#</th><th>Title</th><th class="album-cell">Album</th><th
        class="time-cell"><Clock3 size={14} aria-label="Duration" /></th
      ><th class="save-cell"><span class="sr-only">Save</span></th></tr
    ></thead
  >
  <tbody>
    {#each tracks as track, index (track.id)}
      <tr class:current={track.id === activeId} class:pending={track.id === pendingId}>
        <td class="number-cell">
          {#if track.id === pendingId}<LoaderCircle
              size={15}
              class="spin"
              aria-label="Getting song ready"
            />
          {:else if track.id === activeId && playing}<AudioLines size={15} aria-label="Playing" />
          {:else}<span>{index + 1}</span>{/if}
        </td>
        <td
          ><div class="song-cell">
            <button
              class="song-cover"
              onclick={() => onplay(track)}
              aria-label={`${track.id === activeId && playing ? 'Pause' : 'Play'} ${track.title}`}
            >
              <Artwork url={track.thumbnailUrl} />
              <span class="cover-action"
                >{#if track.id === activeId && playing}<Pause
                    size={15}
                    fill="currentColor"
                  />{:else}<Play size={15} fill="currentColor" />{/if}</span
              >
            </button>
            <div class="song-copy">
              <button class="song-title" onclick={() => onplay(track)}>{track.title}</button><button
                class="song-artist"
                onclick={() => onartist(track.artist)}>{track.artist}</button
              >
            </div>
          </div></td
        >
        <td class="album-cell"><span>{track.album ?? '—'}</span></td>
        <td class="time-cell">{trackTime(track.durationSeconds)}</td>
        <td class="save-cell"
          ><button
            class="icon-button like-button"
            class:liked={likedIds.has(track.id)}
            aria-label={`${likedIds.has(track.id) ? 'Unsave' : 'Save'} ${track.title}`}
            aria-pressed={likedIds.has(track.id)}
            disabled={likesDisabled}
            title={likeDestination}
            onclick={() => onlike(track)}
            ><Heart size={16} fill={likedIds.has(track.id) ? 'currentColor' : 'none'} /></button
          ></td
        >
      </tr>
    {/each}
  </tbody>
</table>
