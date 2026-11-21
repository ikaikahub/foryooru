<script lang="ts">
  import { appPath, runSearch } from '$lib/navigation';
  import { ARTISTS_PAGE, searchPlaceholder, staged } from '$lib/state/search.svelte';
  import TagInput from '../TagInput.svelte';
  import SearchFilters from './SearchFilters.svelte';
  import SearchTools from './SearchTools.svelte';

  let input = $state<ReturnType<typeof TagInput>>();

  // a trial on the Artists page: just the box with the chips inside, its filters live in the page's Filter & sort
  const plain = $derived(appPath() === ARTISTS_PAGE);

  function submit(e?: SubmitEvent) {
    e?.preventDefault();
    input?.commit();
    runSearch(staged.query);
  }
</script>

<form class="flex flex-col gap-2" onsubmit={submit}>
  <div class="flex gap-1">
    <button type="button" class="btn btn-square btn-sm" title="Clear the search" onmousedown={(e) => e.preventDefault()} onclick={() => staged.clear()}>🧹</button>
    <div class="min-w-0 flex-1">
      <TagInput bind:this={input} placeholder={searchPlaceholder(appPath())} inlineChips={plain} onsubmit={() => submit()} />
    </div>
  </div>
  {#if !plain}
    <div class="flex justify-end gap-1"><SearchTools /></div>
    <SearchFilters />
  {/if}
</form>
