<script lang="ts">
  import { DISCOVER_KEYS, type AlgorithmPage } from '$lib/algorithm';
  import { STAR_PROHIBITED } from '$lib/emoji';
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

<!-- shared by For you and the feed -->
<button
  class={['btn font-normal', buttonClass, settings.hideLiked ? 'btn-primary' : idleClass]}
  title="Leave out posts you've already liked"
  aria-pressed={settings.hideLiked}
  aria-label="Hide liked"
  onclick={() => (settings.hideLiked = !settings.hideLiked)}
>
  <img class="size-[1.125rem]" src={STAR_PROHIBITED} alt="" />
  {#if !compact}Hide liked{settings.hideLiked ? ' ✓' : ''}{/if}
</button>
