<script lang="ts">
  import { library } from '$lib/state/library.svelte';
  import { artistSearch, bare } from '$lib/state/search.svelte';
  import TagChip from '../TagChip.svelte';
  import TagInput from '../TagInput.svelte';

  let { onsubmit }: { onsubmit: () => void } = $props();
</script>

<!-- the Artists page's search for which artists to show, above the usual tag box that filters their posts -->
<div class="flex flex-col gap-1">
  <TagInput editor={artistSearch} placeholder="Find artists…" {onsubmit} />
  {#if artistSearch.chips.length}
    <div class="flex flex-wrap gap-1">
      {#each artistSearch.chips as chip (chip.tag)}
        <TagChip
          tag={chip.tag}
          type={chip.type}
          blacklisted={library.blacklisted.has(bare(chip.tag))}
          title="Include / exclude"
          onclick={() => artistSearch.toggleExclude(chip)}
        >
          <button
            type="button"
            class="-my-1 flex min-w-7 cursor-pointer items-center justify-center self-stretch py-1 pr-1 text-base opacity-60 hover:opacity-100"
            aria-label="Remove"
            onclick={() => artistSearch.remove(bare(chip.tag))}>×</button
          >
        </TagChip>
      {/each}
    </div>
  {/if}
</div>
