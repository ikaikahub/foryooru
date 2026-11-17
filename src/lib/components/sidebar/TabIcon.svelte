<script lang="ts">
  import type { Tab } from '$lib/tabs';

  let { tab, class: className = 'size-4' }: { tab: Tab; class?: string } = $props();
</script>

{#if 'image' in tab}<img class={className} src={tab.image} alt="" />{:else}<span>{tab.emoji}</span>{/if}
