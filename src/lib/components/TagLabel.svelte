<script lang="ts">
  import { imageUrl, type TagType } from '$lib/api';
  import { bare, similarId } from '$lib/state/search.svelte';
  import { tagText } from '$lib/tags';

  let { tag, type = null }: { tag: string; type?: TagType | null } = $props();

  const EXCLUDED = 'text-error line-through';

  const excluded = $derived(tag.startsWith('-'));
  const similar = $derived(similarId(tag));
</script>

{#if similar !== null}
  <span class="flex items-center gap-1">
    <img class="size-5 shrink-0 rounded object-cover" src={imageUrl('thumbnail', similar)} alt="" />
    <span class={[excluded && EXCLUDED]}>similar</span>
  </span>
{:else}
  <span class={['break-all', excluded ? EXCLUDED : tagText(type)]}>{bare(tag)}</span>
{/if}
