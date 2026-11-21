<script lang="ts">
  import { page } from '$app/state';
  import { appPath, closeLayer, openLayer, runSearch } from '$lib/navigation';
  import { algorithmPageFor } from '$lib/algorithm';
  import { keepFocus } from '$lib/attachments';
  import { ARTISTS_PAGE, searchPlaceholder, staged } from '$lib/state/search.svelte';
  import { untrack } from 'svelte';
  import { fade, fly, scale } from 'svelte/transition';
  import AlgorithmControls from '../AlgorithmControls.svelte';
  import ArtistFilterButton from '../ArtistFilterButton.svelte';
  import TagInput from '../TagInput.svelte';
  import SearchFilters from '../sidebar/SearchFilters.svelte';
  import SearchTools from '../sidebar/SearchTools.svelte';

  const SCROLL_NOISE_PX = 4;
  const OPEN_MS = 180;

  let input = $state<ReturnType<typeof TagInput>>();
  let barHeight = $state(0);
  let scrolledDown = $state(false);
  let lastY = 0;
  // set during Back / Forward so a restored search layer doesn't pop the keyboard
  let fromHistory = false;

  const searching = $derived(page.state.searching === true);
  const hidden = $derived(scrolledDown && !searching);
  const algorithmPage = $derived(algorithmPageFor(appPath()));

  $effect(() => {
    if (searching) untrack(() => !fromHistory && input?.focus());
    else untrack(() => input?.blur());
  });

  // lets the jump buttons sit under the bar, or at the screen edge once it slides away
  $effect(() => {
    document.documentElement.style.setProperty('--top-bar-offset', hidden ? '0px' : 'var(--top-bar)');
    return () => document.documentElement.style.removeProperty('--top-bar-offset');
  });

  function onscroll() {
    const delta = scrollY - lastY;
    lastY = scrollY;
    if (Math.abs(delta) >= SCROLL_NOISE_PX) scrolledDown = delta > 0 && scrollY > barHeight;
  }

  async function submit(e?: SubmitEvent) {
    e?.preventDefault();
    input?.commit();
    const query = staged.query;
    await closeLayer('searching');
    runSearch(query);
  }
</script>

<svelte:window
  {onscroll}
  onpopstate={() => {
    fromHistory = true;
    requestAnimationFrame(() => (fromHistory = false));
  }}
/>

<header
  class={['fixed inset-x-0 top-0 z-(--z-tab-bar) flex h-(--top-bar) items-center gap-1 bg-base-200 px-1 shadow transition-transform', hidden && '-translate-y-full']}
  bind:clientHeight={barHeight}
>
  {#if searching}
    <button
      class="btn btn-square btn-ghost"
      title="Clear the search"
      in:scale={{ duration: OPEN_MS, start: 0.5 }}
      onmousedown={(e) => e.preventDefault()}
      onclick={() => staged.clear()}>🧹</button
    >
  {/if}
  <form class="min-w-0 flex-1" onsubmit={submit}>
    <TagInput bind:this={input} placeholder={searchPlaceholder(appPath())} onsubmit={() => submit()} onfocus={() => openLayer('searching')} />
  </form>
  {#if searching}
    <div class="flex gap-1" in:scale={{ duration: OPEN_MS, start: 0.5 }}><SearchTools /></div>
  {:else if appPath() === ARTISTS_PAGE}
    <ArtistFilterButton />
  {:else if algorithmPage}
    <AlgorithmControls page={algorithmPage} buttonClass="btn-square btn-sm" compact />
  {/if}
</header>

{#if searching}
  <button
    class="fixed inset-x-0 top-(--top-bar) bottom-0 z-(--z-search) cursor-default bg-black/50"
    aria-label="Close the search"
    transition:fade={{ duration: OPEN_MS }}
    onclick={() => closeLayer('searching')}
  ></button>
  <form
    class="fixed inset-x-0 top-(--top-bar) z-(--z-search) flex max-h-[calc(100dvh-var(--top-bar))] flex-col gap-2 overflow-y-auto rounded-b-box bg-base-100 p-3 shadow-lg"
    transition:fly={{ y: -16, duration: OPEN_MS }}
    {@attach keepFocus}
    onsubmit={submit}
  >
    <SearchFilters />
  </form>
{/if}
