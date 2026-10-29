<script lang="ts">
  import SoundCloudMark from './SoundCloudMark.svelte';
  import type { MusicProvider } from '../native';
  let {
    provider,
    busy,
    youtubeConnected,
    soundcloudConnected,
    ontoggle,
  }: {
    provider: MusicProvider;
    busy: boolean;
    youtubeConnected: boolean;
    soundcloudConnected: boolean;
    ontoggle: () => void;
  } = $props();
</script>

<button
  class="provider-switch"
  class:soundcloud-selected={provider === 'soundcloud'}
  role="switch"
  aria-checked={provider === 'soundcloud'}
  aria-label={provider === 'soundcloud' ? 'Switch to YouTube Music' : 'Switch to SoundCloud'}
  aria-busy={busy}
  disabled={busy}
  onclick={ontoggle}
  title="Change music service · accounts stay connected"
>
  <span class="provider-switch-slider" aria-hidden="true"></span>
  <span class="provider-option" class:chosen={provider === 'youtubeMusic'}
    ><svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"
      ><path
        d="M12 0C5.376 0 0 5.376 0 12s5.376 12 12 12 12-5.376 12-12S18.624 0 12 0zm0 19.104c-3.924 0-7.104-3.18-7.104-7.104S8.076 4.896 12 4.896s7.104 3.18 7.104 7.104-3.18 7.104-7.104 7.104zm0-13.332c-3.432 0-6.228 2.796-6.228 6.228S8.568 18.228 12 18.228s6.228-2.796 6.228-6.228S15.432 5.772 12 5.772zM9.684 15.54V8.46L15.816 12l-6.132 3.54z"
      /></svg
    ><span>YouTube Music</span>{#if youtubeConnected}<i
        class="provider-connected"
        title="YouTube Music connected"
      ></i>{/if}</span
  >
  <span class="provider-option" class:chosen={provider === 'soundcloud'}
    ><SoundCloudMark size={18} /><span>SoundCloud</span>{#if soundcloudConnected}<i
        class="provider-connected"
        title="SoundCloud connected"
      ></i>{/if}</span
  >
</button>
