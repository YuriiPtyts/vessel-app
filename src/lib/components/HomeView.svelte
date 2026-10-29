<script lang="ts">
  import { ArrowUpRight, LoaderCircle, Play, RefreshCw, Radio } from '@lucide/svelte';
  import Artwork from './Artwork.svelte';
  import type { HomeFeed, HomeItem, Track } from '../native';
  let {
    feed,
    loading,
    failed,
    connected,
    pendingId,
    onrefresh,
    onplay,
    onbrowse,
    sourceName = 'YouTube Music',
  }: {
    feed: HomeFeed | null;
    loading: boolean;
    failed: boolean;
    connected: boolean;
    pendingId?: string;
    onrefresh: () => void;
    onplay: (track: Track, tracks: Track[]) => void;
    onbrowse: (item: HomeItem) => void;
    sourceName?: string;
  } = $props();
  function scrollShelf(id: string, direction: number) {
    document.getElementById(`items-${id}`)?.scrollBy({
      left: direction * 360,
      behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth',
    });
  }
</script>

<section class="provider-home-heading">
  <div>
    <p class="eyebrow">FROM {sourceName.toUpperCase()}</p>
    <h1>Listen now<span>.</span></h1>
    <p>Recommendations from {sourceName}, ready to explore.</p>
  </div>
  <div class="provider-home-actions">
    <span class="feed-scope"
      ><Radio size={12} />{feed?.personalized ? 'Your feed' : 'Guest recommendations'}</span
    ><button
      class="icon-button"
      aria-label="Refresh recommendations"
      disabled={loading || !connected}
      onclick={onrefresh}><RefreshCw size={16} class={loading ? 'spin' : ''} /></button
    >
  </div>
</section>
{#if failed && feed}<p class="notice" role="status">
    {sourceName} could not refresh the feed. Showing the last loaded recommendations.
  </p>{/if}
{#if loading && !feed}
  <div
    class="provider-home-loading"
    aria-label={`Loading ${sourceName} recommendations`}
    role="status"
  >
    {#each [1, 2] as shelf (shelf)}<section class="provider-shelf">
        <div class="skeleton shelf-title-skeleton"></div>
        <div class="provider-card-grid">
          {#each [1, 2, 3, 4] as card (card)}<div class="card-skeleton">
              <div class="skeleton"></div>
              <span class="skeleton"></span>
            </div>{/each}
        </div>
      </section>{/each}
  </div>
{:else if feed?.shelves.length}
  {#each feed.shelves as shelf (shelf.id)}
    <section class="provider-shelf" aria-labelledby={`heading-${shelf.id}`}>
      <div class="provider-shelf-heading">
        <h2 id={`heading-${shelf.id}`}>{shelf.title}</h2>
        <div class="shelf-scroll-controls">
          <button
            class="icon-button"
            aria-label={`Scroll ${shelf.title} backward`}
            onclick={() => scrollShelf(shelf.id, -1)}>‹</button
          ><button
            class="icon-button"
            aria-label={`Scroll ${shelf.title} forward`}
            onclick={() => scrollShelf(shelf.id, 1)}>›</button
          >
        </div>
      </div>
      <div id={`items-${shelf.id}`} class="provider-card-grid">
        {#each shelf.items as item (item.id)}
          <button
            class="provider-card"
            aria-label={`${item.track ? 'Play' : 'Open'} ${item.title}`}
            onclick={() => {
              if (item.track)
                onplay(
                  item.track,
                  shelf.items.flatMap((item) => (item.track ? [item.track] : [])),
                );
              else onbrowse(item);
            }}
          >
            <span class="provider-card-art"
              ><Artwork url={item.thumbnailUrl} /><span class="provider-card-action"
                >{#if pendingId === item.id}<LoaderCircle
                    size={20}
                    class="spin"
                  />{:else if item.track}<Play size={18} fill="currentColor" />{:else}<ArrowUpRight
                    size={18}
                  />{/if}</span
              ><span class="provider-card-kind"
                >{item.kind === 'video' ? 'Music video' : item.kind}</span
              ></span
            ><strong>{item.title}</strong><span class="provider-card-subtitle">{item.subtitle}</span
            >
          </button>
        {/each}
      </div>
    </section>
  {/each}
{:else}
  <div class="empty-collection">
    <Radio size={32} strokeWidth={1.2} />
    <h2>{failed ? 'Your feed is taking a moment.' : 'A world of music awaits.'}</h2>
    <p>
      {failed
        ? `${sourceName} could not load guest recommendations. Search is still available.`
        : connected
          ? 'No guest recommendations are available right now.'
          : `Open the desktop app to load the ${sourceName} feed.`}
    </p>
    {#if connected}<button class="secondary-button" onclick={onrefresh}
        >Refresh recommendations<RefreshCw size={14} /></button
      >{/if}
  </div>
{/if}
