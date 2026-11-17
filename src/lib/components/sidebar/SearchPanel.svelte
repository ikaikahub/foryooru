<script lang="ts">
  import { runSearch } from '$lib/navigation';
  import { staged } from '$lib/state/search.svelte';
  import TagInput from '../TagInput.svelte';
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
  <div class="flex gap-1">
    <div class="min-w-0 flex-1"><TagInput bind:this={input} onsubmit={() => submit()} /></div>
    <button type="button" class="btn btn-square btn-sm" title="Clear the search" onclick={() => staged.clear()}>🧹</button>
  </div>
  <div class="flex justify-end gap-1"><SearchTools /></div>
  <SearchFilters />
</form>
