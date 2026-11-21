<script lang="ts">
  import { appPath, runSearch } from '$lib/navigation';
  import { ARTISTS_PAGE, staged } from '$lib/state/search.svelte';
  import TagInput from '../TagInput.svelte';
  import ArtistSearch from './ArtistSearch.svelte';
  import SearchFilters from './SearchFilters.svelte';
  import SearchTools from './SearchTools.svelte';

  let input = $state<ReturnType<typeof TagInput>>();

  function submit(e?: SubmitEvent) {
    e?.preventDefault();
    input?.commit();
    runSearch(staged.query);
  }
</script>

<form class="flex flex-col gap-2" onsubmit={submit}>
  {#if appPath() === ARTISTS_PAGE}<ArtistSearch onsubmit={() => submit()} />{/if}
  <div class="flex gap-1">
    <button type="button" class="btn btn-square btn-sm" title="Clear the search" onmousedown={(e) => e.preventDefault()} onclick={() => staged.clear()}>🧹</button>
    <div class="min-w-0 flex-1"><TagInput bind:this={input} onsubmit={() => submit()} /></div>
  </div>
  <div class="flex justify-end gap-1"><SearchTools /></div>
  <SearchFilters />
</form>
