<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { api, errorMessage, type ImportStatus, type StorageUsage, type Usage } from '$lib/api';
  import { formatBytes } from '$lib/format';
  import { announce } from '$lib/postActions';
  import { currentSite } from '$lib/sites';
  import { library } from '$lib/state/library.svelte';
  import { rememberedSearch, searchHref } from '$lib/state/search.svelte';
  import { settings } from '$lib/state/settings.svelte';
  import { toast } from '$lib/state/toast.svelte';
  import { listen } from '$lib/engine/client';
  import { onMount } from 'svelte';
  import ApiGroup from './ApiGroup.svelte';
  import AlgorithmGroup from './AlgorithmGroup.svelte';
  import AppearanceGroup from './AppearanceGroup.svelte';
  import BlacklistPanel from './BlacklistPanel.svelte';
  import DebugGroup from './DebugGroup.svelte';
  import GroupColumns from '../GroupColumns.svelte';
  import LoadingBar from '../LoadingBar.svelte';
  import SettingsGroup from '../SettingsGroup.svelte';
  import SettingsRow from '../SettingsRow.svelte';

  const NOTE = 'px-3 py-2.5 text-sm';
  const ERROR_MS = 8000;

  let usage = $state<StorageUsage>();
  let usageError = $state<string>();
  let importStatus = $state<ImportStatus>();
  let importUser = $state('');
  let fileInput = $state<HTMLInputElement>();
  let backupInput = $state<HTMLInputElement>();
  let backupStep = $state<string | null>(null);
  let working = $state<string | null>(null);
  const onGelbooru = $derived(settings.site === 'gelbooru');

  $effect(() => {
    importUser = onGelbooru ? settings.userId : settings.danbooruLogin;
  });

  function refreshUsage() {
    api.storageUsage().then(
      (found) => (usage = found),
      (error) => (usageError = errorMessage(error)),
    );
  }

  onMount(() => {
    refreshUsage();
    api.importStatus().then((status) => (importStatus = status));
    const unlisten = listen<ImportStatus>('import', (status) => (importStatus = status));
    return () => unlisten();
  });

  const describe = (found: Usage | undefined) =>
    found ? `${found.files} files · ${formatBytes(found.bytes)}` : (usageError ?? '…');

  const showError = (error: unknown) => toast.show(errorMessage(error), ERROR_MS);

  async function act(action: () => Promise<unknown>, doing: string | null = null) {
    working = doing;
    try {
      await action();
    } catch (error) {
      showError(error);
    } finally {
      working = null;
    }
  }

  async function importFile() {
    const file = fileInput?.files?.[0];
    if (!file) return;
    await act(async () => api.importFromFile(file.name, await file.text()));
    if (fileInput) fileInput.value = '';
  }

  async function importBackup() {
    const file = backupInput?.files?.[0];
    if (!file) {
      backupStep = 'the picker closed without a file — try a normal tab or another browser';
      return;
    }
    backupStep = `picked ${file.name} (${formatBytes(file.size)})`;
    let bytes: Uint8Array;
    try {
      // read before the confirm dialog: Android can lose access to the picked file while it is open
      bytes = new Uint8Array(await file.arrayBuffer());
    } catch {
      backupStep = 'the browser could not read that file';
      showError(new Error('Could not read that file. On Android, move it out of Downloads (for example to Documents) and pick it again.'));
      return;
    }
    if (!confirm(`Replace all your databases and settings with ${file.name} (${formatBytes(file.size)})? Your current data will be lost.`)) {
      backupStep = 'cancelled';
      return;
    }
    await act(async () => {
      await api.importData(bytes);
      location.reload();
    }, `Importing ${file.name}…`);
  }

  function chooseBackup() {
    // clear first so picking the same file again still fires change
    if (backupInput) backupInput.value = '';
    backupStep = 'waiting for the picker…';
  }

  async function confirmed(message: string, action: () => Promise<unknown>, doing: string) {
    if (!confirm(message)) return;
    await act(action, doing);
    refreshUsage();
  }

  async function resetSettings() {
    localStorage.removeItem('settings.json');
  }

  async function resetEverything() {
    await api.resetAllDatabases();
    await resetSettings();
  }

  function reloadAfter(action: () => Promise<unknown>) {
    return async () => {
      await action();
      location.reload();
    };
  }
</script>

