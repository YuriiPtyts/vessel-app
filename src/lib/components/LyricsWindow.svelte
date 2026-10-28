<script lang="ts">
  import { LoaderCircle, TextQuote } from '@lucide/svelte';
  import FeatureDialog from './FeatureDialog.svelte';
  import { getSongLyrics, errorMessage, isCancelled, type SongLyrics, type Track } from '../native';
  let {
    track,
    accountRevision,
    onclose,
  }: { track: Track; accountRevision: number; onclose: () => void } = $props();
  let lyrics = $state<SongLyrics | null>(null);
  let loading = $state(true);
  let failure = $state<string | null>(null);
  let retry = $state(0);
  const videoId = $derived(track.id);
  $effect(() => {
    const id = videoId;
    const revision = accountRevision;
    void retry;
    let current = true;
    lyrics = null;
    loading = true;
    failure = null;
    void getSongLyrics(id)
      .then((result) => {
        if (current && revision === accountRevision && result.videoId === id) lyrics = result;
      })
      .catch((error) => {
        if (current && !isCancelled(error)) failure = errorMessage(error);
      })
      .finally(() => {
        if (current) loading = false;
      });
    return () => {
      current = false;
    };
  });
</script>

<FeatureDialog
  title="Lyrics"
  subtitle={`${track.title} · ${track.artist}`}
  className="lyrics-window"
  {onclose}
>
  {#if loading}<div class="feature-empty" role="status">
      <LoaderCircle size={28} class="spin" />
      <p>Finding lyrics…</p>
    </div>
  {:else if failure}<div class="feature-empty">
      <p role="alert">{failure}</p>
      <button class="secondary-button" onclick={() => retry++}>Try again</button>
    </div>
  {:else if lyrics?.text}<div class="lyrics-copy">{lyrics.text}</div>
    {#if lyrics.source}<p class="lyrics-source">{lyrics.source}</p>{/if}
  {:else}<div class="feature-empty" role="status">
      <TextQuote size={32} />
      <h3>No lyrics available for this song.</h3>
      <p>YouTube Music has not supplied lyrics for this recording.</p>
    </div>{/if}
</FeatureDialog>
