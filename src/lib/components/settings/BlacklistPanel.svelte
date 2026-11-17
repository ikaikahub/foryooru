<script lang="ts">
  import { library } from '$lib/state/library.svelte';
  import TagChip from '../TagChip.svelte';

  let typed = $state('');

  async function add(e: SubmitEvent) {
    e.preventDefault();
    await library.addTags('blacklist', typed);
    typed = '';
  }
</script>

<form class="mb-2 flex gap-1" onsubmit={add}>
  <input
    class="input input-sm min-w-0 flex-1"
    placeholder="Add tags…"
    aria-label="Tags to blacklist"
    autocomplete="off"
    autocapitalize="none"
    spellcheck="false"
    bind:value={typed}
  />
  <button class="btn btn-sm" disabled={!typed.trim()}>Add</button>
</form>

{#if library.blacklist.length}
  <div class="flex flex-wrap gap-1">
    {#each library.blacklist as tag (tag.name)}
      <TagChip tag={`-${tag.name}`} type={tag.type}>
        <button
          type="button"
          class="cursor-pointer pr-2 pl-1 opacity-60 hover:opacity-100"
          title="Remove from the blacklist"
          onclick={() => library.toggleTag('blacklist', tag.name)}>×</button
        >
      </TagChip>
    {/each}
  </div>
{:else}
  <p class="text-sm opacity-70">Empty. Add tags here, or from a tag's menu in the post viewer.</p>
{/if}
<p class="mt-2 text-sm opacity-70">
  Posts with these tags are never shown, anywhere. It's only a filter: recommendations don't learn from it. Changes apply to the next page you load.
</p>
