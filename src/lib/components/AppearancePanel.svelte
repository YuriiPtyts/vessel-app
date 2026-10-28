<script lang="ts">
  import { Check, Moon, Sun, RotateCcw, X } from '@lucide/svelte';
  import { panelReveal } from '../motion';
  import { dismissOnOutside } from '../popover';
  import {
    ACCENTS,
    DEFAULT_THEME,
    GLASS_TINT_MIN,
    GLASS_TINT_MAX,
    type AppearancePreferences,
  } from '../appearance';
  let {
    preferences,
    onchange,
    onclose,
    onreset,
  }: {
    preferences: AppearancePreferences;
    onchange: (change: Partial<AppearancePreferences>, persist?: boolean) => void;
    onclose: () => void;
    onreset: () => void;
  } = $props();
</script>

<section
  class="appearance-popover"
  data-material-overlay
  use:dismissOnOutside={{ onDismiss: onclose, trigger: '[data-popover-trigger="appearance"]' }}
  aria-label="Appearance settings"
  in:panelReveal={{ origin: 'bottom-left' }}
>
  <div class="appearance-scroll">
    <div class="popover-heading">
      <div>
        <p class="eyebrow">MAKE IT YOURS</p>
        <h2>Your space, your way.</h2>
      </div>
      <button class="icon-button" aria-label="Close appearance settings" onclick={onclose}
        ><X size={16} /></button
      >
    </div>
    <div class="theme-preview">
      <span class="theme-preview-mark"></span>
      <div>
        <strong>{DEFAULT_THEME.name}</strong><span>{DEFAULT_THEME.variant} · Default palette</span>
      </div>
      <span class="theme-default-badge">DEFAULT</span>
    </div>
    <div class="appearance-section">
      <h3>Light or dark</h3>
      <div class="appearance-modes" role="group" aria-label="Light or dark appearance">
        <button
          aria-label="Light appearance"
          aria-pressed={preferences.colorMode === 'light'}
          onclick={() => onchange({ colorMode: 'light' })}><Sun size={15} />Light</button
        >
        <button
          aria-label="Dark appearance"
          aria-pressed={preferences.colorMode === 'dark'}
          onclick={() => onchange({ colorMode: 'dark' })}><Moon size={15} />Dark</button
        >
      </div>
    </div>
    <div class="appearance-section">
      <h3>Accent color</h3>
      <div class="accent-swatches">
        {#each ACCENTS as accent (accent.id)}<button
            style:--swatch={accent.color}
            class:selected={preferences.accent === accent.id}
            aria-label={`${accent.name} accent`}
            aria-pressed={preferences.accent === accent.id}
            onclick={() => onchange({ accent: accent.id })}
            ><span
              >{#if preferences.accent === accent.id}<Check size={13} />{/if}</span
            ><small>{accent.name}</small></button
          >{/each}
      </div>
    </div>
    <div class="appearance-section">
      <div class="preference-label">
        <label for="glass-opacity">Glass tint</label><output for="glass-opacity"
          >{preferences.glassOpacity}%</output
        >
      </div>
      <input
        id="glass-opacity"
        type="range"
        min={GLASS_TINT_MIN}
        max={GLASS_TINT_MAX}
        step="1"
        value={preferences.glassOpacity}
        style:--range-progress={`${((preferences.glassOpacity - GLASS_TINT_MIN) / (GLASS_TINT_MAX - GLASS_TINT_MIN)) * 100}%`}
        disabled={preferences.reducedTransparency}
        oninput={(event) => onchange({ glassOpacity: event.currentTarget.valueAsNumber }, false)}
        onchange={() => onchange({})}
      />
      <p>
        Changes transparency only. Your light/dark choice stays the same. Text gets extra contrast
        at high transparency. At 100%, the background is solid.
      </p>
    </div>
    <button
      class="appearance-option"
      role="switch"
      aria-checked={preferences.reducedTransparency}
      onclick={() => onchange({ reducedTransparency: !preferences.reducedTransparency })}
      ><span
        ><strong>Reduce transparency</strong><small>A solid, high-clarity backdrop.</small></span
      ><span class="toggle" class:on={preferences.reducedTransparency}
        >{#if preferences.reducedTransparency}<Check size={11} />{/if}</span
      ></button
    >
    <div class="appearance-section">
      <label class="preference-label" for="interface-density">Song list spacing</label><select
        id="interface-density"
        value={preferences.density}
        onchange={(event) =>
          onchange({
            density: event.currentTarget.value === 'compact' ? 'compact' : 'comfortable',
          })}
        ><option value="comfortable">Comfortable</option><option value="compact">Compact</option
        ></select
      >
    </div>
    <button
      class="appearance-option"
      role="switch"
      aria-checked={preferences.sidebarCollapsed}
      onclick={() => onchange({ sidebarCollapsed: !preferences.sidebarCollapsed })}
      ><span
        ><strong>Compact sidebar</strong><small>Keep the icons, give music more room.</small></span
      ><span class="toggle" class:on={preferences.sidebarCollapsed}
        >{#if preferences.sidebarCollapsed}<Check size={11} />{/if}</span
      ></button
    >
    <button
      class="appearance-option"
      role="switch"
      aria-checked={preferences.showNowPlaying}
      onclick={() => onchange({ showNowPlaying: !preferences.showNowPlaying })}
      ><span
        ><strong>Now playing artwork</strong><small>Show the cover beside your music.</small></span
      ><span class="toggle" class:on={preferences.showNowPlaying}
        >{#if preferences.showNowPlaying}<Check size={11} />{/if}</span
      ></button
    >
    <button class="reset-appearance" onclick={onreset}
      ><RotateCcw size={13} />Reset appearance</button
    >
  </div>
</section>
