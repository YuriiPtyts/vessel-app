<script lang="ts">
  import { LoaderCircle, ListMusic, Plus } from '@lucide/svelte';
  import FeatureDialog from './FeatureDialog.svelte';
  import {
    createAccountPlaylist,
    addToAccountPlaylist,
    errorMessage,
    isCancelled,
    type AccountLibrarySnapshot,
    type PlaylistMutation,
    type PlaylistPrivacy,
    type Track,
  } from '../native';
  let {
    track,
    library,
    connected,
    accountRevision,
    onclose,
    onupdated,
    onconnect,
  }: {
    track: Track | null;
    library: AccountLibrarySnapshot;
    connected: boolean;
    accountRevision: number;
    onclose: () => void;
    onupdated: (result: PlaylistMutation, created: boolean) => void;
    onconnect: () => void;
  } = $props();
  let creating = $state(false);
  let title = $state('');
  let description = $state('');
  let privacy = $state<PlaylistPrivacy>('PRIVATE');
  let busy = $state(false);
  let failure = $state<string | null>(null);
  const editable = $derived(
    library.playlists.filter((playlist) =>
      (library.editablePlaylistIds ?? []).includes(playlist.id),
    ),
  );
  let alive = true;
  $effect(() => () => {
    alive = false;
  });
  async function save(id?: string) {
    if (busy || !connected) return;
    const revision = accountRevision;
    busy = true;
    failure = null;
    try {
      const result =
        id && track
          ? await addToAccountPlaylist(id, track.id, revision)
          : await createAccountPlaylist(
              title.trim(),
              description,
              privacy,
              track?.id ?? null,
              revision,
            );
      if (alive && revision === accountRevision) onupdated(result, !id);
    } catch (error) {
      if (alive && revision === accountRevision && !isCancelled(error))
        failure = errorMessage(error);
    } finally {
      if (alive) busy = false;
    }
  }
</script>

<FeatureDialog
  title={track ? 'Add to playlist' : 'Create playlist'}
  subtitle={track ? `${track.title} · ${track.artist}` : 'Saved in your YouTube Music account.'}
  {onclose}
>
  {#if !connected}
    <div class="feature-empty">
      <ListMusic size={30} />
      <h3>Connect to make playlists.</h3>
      <p>Playlists and songs sync with your YouTube Music account.</p>
      <button class="primary-button" onclick={onconnect}>Connect Google</button>
    </div>
  {:else}
    {#if failure}<p class="notice error-notice" role="alert">{failure}</p>{/if}
    {#if busy}<p class="feature-status" role="status">
        <LoaderCircle size={16} class="spin" />Waiting for YouTube Music to confirm…
      </p>{/if}
    {#if creating || !track}
      <form
        class="playlist-form"
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <label
          >Playlist name<input
            data-initial-focus
            type="text"
            bind:value={title}
            maxlength="100"
            required
            disabled={busy}
            placeholder="A name for this feeling"
          /></label
        >
        <label
          >Description <span>(optional)</span><textarea
            bind:value={description}
            maxlength="5000"
            rows="3"
            disabled={busy}></textarea></label
        >
        <label
          >Visibility<select bind:value={privacy} disabled={busy}
            ><option value="PRIVATE">Private · only you</option><option value="UNLISTED"
              >Unlisted · anyone with the link</option
            ><option value="PUBLIC">Public · anyone</option></select
          ></label
        >
        {#if track}<p class="feature-status">This playlist will include “{track.title}”.</p>{/if}
        <div class="feature-form-actions">
          {#if track}<button
              type="button"
              class="secondary-button"
              disabled={busy}
              onclick={() => {
                creating = false;
                failure = null;
              }}>Back</button
            >{/if}<button class="primary-button" disabled={busy || !title.trim()} type="submit"
            >{track ? 'Create and add song' : 'Create playlist'}</button
          >
        </div>
      </form>
    {:else}
      <button
        class="secondary-button new-playlist-choice"
        disabled={busy}
        onclick={() => {
          creating = true;
          failure = null;
        }}><Plus size={17} />New playlist</button
      >
      <p class="feature-status">Your editable playlists</p>
      {#if !library.playlistsReady}<p role="status">Syncing your playlists…</p>{/if}
      {#if library.playlistsError}<p role="alert">{library.playlistsError.message}</p>{/if}
      <div class="playlist-choices">
        {#each editable as playlist (playlist.id)}<button
            disabled={busy}
            onclick={() => void save(playlist.id)}
            ><ListMusic size={20} /><span>{playlist.title}</span><Plus size={16} /></button
          >{/each}
      </div>
      {#if library.playlistsReady && !editable.length}<p class="feature-status">
          No editable playlists yet. Create one above. Saved playlists from other people are
          read-only.
        </p>{/if}
    {/if}
  {/if}
</FeatureDialog>
