<script lang="ts">
  import { Music2 } from '@lucide/svelte';
  import { artworkSource } from '../artwork';
  let {
    url,
    className = '',
    size = 128,
    alt = '',
  }: { url?: string | null; className?: string; size?: number; alt?: string } = $props();
  let attempt = $state(0);
  let previousSource: string | undefined;
  const source = $derived(attempt === 0 ? artworkSource(url, size) : url);
  $effect(() => {
    const key = `${size}:${url ?? ''}`;
    if (key !== previousSource) {
      previousSource = key;
      attempt = 0;
    }
  });
</script>

<span class="artwork {className}">
  {#if source && attempt < 2}
    <img
      src={source}
      {alt}
      loading="lazy"
      decoding="async"
      referrerpolicy="no-referrer"
      onerror={() => {
        attempt = attempt === 0 && source !== url ? 1 : 2;
      }}
    />
  {:else}
    <Music2 size={22} strokeWidth={1.4} aria-hidden="true" />
  {/if}
</span>
