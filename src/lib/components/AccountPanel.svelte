<script lang="ts">
  import { genie } from '../motion';
  import {
    ArrowUpRight,
    Check,
    LoaderCircle,
    LogOut,
    ShieldCheck,
    UserRound,
    X,
  } from '@lucide/svelte';
  import type { AccountSnapshot } from '../native';
  let {
    account,
    available,
    busy,
    onclose,
    onconnect,
    oncancel,
    onsignout,
  }: {
    account: AccountSnapshot;
    available: boolean;
    busy: boolean;
    onclose: () => void;
    onconnect: () => void;
    oncancel: () => void;
    onsignout: () => void;
  } = $props();
  const connecting = $derived(account.mode === 'signingIn' || account.mode === 'checking');
</script>

<section
  class="music-popover account-popover"
  data-material-overlay
  aria-label="Google account"
  in:genie={{ origin: 'top-right' }}
>
  <div class="popover-heading">
    <div>
      <p class="eyebrow">YOUR MUSIC, YOUR CHOICE</p>
      <h2>Your account.</h2>
    </div>
    <button class="icon-button" aria-label="Close account settings" onclick={onclose}
      ><X size={17} /></button
    >
  </div>
  <div class="account-identity">
    <span class="account-symbol"><UserRound size={23} /></span>
    <div>
      <strong>{account.profile?.name ?? 'Listening as a guest'}</strong><small
        >{account.profile?.email ?? 'An account is always optional.'}</small
      >
    </div>
    {#if account.mode === 'connected'}<Check size={17} class="account-connected" />{/if}
  </div>
  {#if account.error}<p class="account-error" role="status">{account.error.message}</p>{/if}
  {#if connecting}<p class="account-description" role="status">
      <LoaderCircle size={15} class="spin" />{account.mode === 'checking'
        ? 'Checking your saved session…'
        : 'Finish signing in in the Google window.'}
    </p>
    <button class="secondary-button" disabled={busy} onclick={oncancel}>Cancel sign-in</button>
  {:else if account.mode === 'connected'}
    <p class="account-description">
      Your likes and saved playlists sync with YouTube Music. Home and radio can use your account’s
      recommendations.
    </p>
    <button class="secondary-button" disabled={busy} onclick={onsignout}
      ><LogOut size={15} />Sign out</button
    >
  {:else}
    <p class="account-description">
      Connect Google for your YouTube Music likes, saved playlists, and recommendations.
    </p>
    <button class="primary-button connect-google" disabled={!available || busy} onclick={onconnect}
      >{#if busy}<LoaderCircle size={15} class="spin" />{/if}{account.mode === 'expired'
        ? 'Reconnect Google'
        : 'Connect Google'}<ArrowUpRight size={15} /></button
    >
    <p class="account-caption">
      Google opens in an isolated window and remembers you on this device. If Google declines
      embedded sign-in, guest listening stays available.
    </p>
  {/if}
  {#if account.mode === 'expired' || account.mode === 'unavailable'}
    <button class="text-button forget-account" disabled={busy} onclick={onsignout}
      >Forget saved session</button
    >
  {/if}
  <div class="account-privacy">
    <ShieldCheck size={17} />
    <p>
      Your verified session is protected by your operating system and restored on launch. Sign out
      clears remembered Google sign-in; local liked songs, history, and appearance stay on this
      device.
    </p>
  </div>
</section>
