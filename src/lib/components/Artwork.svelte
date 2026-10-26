<script lang="ts">
  import { Music2 } from '@lucide/svelte';
  let { url, className = '' }: { url?: string | null; className?: string } = $props();
  let failed = $state(false);
  $effect(() => {
    url;
    failed = false;
  });
</script>

<span class="artwork {className}">
  {#if url && !failed}
    <img
      src={url}
      alt=""
      loading="lazy"
      decoding="async"
      referrerpolicy="no-referrer"
      onerror={() => {
        failed = true;
      }}
    />
  {:else}
    <Music2 size={22} strokeWidth={1.4} aria-hidden="true" />
  {/if}
</span>
