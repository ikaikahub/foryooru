<script lang="ts">
  import { api, errorMessage } from '$lib/api';
  import { BUILD_LABEL } from '$lib/build';
  import { debugReport } from '$lib/debug';
  import { settings } from '$lib/state/settings.svelte';
  import { toast } from '$lib/state/toast.svelte';
  import SettingsGroup from '../SettingsGroup.svelte';
  import SettingsRow from '../SettingsRow.svelte';

  let copying = $state(false);

  $effect(() => {
    api.setTimings(settings.debug).catch(() => {});
  });

  async function copyLogs() {
    copying = true;
    try {
      await navigator.clipboard.writeText(debugReport(await api.timings(), settings.site));
      toast.show('Debug logs copied');
    } catch (error) {
      toast.show(errorMessage(error));
    } finally {
      copying = false;
    }
  }
</script>

{#snippet build()}{BUILD_LABEL}{/snippet}

<SettingsGroup title="Debug" trailing={build}>
  <SettingsRow icon="🐞" label="Log timings" detail="Records slow commands, fetches and database writes so they can be copied">
    <input type="checkbox" class="toggle toggle-sm" aria-label="Log timings" bind:checked={settings.debug} />
  </SettingsRow>
  {#if settings.debug}
    <SettingsRow icon="📋" label="Copy logs" detail="Everything since the engine started. Clears when copied.">
      <button class="btn btn-sm" disabled={copying} onclick={copyLogs}>Copy</button>
    </SettingsRow>
  {/if}
</SettingsGroup>