<GroupColumns>
  <SettingsGroup title="Your posts">
    <SettingsRow icon="❤️" label="Liked posts" onclick={() => goto(searchHref('/likes', rememberedSearch('/likes')))} />
    <SettingsRow icon="💔" label="Disliked posts" onclick={() => goto(searchHref('/disliked', rememberedSearch('/disliked')))} />
  </SettingsGroup>

  <AlgorithmGroup />

  <SettingsGroup title="Activity">
    <SettingsRow icon="📜" label="View activity" onclick={() => goto(resolve('/activity'))} />
    <SettingsRow icon="🗃️" label="Database info" onclick={() => goto(resolve('/database'))} />
  </SettingsGroup>

  <DebugGroup />

  <AppearanceGroup />

  <ApiGroup />

  <SettingsGroup title="Import favourites">
    {#if importStatus?.running}
      <li class={[NOTE, 'flex flex-col gap-1']}>
        <span>Importing from {importStatus.source}…</span>
        <progress class="progress w-full" value={importStatus.imported + importStatus.skipped} max={importStatus.total ?? undefined}></progress>
        {#if importStatus.total !== null}
          <span class="text-xs opacity-60">{importStatus.imported + importStatus.skipped} of {importStatus.total}</span>
        {/if}
      </li>
    {:else}
      <SettingsRow icon="📥" label="{currentSite().label} user">
        <input
          class="input input-sm w-28"
          inputmode={onGelbooru ? 'numeric' : 'text'}
          placeholder={onGelbooru ? 'User ID' : 'Username'}
          aria-label="{currentSite().label} user"
          bind:value={importUser}
        />
        <button class="btn btn-sm" disabled={!importUser.trim()} onclick={() => act(() => api.importFavourites(importUser.trim()))}>Import</button>
      </SettingsRow>
      {#if onGelbooru}
        <SettingsRow icon="📄" label="Booruview export">
          <input bind:this={fileInput} type="file" accept=".json,application/json" class="hidden" onchange={importFile} />
          <button class="btn btn-sm" onclick={() => fileInput?.click()}>📂 Choose file</button>
        </SettingsRow>
      {/if}
      {#if importStatus?.error}
        <li class={[NOTE, 'text-error']}>Import stopped after {importStatus.imported}: {importStatus.error}</li>
      {:else if importStatus?.source}
        <li class={[NOTE, 'opacity-70']}>
          Last import: {importStatus.imported} likes from {importStatus.source}{importStatus.skipped ? ` (${importStatus.skipped} not found)` : ''}.
        </li>
      {/if}
      <SettingsRow icon="🗑️" label="Clear all likes">
        <button
          class="btn btn-sm"
          disabled={working !== null}
          onclick={() => confirmed('Remove all your likes?', () => library.clearLikes(), 'Clearing likes…')}>Clear</button
        >
      </SettingsRow>
    {/if}
  </SettingsGroup>

  <SettingsGroup title="Storage">
    <SettingsRow icon="🗄️" label="{currentSite().label} database" detail={describe(usage?.database)}>
      <button
        class="btn btn-sm"
        disabled={working !== null}
        onclick={() =>
          confirmed(
            `Delete all ${currentSite().label} posts, likes, dislikes, favourite tags, saved searches and the blacklist?`,
            reloadAfter(api.resetDatabase),
            `Resetting the ${currentSite().label} database…`,
          )}
        >Reset</button
      >
    </SettingsRow>
    <SettingsRow icon="⚙️" label="Settings" detail={describe(usage?.settings)}>
      <button
        class="btn btn-sm"
        disabled={working !== null}
        onclick={() => confirmed('Reset settings?', reloadAfter(resetSettings), 'Resetting settings…')}>Reset</button
      >
    </SettingsRow>
  </SettingsGroup>

  <SettingsGroup title="Backup">
    <SettingsRow icon="📤" label="Export data" detail="Both databases and all settings, including logins. Saved to your save location">
      <button class="btn btn-sm" disabled={working !== null} onclick={() => act(async () => announce(await api.exportData()), 'Exporting data…')}
        >Export</button
      >
    </SettingsRow>
    <SettingsRow
      icon="📥"
      label="Import data"
      detail={backupStep ?? 'Replaces everything with a backup file'}
    >
      <input
        bind:this={backupInput}
        type="file"
        class="file-input file-input-sm"
        aria-label="Backup file"
        disabled={working !== null}
        onclick={chooseBackup}
        onchange={importBackup}
        oncancel={() => (backupStep = 'the picker was closed — try a normal tab or another browser')}
      />
    </SettingsRow>
  </SettingsGroup>

  <SettingsGroup title="Blacklist">
    <li class={NOTE}><BlacklistPanel /></li>
  </SettingsGroup>
</GroupColumns>

{#if working}<LoadingBar text={working} />{/if}

<button
  class="btn btn-outline btn-error btn-sm mt-3 w-full"
  disabled={working !== null}
  onclick={() => confirmed('Reset EVERYTHING?.', reloadAfter(resetEverything), 'Resetting everything…')}
>
  Reset everything
</button>
