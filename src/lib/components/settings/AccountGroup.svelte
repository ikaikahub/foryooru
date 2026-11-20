<script lang="ts">
  import { api, errorMessage, type SyncStatus } from '$lib/api';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import SettingsGroup from '../SettingsGroup.svelte';
  import SettingsRow from '../SettingsRow.svelte';

  const NOTE = 'px-3 py-2.5 text-sm';

  let username = $state<string | null>(null);
  let status = $state<SyncStatus>();
  let typedUsername = $state('');
  let password = $state('');
  let showPassword = $state(false);
  let working = $state(false);
  let accountError = $state<string>();

  onMount(() => {
    api.account().then((found) => (username = found));
    api.syncStatus().then((found) => (status = found));
    const unlisten = listen<SyncStatus>('sync', async (event) => {
      status = event.payload;
      // a rejected session logs the app out, so the shown username has to follow
      if (!event.payload.running) username = await api.account();
    });
    return () => unlisten.then((stop) => stop());
  });

  const canEnter = $derived(!working && typedUsername.trim() !== '' && password !== '');

  const syncDetail = $derived.by(() => {
    if (!status) return '…';
    if (status.running) return 'Syncing…';
    if (status.lastSynced === null) return 'Not synced yet';
    return `Last synced ${new Date(status.lastSynced).toLocaleString([], { dateStyle: 'short', timeStyle: 'short' })}`;
  });

  async function enter(newAccount: boolean) {
    working = true;
    accountError = undefined;
    try {
      const login = newAccount ? api.signUp : api.logIn;
      username = await login(typedUsername, password);
      password = '';
    } catch (error) {
      accountError = errorMessage(error);
    } finally {
      working = false;
    }
  }

  async function logOut() {
    working = true;
    try {
      await api.logOut();
      username = null;
    } catch (error) {
      accountError = errorMessage(error);
    } finally {
      working = false;
    }
  }
</script>

<SettingsGroup title="Account">
  {#if username}
    <SettingsRow icon="👤" label={username} detail={syncDetail}>
      <button class="btn btn-sm" disabled={working} onclick={logOut}>Log out</button>
    </SettingsRow>
    {#if status?.error}<li class={[NOTE, 'text-error']}>{status.error}</li>{/if}
  {:else}
    <SettingsRow icon="👤" label="Username">
      <input
        class="input input-sm w-36"
        autocomplete="username"
        autocapitalize="none"
        spellcheck="false"
        aria-label="Account username"
        bind:value={typedUsername}
      />
    </SettingsRow>
    <SettingsRow icon="🔒" label="Password">
      <input
        class="input input-sm w-36"
        type={showPassword ? 'text' : 'password'}
        autocomplete="current-password"
        aria-label="Account password"
        bind:value={password}
        onkeydown={(event) => event.key === 'Enter' && canEnter && enter(false)}
      />
      <button
        class="btn btn-ghost btn-sm btn-square"
        aria-label={showPassword ? 'Hide password' : 'Show password'}
        aria-pressed={showPassword}
        onclick={() => (showPassword = !showPassword)}>{showPassword ? '🙈' : '👁️'}</button
      >
    </SettingsRow>
    <SettingsRow label="Syncs likes, dislikes, favourite tags, the blacklist and saved searches between devices.">
      <button class="btn btn-sm" disabled={!canEnter} onclick={() => enter(true)}>Sign up</button>
      <button class="btn btn-sm btn-primary" disabled={!canEnter} onclick={() => enter(false)}>Log in</button>
    </SettingsRow>
  {/if}
  {#if accountError}<li class={[NOTE, 'text-error']}>{accountError}</li>{/if}
</SettingsGroup>
