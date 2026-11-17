<script lang="ts">
  import { drawerLayout } from '$lib/screen';
  import type { Snippet } from 'svelte';
  import SearchPanel from './SearchPanel.svelte';
  import Sidebar from './Sidebar.svelte';

  let { children }: { children?: Snippet } = $props();
</script>

{#if !drawerLayout.current}
  <Sidebar>
    <SearchPanel />
    {#if children}
      <div class="mt-4 border-t border-base-300 pt-3">{@render children()}</div>
    {/if}
  </Sidebar>
{/if}
