<script lang="ts">
  import SectionTitle from '../SectionTitle.svelte';
  import { library } from '$lib/state/library.svelte';
  import { RATING_BUTTONS, bare, staged } from '$lib/state/search.svelte';
  import { tagMenu } from '$lib/state/tagMenu.svelte';
  import TagChip from '../TagChip.svelte';
</script>

{#if staged.chips.length}
  <div class="flex flex-wrap gap-1">
    {#each staged.chips as chip (chip.tag)}
      <TagChip tag={chip.tag} type={chip.type} title="Include / exclude" onclick={() => staged.toggleExclude(chip)}>
        {@const starred = library.starred.has(bare(chip.tag))}
        <button
          type="button"
          class={['cursor-pointer px-0.5', starred ? 'text-warning hover:opacity-60' : 'opacity-60 hover:text-warning hover:opacity-100']}
          title={starred ? 'Remove from favourite tags' : 'Favourite tag'}
          onclick={() => library.toggleTag('favouriteTags', bare(chip.tag))}
        >
          {starred ? '★' : '☆'}
        </button>
        <button
          type="button"
          class="cursor-pointer pr-2 pl-1 opacity-60 hover:opacity-100"
          aria-label="Remove"
          onclick={() => staged.remove(bare(chip.tag))}>×</button
        >
      </TagChip>
    {/each}
  </div>
{/if}

<div class="flex gap-1">
  {#each RATING_BUTTONS as rating (rating)}
    {@const chip = `rating:${rating}`}
    <button
      type="button"
      class={['btn btn-sm min-w-0 flex-1 px-1 text-xs capitalize', staged.state(chip) === 'include' && 'btn-primary']}
      title={rating}
      onclick={() => staged.toggle(chip)}><span class="truncate">{rating}</span></button
    >
  {/each}
</div>

<button class="btn btn-primary btn-sm">Search</button>

<SectionTitle>Favourite tags</SectionTitle>
{#if library.favouriteTags.length}
  <div class="flex flex-wrap gap-1">
    {#each library.favouriteTags as tag (tag.name)}
      <TagChip
        tag={tag.name}
        type={tag.type}
        title="Search, include, exclude…"
        onclick={(e) => tagMenu.show(e.currentTarget as HTMLElement, tag.name, tag.type, staged)}
      >
        <button
          type="button"
          class="cursor-pointer pr-2 pl-1 text-warning hover:opacity-60"
          title="Remove from favourite tags"
          onclick={() => library.toggleTag('favouriteTags', tag.name)}>★</button
        >
      </TagChip>
    {/each}
  </div>
{:else}
  <p class="text-sm opacity-70">Star a tag (☆ on a search tag, or from a post's tag menu) to keep it here.</p>
{/if}
