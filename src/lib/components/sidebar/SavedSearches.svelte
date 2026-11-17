<script lang="ts">
  import { runSearch } from '$lib/navigation';
  import { drawerLayout } from '$lib/screen';
  import { library } from '$lib/state/library.svelte';
  import { staged } from '$lib/state/search.svelte';
  import PopoverMenu from '../PopoverMenu.svelte';
  import TagLabel from '../TagLabel.svelte';

  let anchor = $state<HTMLElement | null>(null);

  async function run(query: string) {
    anchor = null;
    await staged.load(query);
    runSearch(query);
  }
</script>

<button
  type="button"
  class="btn btn-sm px-1"
  title="Saved searches"
  disabled={!library.savedSearches.length}
  onclick={(e) => (anchor = e.currentTarget)}>{drawerLayout.current ? '▾' : '▸'}</button
>

<PopoverMenu {anchor} placement={drawerLayout.current ? 'below' : 'right'} class="menu-sm w-72" onclose={() => (anchor = null)}>
  {#each library.savedSearches as search (search.query)}
    <li class="group flex-row flex-nowrap">
      <button class="flex min-w-0 flex-1 flex-wrap gap-x-2 gap-y-0.5 text-left" title="Search for this" onclick={() => run(search.query)}>
        {#each search.tags as tag (tag.name)}<TagLabel tag={tag.name} type={tag.type} />{/each}
      </button>
      <button
        class="opacity-0 group-hover:opacity-60 hover:text-error hover:opacity-100 [@media(hover:none)]:opacity-60"
        title="Forget this search"
        onclick={() => library.toggleSavedSearch(search.query)}>×</button
      >
    </li>
  {/each}
</PopoverMenu>
