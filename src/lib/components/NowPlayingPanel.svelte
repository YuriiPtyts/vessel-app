<script lang="ts">
  import { AudioLines, Heart, X } from '@lucide/svelte';
  import { panelReveal } from '../motion';
  import Artwork from './Artwork.svelte';
  import { formatLabel, type PlaybackSnapshot, type Track } from '../native';
  let {
    track,
    playback,
    loading,
    liked,
    likesDisabled = false,
    likeDestination = 'YouTube Music likes',
    onclose,
    onlike,
  }: {
    track: Track;
    playback: PlaybackSnapshot;
    loading: boolean;
    liked: boolean;
    likesDisabled?: boolean;
    likeDestination?: string;
    onclose: () => void;
    onlike: () => void;
  } = $props();
</script>

<aside
  class="now-playing-panel"
  aria-label="Now playing artwork"
  data-material-overlay
  in:panelReveal
>
  <div class="now-playing-panel-heading">
    <span class="eyebrow">NOW PLAYING</span><button
      class="icon-button"
      aria-label="Close now playing artwork"
      onclick={onclose}><X size={16} /></button
    >
  </div>
  <div class="large-artwork-frame">
    <Artwork
      url={track.thumbnailUrl}
      size={512}
      alt={`Album artwork for ${track.title}`}
      className="large-track-art"
    />
  </div>
  <div class="artwork-track-heading">
    <div>
      <h2>{track.title}</h2>
      <p>{track.artist}</p>
    </div>
    <button
      class="icon-button"
      class:liked
      aria-label={liked ? 'Unsave artwork song' : 'Save artwork song'}
      aria-pressed={liked}
      disabled={likesDisabled}
      title={likeDestination}
      onclick={onlike}><Heart size={18} fill={liked ? 'currentColor' : 'none'} /></button
    >
  </div>
  {#if track.album}<div class="album-detail">
      <span class="eyebrow">ALBUM</span>
      <p>{track.album}</p>
    </div>{/if}
  <div class="listening-detail">
    <AudioLines size={15} /><span
      >{loading
        ? 'Getting your song ready…'
        : playback.status === 'paused'
          ? 'A moment of quiet'
          : playback.status === 'playing'
            ? 'Enjoy the moment'
            : 'Your music, kept close'}</span
    >
  </div>
  {#if !loading && playback.activeFormat}<p class="artwork-source-quality">
      {formatLabel(playback.activeFormat)}
    </p>{/if}
  <p class="artwork-footer">A little more room for the music.</p>
</aside>
