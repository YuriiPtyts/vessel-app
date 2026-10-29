<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X } from '@lucide/svelte';
  import { openModal } from '../modal';
  import { panelReveal } from '../motion';
  let {
    title,
    subtitle,
    onclose,
    children,
    className = '',
    sourceName = 'YouTube Music',
  }: {
    title: string;
    subtitle?: string;
    onclose: () => void;
    children: Snippet;
    className?: string;
    sourceName?: string;
  } = $props();
</script>

<dialog
  class="feature-dialog {className}"
  aria-label={title}
  data-material-overlay
  use:openModal={onclose}
  in:panelReveal
>
  <header class="feature-dialog-heading">
    <div>
      <p class="eyebrow">{sourceName.toUpperCase()}</p>
      <h2>{title}</h2>
      {#if subtitle}<p>{subtitle}</p>{/if}
    </div>
    <button class="icon-button" aria-label={`Close ${title.toLowerCase()}`} onclick={onclose}
      ><X size={18} /></button
    >
  </header>
  <div class="feature-dialog-scroll">{@render children()}</div>
</dialog>
