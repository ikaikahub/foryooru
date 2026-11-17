<script lang="ts">
  import { DISCOVER_KEYS, type AlgorithmPage } from '$lib/algorithm';
  import { settings } from '$lib/state/settings.svelte';

  let {
    page,
    buttonClass = 'btn-sm',
    idleClass = '',
    compact = false,
  }: { page: AlgorithmPage; buttonClass?: string; idleClass?: string; compact?: boolean } = $props();

  const key = $derived(DISCOVER_KEYS[page]);
</script>

<button
  class={['btn font-normal', buttonClass, settings[key] ? 'btn-primary' : idleClass]}
  title="Show posts next to your tastes that are new to you"
  aria-pressed={settings[key]}
  aria-label="Discover"
  onclick={() => (settings[key] = !settings[key])}
>
  {#if compact}🧭{:else}🧭 Discover{settings[key] ? ' ✓' : ''}{/if}
</button>
